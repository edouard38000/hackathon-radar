//! The setup skill for coding agents (ADR 0007) stays in sync with the CLI it
//! drives. It is prose for an Agent to follow, so nothing compiles it; these
//! tests read it instead:
//!
//! - every `hackathon-radar` flag it mentions exists in that command's clap
//!   definition (read from the built binary's `--help`), and every question
//!   `init` asks has its flag in the skill, so a renamed or added flag fails
//!   here instead of silently breaking setup;
//! - no secret ever goes on a command line;
//! - its frontmatter parses, and keeps to the Agent Skills fields;
//! - every complete `init` command it shows, run in order against one fresh
//!   Profile with a fake keychain, fake programs and a fake network, does
//!   what the skill says: `--yes` commands ask nothing, and the commands the
//!   person runs ask for nothing but secrets.
//!
//! No test touches the network, the real keychain or a real Agent.

mod support;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use clap::Parser;
use hackathon_radar::alert::photon::Sidecar;
use hackathon_radar::init::{self, InitEnv, InitFlags, Terminal};
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::{AlertChannelSetting, Profile};
use hackathon_radar::secrets::{SecretName, Secrets};
use serde::Deserialize;
use support::*;
use tempfile::TempDir;

const SKILL_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.claude/skills/setup-hackathon-radar"
);
const BINARY: &str = env!("CARGO_BIN_EXE_hackathon-radar");

/// Flags of other programs the skill names on their own, in inline code.
const OTHER_PROGRAMS_FLAGS: &[&str] = &["--dangerously-skip-permissions"];

/// `init` flags the skill has no reason to mention: where the files are, and
/// skipping `npm ci`.
const INIT_FLAGS_NOT_IN_THE_SKILL: &[&str] = &["--profile", "--db", "--skip-npm-ci", "--help"];

/// Words that mark a flag as carrying a secret.
const SECRET_WORDS: &[&str] = &["key", "token", "secret", "credential", "password"];

fn skill() -> String {
    std::fs::read_to_string(Path::new(SKILL_DIR).join("SKILL.md")).unwrap()
}

/// The YAML between the opening and closing `---`, and the rest.
fn split_frontmatter(text: &str) -> (&str, &str) {
    let rest = text
        .strip_prefix("---\n")
        .expect("SKILL.md must start with a `---` line, or the frontmatter is ignored");
    let end = rest
        .find("\n---\n")
        .expect("the frontmatter needs a closing `---` line");
    (&rest[..end], &rest[end + 5..])
}

/// The code in the skill: each line of its fenced blocks (with `\`
/// continuations joined), then each inline code span outside them.
fn code(text: &str) -> (Vec<String>, Vec<String>) {
    let mut fenced = Vec::new();
    let mut inline = Vec::new();
    let mut in_fence = false;
    let mut pending = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            match line.trim_end().strip_suffix('\\') {
                Some(start) => pending.push_str(start),
                None => {
                    pending.push_str(line);
                    fenced.push(std::mem::take(&mut pending));
                }
            }
            continue;
        }
        let mut parts = line.split('`');
        parts.next();
        while let (Some(span), Some(_)) = (parts.next(), parts.next()) {
            inline.push(span.to_owned());
        }
        // A span left open at the end of the line has no closing `.
    }
    assert!(!in_fence, "a code fence in SKILL.md is never closed");
    (fenced, inline)
}

/// Splits a command line into words, as a POSIX shell would for the simple
/// quoting the skill uses.
fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    word.push(c);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// One `hackathon-radar …` command in the skill.
#[derive(Debug)]
struct Invocation {
    /// The subcommand, such as `["init"]` or `["schedule", "install"]`.
    path: Vec<String>,
    /// The words after it.
    args: Vec<String>,
    /// Where it came from, for messages.
    text: String,
    /// What is piped into it, if anything.
    piped_from: Option<String>,
}

impl Invocation {
    fn flags(&self) -> impl Iterator<Item = &str> {
        self.args
            .iter()
            .filter(|a| a.starts_with("--") && a.len() > 2)
            .map(|a| a.split('=').next().unwrap())
    }

    /// Complete: no `<PLACEHOLDER>` left to fill in.
    fn is_concrete(&self) -> bool {
        !self.args.iter().any(|a| a.contains(['<', '>']))
    }
}

/// Every `hackathon-radar` command in these snippets: the program's name as a
/// word of its own (not the `hackathon-radar-…` ntfy topics), up to the end of
/// that command in a pipeline or list.
fn invocations(snippets: &[String]) -> Vec<Invocation> {
    const NAME: &str = "hackathon-radar";
    let mut found = Vec::new();
    for snippet in snippets {
        let mut from = 0;
        while let Some(at) = snippet[from..].find(NAME).map(|i| from + i) {
            from = at + NAME.len();
            let before = snippet[..at].chars().next_back();
            let after = snippet[from..].chars().next();
            let starts = before.is_none_or(|c| c.is_whitespace() || "/|(".contains(c));
            let ends = after.is_none_or(char::is_whitespace);
            if !(starts && ends) {
                continue;
            }
            let rest = &snippet[from..];
            let end = rest.find(['|', ';', '&', ')', '#']).unwrap_or(rest.len());
            let words = shell_words(&rest[..end]);
            let path_len = words
                .iter()
                .take_while(|w| {
                    w.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                        && w.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                })
                .count();
            let piped_from = snippet[..at]
                .rfind('|')
                .map(|bar| snippet[..bar].trim().to_owned());
            found.push(Invocation {
                path: words[..path_len].to_vec(),
                args: words[path_len..].to_vec(),
                text: format!("{NAME}{}", &rest[..end])
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                piped_from,
            });
        }
    }
    found
}

/// The long flags `hackathon-radar <path> --help` lists, and whether each
/// takes a value.
fn help_flags(path: &[String]) -> BTreeMap<String, bool> {
    let output = Process::new(BINARY)
        .args(path)
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hackathon-radar {} --help` failed, so the skill names a command that doesn't exist:\n{}",
        path.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut flags = BTreeMap::new();
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        let line = line.trim_start();
        let line = match line.strip_prefix('-') {
            // `-h, --help`
            Some(short) if !short.starts_with('-') => match short.split_once(", ") {
                Some((_, long)) => long,
                None => continue,
            },
            Some(_) => line,
            None => continue,
        };
        let mut words = line.split_whitespace();
        let Some(flag) = words.next().filter(|f| f.starts_with("--")) else {
            continue;
        };
        let takes_value = words.next().is_some_and(|w| w.starts_with('<'));
        flags.insert(flag.to_owned(), takes_value);
    }
    assert!(
        flags.contains_key("--help"),
        "couldn't read the flags from `hackathon-radar {} --help`",
        path.join(" ")
    );
    flags
}

/// `help_flags`, asked once per command.
#[derive(Default)]
struct Help(HashMap<Vec<String>, BTreeMap<String, bool>>);

impl Help {
    fn of(&mut self, path: &[String]) -> &BTreeMap<String, bool> {
        self.0
            .entry(path.to_vec())
            .or_insert_with(|| help_flags(path))
    }
}

/// The subcommands `hackathon-radar --help` lists.
fn subcommands() -> Vec<String> {
    let output = Process::new(BINARY).arg("--help").output().unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    let list: Vec<String> = help
        .lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .map(str::to_owned)
        .collect();
    assert!(
        list.iter().any(|c| c == "init"),
        "couldn't read the subcommands:\n{help}"
    );
    list
}

fn path(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

fn names_a_secret(flag: &str) -> bool {
    SECRET_WORDS.iter().any(|w| flag.contains(w))
}

#[test]
fn every_flag_the_skill_mentions_exists() {
    let (fenced, inline) = code(&skill());
    let mut all_code = fenced.clone();
    all_code.extend(inline.iter().cloned());
    let found = invocations(&all_code);
    let mut help = Help::default();
    let mut wrong = Vec::new();

    for invocation in &found {
        let known = help.of(&invocation.path).clone();
        for flag in invocation.flags() {
            if !known.contains_key(flag) {
                wrong.push(format!(
                    "`{}`: `hackathon-radar {}` has no {flag}",
                    invocation.text,
                    invocation.path.join(" ")
                ));
            }
        }
    }

    // Flags named on their own, such as `--yes` or `--home-city <CITY>`,
    // belong to one of the subcommands.
    let any_subcommand: BTreeSet<String> = subcommands()
        .iter()
        .flat_map(|sub| help.of(std::slice::from_ref(sub)).clone().into_keys())
        .collect();
    for span in inline.iter().filter(|s| s.starts_with("--")) {
        for flag in shell_words(span)
            .iter()
            .filter(|w| w.starts_with("--") && w.len() > 2)
        {
            if flag.starts_with("--…") || OTHER_PROGRAMS_FLAGS.contains(&flag.as_str()) {
                continue;
            }
            if !any_subcommand.contains(flag) {
                wrong.push(format!("`{span}`: no hackathon-radar command has {flag}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "SKILL.md is out of date:\n{}",
        wrong.join("\n")
    );

    // The checks above aren't vacuous: the skill drives both commands.
    let drives = |sub: &str, flag: &str| {
        found
            .iter()
            .any(|i| i.path == [sub] && i.flags().any(|f| f == flag))
    };
    assert!(drives("init", "--yes"), "the skill never runs `init --yes`");
    assert!(
        drives("doctor", "--json"),
        "the skill never runs `doctor --json`"
    );
    assert!(drives("doctor", "--send-test-alert"));
}

#[test]
fn every_question_init_asks_has_its_flag_in_the_skill() {
    let text = skill();
    let missing: Vec<String> = help_flags(&path(&["init"]))
        .into_keys()
        .filter(|flag| !INIT_FLAGS_NOT_IN_THE_SKILL.contains(&flag.as_str()))
        .filter(|flag| {
            // As a whole flag, not the start of a longer one.
            !text.match_indices(flag.as_str()).any(|(at, _)| {
                !text[at + flag.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
            })
        })
        .collect();
    assert!(
        missing.is_empty(),
        "`init` has flags the setup skill never mentions, so the skill doesn't ask about them: {}",
        missing.join(", ")
    );
}

#[test]
fn no_secret_goes_on_a_command_line() {
    let text = skill();
    let (fenced, inline) = code(&text);
    let mut all_code = fenced.clone();
    all_code.extend(inline);
    let mut help = Help::default();

    // `init` itself takes no secret as a flag's value.
    for (flag, takes_value) in help.of(&path(&["init"])).clone() {
        if names_a_secret(&flag) {
            assert!(
                flag.ends_with("-stdin") && !takes_value,
                "`init {flag}` takes a secret on the command line"
            );
        }
    }

    for invocation in invocations(&all_code) {
        let known = help.of(&invocation.path).clone();
        for flag in invocation.flags() {
            if names_a_secret(flag) {
                assert!(
                    flag.ends_with("-stdin") && known.get(flag) == Some(&false),
                    "`{}` passes a secret on the command line with {flag}",
                    invocation.text
                );
            }
        }
        // A secret piped into `init` comes from a source the person
        // controls, never from a literal in the command.
        if invocation.flags().any(|f| f.ends_with("-stdin")) {
            let source = invocation.piped_from.as_deref().unwrap_or_else(|| {
                panic!("`{}` reads stdin but nothing is piped in", invocation.text)
            });
            let program = shell_words(source).into_iter().next().unwrap_or_default();
            assert!(
                !["echo", "printf", "cat"].contains(&program.as_str()),
                "`{source} | {}` writes the secret into the command",
                invocation.text
            );
        }
    }

    // Nor in an environment variable assignment.
    for name in SecretName::ALL {
        let assignment = format!("{}=", name.env_var());
        assert!(
            !text.contains(&assignment),
            "SKILL.md sets {} on a command line",
            name.env_var()
        );
    }
}

/// The frontmatter fields of the Agent Skills spec (https://agentskills.io),
/// which Claude Code reads and other agents can too. Fields only Claude Code
/// knows, such as `disable-model-invocation` (which would also stop the skill
/// from triggering on "set me up"), are refused.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Only parsed.
struct Frontmatter {
    name: String,
    description: String,
    license: Option<String>,
    compatibility: Option<String>,
    metadata: Option<BTreeMap<String, String>>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<String>,
}

#[test]
fn the_frontmatter_parses_and_triggers_on_set_me_up() {
    let text = skill();
    let (yaml, body) = split_frontmatter(&text);
    let frontmatter: Frontmatter = serde_norway::from_str(yaml).unwrap();

    let dir = Path::new(SKILL_DIR).file_name().unwrap().to_str().unwrap();
    assert_eq!(frontmatter.name, dir, "the name must match its directory");
    assert!(frontmatter.name.len() <= 64);
    assert!(
        frontmatter
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    );

    let description = &frontmatter.description;
    assert!(!description.trim().is_empty());
    // The spec's limit; Claude Code cuts its listing at 1,536 characters.
    assert!(
        description.chars().count() <= 1024,
        "the description is {} characters",
        description.chars().count()
    );
    // Claude Code shows angle brackets in a description as &lt; and &gt;.
    assert!(!description.contains(['<', '>']));
    for trigger in ["set me up", "install hackathon-radar"] {
        assert!(
            description.contains(trigger),
            "the description doesn't mention {trigger:?}"
        );
    }
    assert!(body.trim_start().starts_with("# "));
}

/// The person at the terminal: answers `init`'s hidden prompts with fake
/// secrets and feeds the lines piped on stdin, and notes any other question,
/// since a command in the skill gives every other answer as a flag.
struct SecretsOnly {
    stdin: Vec<String>,
    output: String,
    questions: Vec<String>,
}

impl Terminal for SecretsOnly {
    fn say(&mut self, text: &str) -> io::Result<()> {
        self.output.push_str(text);
        self.output.push('\n');
        Ok(())
    }

    fn ask(&mut self, prompt: &str) -> io::Result<Option<String>> {
        self.questions.push(prompt.to_owned());
        Ok(None)
    }

    fn ask_secret(&mut self, prompt: &str) -> io::Result<Option<String>> {
        self.output.push_str(prompt);
        self.output.push('\n');
        let answer = if prompt.starts_with("SerpApi key") {
            SERPAPI_KEY
        } else if prompt.starts_with("Photon project ID") {
            PHOTON_ID
        } else if prompt.starts_with("Photon project secret") {
            PHOTON_SECRET
        } else if prompt.contains("ntfy") {
            NTFY_TOKEN
        } else {
            panic!("init asked for a secret the walkthrough doesn't know: {prompt}")
        };
        Ok(Some(answer.to_owned()))
    }

    fn read_line(&mut self) -> io::Result<Option<String>> {
        Ok((!self.stdin.is_empty()).then(|| self.stdin.remove(0)))
    }
}

const SERPAPI_KEY: &str = "serp-walkthrough-key-31";
const PHOTON_ID: &str = "photon-walkthrough-id-47";
const PHOTON_SECRET: &str = "photon-walkthrough-secret-59";
const NTFY_TOKEN: &str = "ntfy-walkthrough-token-73";

/// `npm ci` succeeds and the Photon sidecar answers that it sent the message.
struct Machine {
    commands: RefCell<Vec<String>>,
}

impl ProcessRunner for Machine {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands
            .borrow_mut()
            .push(format!("{} {}", command.program, command.args.join(" ")));
        let stdout = match command.program.as_str() {
            "npm" => "added 12 packages".to_owned(),
            "node" => fixture("photon/sent.json"),
            other => panic!("init ran an unexpected program: {other}"),
        };
        Ok(Output {
            code: Some(0),
            stdout,
            stderr: String::new(),
        })
    }
}

#[derive(Parser)]
struct InitCli {
    #[arg(long)]
    profile: Option<PathBuf>,
    #[arg(long)]
    db: Option<PathBuf>,
    #[command(flatten)]
    flags: InitFlags,
}

#[test]
fn the_init_commands_in_the_skill_set_up_a_profile() {
    let (fenced, _) = code(&skill());
    let commands: Vec<Invocation> = invocations(&fenced)
        .into_iter()
        .filter(|i| i.path == ["init"] && i.is_concrete())
        .collect();
    assert!(
        commands.iter().any(|c| c.flags().any(|f| f == "--yes"))
            && commands.iter().any(|c| !c.flags().any(|f| f == "--yes"))
            && commands.iter().any(|c| c.piped_from.is_some()),
        "the skill should show a full `init --yes` command, one the person runs, and a piped one"
    );

    let dir = TempDir::new().unwrap();
    let profile_path = dir.path().join("config/profile.toml");
    let keychain = FakeKeychain::default();
    let secrets: Secrets = secrets_in(&keychain, &dir.path().join("config"), &[]);
    let http = RecordingPost::ok();
    let machine = Machine {
        commands: RefCell::new(Vec::new()),
    };
    let sidecar = Sidecar {
        dir: dir.path().join("sidecar/photon"),
    };
    std::fs::create_dir_all(&sidecar.dir).unwrap();
    let schedules = RefCell::new(Vec::new());
    let mut printed = String::new();

    for command in &commands {
        let cli = InitCli::try_parse_from(
            std::iter::once("init").chain(command.args.iter().map(String::as_str)),
        )
        .unwrap_or_else(|e| panic!("`{}` doesn't parse: {e}", command.text));
        let flags = cli.flags;
        assert!(
            cli.profile.is_none() && cli.db.is_none(),
            "the walkthrough uses its own paths"
        );
        // The lines piped on stdin, in the order `init` reads them.
        let mut stdin = Vec::new();
        if flags.serpapi_key_stdin {
            stdin.push(SERPAPI_KEY.to_owned());
        }
        if flags.ntfy_token_stdin {
            stdin.push(NTFY_TOKEN.to_owned());
        }
        if flags.photon_credentials_stdin {
            stdin.extend([PHOTON_ID.to_owned(), PHOTON_SECRET.to_owned()]);
        }
        let mut terminal = SecretsOnly {
            stdin,
            output: String::new(),
            questions: Vec::new(),
        };
        let find_program = |name: &str| {
            ["claude", "npm", "node"]
                .contains(&name)
                .then(|| PathBuf::from(format!("/usr/local/bin/{name}")))
        };
        let mut install_schedule = |profile: &Path| {
            schedules.borrow_mut().push(profile.to_owned());
            Ok(())
        };
        let alerts_before = http.requests.borrow().len() + sidecar_sends(&machine);

        let outcome = init::init(
            &profile_path,
            &flags,
            InitEnv {
                terminal: &mut terminal,
                secrets: &secrets,
                find_program: &find_program,
                runner: &machine,
                http: &http,
                sidecar: sidecar.clone(),
                install_schedule: &mut install_schedule,
            },
        );
        assert!(
            terminal.questions.is_empty(),
            "`{}` leaves init asking questions that aren't secrets: {:?}",
            command.text,
            terminal.questions
        );
        let outcome = outcome.unwrap_or_else(|e| panic!("`{}` failed: {e:#}", command.text));
        assert!(outcome.succeeded(), "`{}`: {outcome:?}", command.text);
        assert!(
            terminal.stdin.is_empty(),
            "`{}` left stdin unread",
            command.text
        );

        // One test Alert when the command asks for it, none otherwise.
        let alerts = http.requests.borrow().len() + sidecar_sends(&machine) - alerts_before;
        let wanted = usize::from(flags.test_alert == Some(true));
        assert_eq!(alerts, wanted, "`{}` sent {alerts} Alerts", command.text);
        if flags.test_alert == Some(true) {
            assert_eq!(outcome.test_alert, Some(Ok(())));
        }
        assert_eq!(
            outcome.schedule.is_some(),
            flags.install_schedule == Some(true),
            "`{}` installed the schedule without being asked, or didn't when asked",
            command.text
        );
        printed.push_str(&terminal.output);
    }

    // Where the walkthrough ends: the last Profile the skill shows, every
    // secret in the keychain, and none of them printed or in the Profile.
    let profile = Profile::load(&profile_path).unwrap();
    assert!(matches!(
        profile.alert_channel,
        Some(AlertChannelSetting::Photon(_))
    ));
    assert_eq!(schedules.borrow().len(), 1);
    let stored: BTreeSet<&str> = [
        SecretName::SerpApiKey,
        SecretName::PhotonProjectId,
        SecretName::PhotonProjectSecret,
    ]
    .into_iter()
    .filter(|name| keychain.get(name.account()).is_some())
    .map(SecretName::account)
    .collect();
    assert_eq!(stored.len(), 3, "stored only {stored:?}");
    let profile_text = std::fs::read_to_string(&profile_path).unwrap();
    for secret in [SERPAPI_KEY, PHOTON_ID, PHOTON_SECRET] {
        assert!(!printed.contains(secret), "{secret} was printed");
        assert!(!profile_text.contains(secret), "{secret} is in the Profile");
    }
}

/// How many times the Photon sidecar was run, each one sending an Alert.
fn sidecar_sends(machine: &Machine) -> usize {
    machine
        .commands
        .borrow()
        .iter()
        .filter(|c| c.starts_with("node "))
        .count()
}
