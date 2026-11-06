//! `hackathon-radar init`, driven through scripted input and captured output,
//! with a fake keychain, a fake PATH, a recording process runner (for `npm ci`
//! and the Photon sidecar), a recording HTTP client (for ntfy) and a fake
//! schedule install. Nothing touches the real keychain, the network, npm or
//! an Agent. The test Alert goes through the real Alert channels, built from
//! the written Profile and the secret store, as a Run builds them.

mod support;

use std::cell::RefCell;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use clap::Parser;
use hackathon_radar::alert::photon::Sidecar;
use hackathon_radar::init::{self, InitEnv, InitFlags, InitOutcome, IoTerminal};
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::{AgentAdapter, AlertChannelSetting, NtfySetting, Profile};
use hackathon_radar::secrets::{Secrets, Stored};
use hackathon_radar::wiring::TEST_ALERT;
use support::*;
use tempfile::TempDir;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    flags: InitFlags,
}

fn flags(args: &[&str]) -> InitFlags {
    Cli::try_parse_from(std::iter::once("init").chain(args.iter().copied()))
        .unwrap()
        .flags
}

/// Records each command; `npm` succeeds and `node` answers like the Photon
/// sidecar, with the recorded result named in `sidecar_reply`.
struct MachineRunner {
    commands: RefCell<Vec<Command>>,
    sidecar_reply: &'static str,
}

impl ProcessRunner for MachineRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
        match command.program.as_str() {
            "npm" => Ok(Output {
                code: Some(0),
                stdout: "added 12 packages".into(),
                stderr: String::new(),
            }),
            "node" => {
                let stdout = fixture(&format!("photon/{}", self.sidecar_reply));
                let code = if stdout.contains(r#""ok":true"#) {
                    0
                } else {
                    1
                };
                Ok(Output {
                    code: Some(code),
                    stdout,
                    stderr: String::new(),
                })
            }
            other => panic!("init ran an unexpected program: {other}"),
        }
    }
}

/// The machine `init` runs on.
struct World {
    dir: TempDir,
    keychain: FakeKeychain,
    secrets: Secrets,
    /// The programs on PATH.
    programs: Vec<&'static str>,
    runner: MachineRunner,
    http: RecordingPost,
    /// The Profiles a schedule was installed for.
    schedules: RefCell<Vec<PathBuf>>,
    sidecar: Sidecar,
}

impl World {
    fn new() -> Self {
        Self::with_keychain(FakeKeychain::default())
    }

    fn with_keychain(keychain: FakeKeychain) -> Self {
        let dir = TempDir::new().unwrap();
        let secrets = secrets_in(&keychain, &dir.path().join("config"), &[]);
        let sidecar = Sidecar {
            dir: dir.path().join("sidecar/photon"),
        };
        std::fs::create_dir_all(&sidecar.dir).unwrap();
        Self {
            dir,
            keychain,
            secrets,
            programs: vec!["claude", "npm", "node"],
            runner: MachineRunner {
                commands: RefCell::new(Vec::new()),
                sidecar_reply: "sent.json",
            },
            http: RecordingPost::ok(),
            schedules: RefCell::new(Vec::new()),
            sidecar,
        }
    }

    fn profile_path(&self) -> PathBuf {
        self.dir.path().join("config/profile.toml")
    }

    fn profile_text(&self) -> String {
        std::fs::read_to_string(self.profile_path()).unwrap()
    }

    fn write_profile(&self, text: &str) {
        std::fs::create_dir_all(self.profile_path().parent().unwrap()).unwrap();
        std::fs::write(self.profile_path(), text).unwrap();
    }

    fn profile(&self) -> Profile {
        Profile::load(&self.profile_path()).unwrap()
    }

    /// Runs `init` with these flags and these lines of input, returning what
    /// it did and everything it printed.
    fn init(&self, args: &[&str], input: &[&str]) -> (anyhow::Result<InitOutcome>, String) {
        let mut text = input.join("\n");
        text.push('\n');
        let mut terminal = IoTerminal::new(Cursor::new(text.into_bytes()), Vec::new());
        let programs = self.programs.clone();
        let find_program = move |name: &str| {
            programs
                .contains(&name)
                .then(|| PathBuf::from(format!("/usr/local/bin/{name}")))
        };
        let mut install_schedule = |profile: &Path| {
            self.schedules.borrow_mut().push(profile.to_owned());
            Ok(())
        };
        let outcome = init::init(
            &self.profile_path(),
            &flags(args),
            InitEnv {
                terminal: &mut terminal,
                secrets: &self.secrets,
                find_program: &find_program,
                runner: &self.runner,
                http: &self.http,
                sidecar: self.sidecar.clone(),
                install_schedule: &mut install_schedule,
            },
        );
        let output = String::from_utf8(terminal.into_output()).unwrap();
        (outcome, output)
    }

    fn programs_run(&self) -> Vec<String> {
        self.runner
            .commands
            .borrow()
            .iter()
            .map(|c| format!("{} {}", c.program, c.args.join(" ")))
            .collect()
    }
}

/// Asserts that none of these secrets shows up in the output or the Profile.
fn assert_never_shown(world: &World, output: &str, secrets: &[&str]) {
    let profile = std::fs::read_to_string(world.profile_path()).unwrap_or_default();
    for secret in secrets {
        assert!(!output.contains(secret), "{secret} was printed:\n{output}");
        assert!(
            !profile.contains(secret),
            "{secret} is in the Profile:\n{profile}"
        );
    }
}

const SERP_KEY: &str = "serp_interactive_key_99";

/// A first, interactive run: Brussels, ntfy, every question answered.
fn first_run(world: &World) -> (InitOutcome, String) {
    let (outcome, output) = world.init(
        &[],
        &[
            "Brussels", // Home city
            "",         // airports: the suggested BRU, CRL
            "",         // Home country: Belgium
            "Belgium, France, Germany, Netherlands",
            "",                        // currency: EUR, from Home
            "",                        // Train limit: 4
            "200",                     // Flight budget
            "",                        // Travel-funded rule: on
            "AI agents, climate",      // Themes
            "no",                      // student
            "Belgian",                 // nationalities
            "",                        // age: none
            "",                        // Agent: claude-code, found on PATH
            SERP_KEY,                  // SerpApi key, asked as there is a Flight budget
            "ntfy",                    // Alert channel
            "radar-interactive-topic", // ntfy topic
            "",                        // ntfy server: ntfy.sh
            "",                        // test Alert: yes
            "",                        // schedule: yes
        ],
    );
    (outcome.expect("init failed"), output)
}

const FIRST_PROFILE: &str = "\
# Your hackathon-radar Profile, written by `hackathon-radar init`. Run init
# again to change it, or edit it by hand: see examples/profile.toml for every
# setting. It holds no secrets; those live in your keychain.

countries = [\"Belgium\", \"France\", \"Germany\", \"Netherlands\"]
themes = [\"AI agents\", \"climate\"]

[home]
city = \"Brussels\"
airports = [\"BRU\", \"CRL\"]
country = \"Belgium\"

[travel]
train_limit_hours = 4
flight_budget = 200
travel_funded_anywhere = true

[eligibility]
student = false
nationalities = [\"Belgian\"]

[agent]
adapter = \"claude-code\"

[alert_channel]
kind = \"ntfy\"
topic = \"radar-interactive-topic\"
";

#[test]
fn a_first_interactive_run_writes_a_complete_profile_and_sends_a_test_alert() {
    let world = World::new();

    let (outcome, output) = first_run(&world);

    assert!(outcome.created);
    assert!(outcome.succeeded());
    assert_eq!(world.profile_text(), FIRST_PROFILE);
    let profile = world.profile();
    assert_eq!(profile, outcome.profile);
    assert_eq!(profile.home.airports, ["BRU", "CRL"]);
    assert_eq!(profile.currency.code(), "EUR");
    assert_eq!(profile.travel.flight_budget, Some(200));
    assert!(profile.travel.travel_funded_anywhere);
    assert_eq!(profile.themes, ["AI agents", "climate"]);
    assert_eq!(profile.agent.adapter, AgentAdapter::ClaudeCode);
    assert_eq!(
        profile.alert_channel,
        Some(AlertChannelSetting::Ntfy(NtfySetting {
            server: "https://ntfy.sh".into(),
            topic: "radar-interactive-topic".into(),
        }))
    );

    // The SerpApi key went to the keychain, and nowhere else.
    assert_eq!(
        world.keychain.get("serpapi-api-key").as_deref(),
        Some(SERP_KEY)
    );
    assert_eq!(
        outcome.stored,
        [(
            hackathon_radar::secrets::SecretName::SerpApiKey,
            Stored::Keychain
        )]
    );
    assert_never_shown(&world, &output, &[SERP_KEY]);

    // The suggestions and explanations the person saw.
    assert!(
        output.contains("Airports near Brussels: BRU, CRL."),
        "{output}"
    );
    assert!(output.contains("Airports you fly out of (IATA codes, comma-separated) [BRU, CRL]: "));
    assert!(output.contains("Home country (or none) [Belgium]: "));
    assert!(output.contains("Countries or regions (comma-separated) [Belgium]: "));
    assert!(output.contains("Schengen"), "the regions are listed");
    assert!(output.contains("in EUR (or none) [none]: "));
    assert!(output.contains("a Travel-funded Hackathon counts anywhere in the world"));
    assert!(output.contains("Count Travel-funded Hackathons anywhere in the world? [Y/n]: "));
    assert!(output.contains("Found claude at /usr/local/bin/claude."));
    assert!(output.contains("hackathon-radar never stores your Anthropic key"));
    assert!(output.contains("ANTHROPIC_API_KEY"));
    assert!(output.contains("https://serpapi.com/users/sign_up"));
    assert!(output.contains("250 searches a month"));

    // The test Alert went to the topic, and the schedule was installed.
    let requests = world.http.requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://ntfy.sh/radar-interactive-topic");
    assert_eq!(requests[0].body, TEST_ALERT);
    assert_eq!(outcome.test_alert, Some(Ok(())));
    assert!(
        output.contains("Sent a test Alert through ntfy."),
        "{output}"
    );
    assert_eq!(*world.schedules.borrow(), [world.profile_path()]);
    assert_eq!(outcome.schedule, Some(Ok(())));
    assert!(
        output.contains("`hackathon-radar import-sheet <file.csv>`"),
        "{output}"
    );
    assert!(
        world.programs_run().is_empty(),
        "nothing to install for ntfy"
    );
}

#[test]
fn a_rerun_shows_the_current_values_and_changes_only_what_it_is_told() {
    let world = World::new();
    first_run(&world);
    // The person adds a note and a section of their own.
    let edited = FIRST_PROFILE.replace("[travel]\n", "[travel]\n# I hate long trains\n")
        + "\n[deadlines]\nreminder_days = 5\n";
    world.write_profile(&edited);

    let (outcome, output) = world.init(
        &[],
        &[
            "", "", "", "", "",    // Home city, airports, country, Countries, currency
            "5.5", // Train limit
            "", "",        // Flight budget, Travel-funded rule
            "climate", // Themes
            "", "", "", "", // student, nationalities, age, Agent
            "", // SerpApi key: keep the stored one
            "", "", "", // Alert channel, topic, server
            "n", "n", // no test Alert, no schedule
        ],
    );
    let outcome = outcome.expect("init failed");

    assert!(!outcome.created);
    assert!(output.contains("Let's update your Profile"), "{output}");
    assert!(
        !output.contains("import-sheet"),
        "only a new Profile gets the hint"
    );
    for shown in [
        "Home city [Brussels]: ",
        "Airports you fly out of (IATA codes, comma-separated) [BRU, CRL]: ",
        "Countries or regions (comma-separated) [Belgium, France, Germany, Netherlands]: ",
        "in hours [4]: ",
        "in EUR (or none) [200]: ",
        "Themes, such as AI agents, climate (comma-separated, or none) [AI agents, climate]: ",
        "Are you a student? (yes, no or none) [no]: ",
        "Your SerpApi key is in the macOS Keychain.",
        "SerpApi key (hidden; Enter keeps the one you have): ",
        "Alert channel (photon, ntfy or none) [ntfy]: ",
        "ntfy topic [radar-interactive-topic]: ",
    ] {
        let shown = if cfg!(target_os = "macos") {
            shown.to_owned()
        } else {
            shown.replace("the macOS Keychain", "your keychain (the Secret Service)")
        };
        assert!(output.contains(&shown), "{shown:?} not in:\n{output}");
    }
    assert_eq!(
        world.profile_text(),
        edited
            .replace("train_limit_hours = 4", "train_limit_hours = 5.5")
            .replace(
                "themes = [\"AI agents\", \"climate\"]",
                "themes = [\"climate\"]"
            )
    );
    assert_eq!(
        world.keychain.get("serpapi-api-key").as_deref(),
        Some(SERP_KEY)
    );
    assert!(outcome.stored.is_empty());
    assert_eq!(outcome.test_alert, None);
    assert_eq!(outcome.schedule, None);
    assert_eq!(
        world.http.requests.borrow().len(),
        1,
        "only the first run's test Alert"
    );
}

#[test]
fn a_rerun_with_yes_and_no_flags_leaves_the_profile_exactly_as_it_was() {
    let world = World::new();
    let mine = "# mine\ncountries = [ \"Belgium\" ]\n\n[home]\ncity = \"Brussels\"\n\
                airports = [\"BRU\"]\n\n[deadlines]\nreminder_days = 2\n";
    world.write_profile(mine);

    let (outcome, output) = world.init(&["--yes", "--test-alert", "false"], &[]);

    outcome.expect("init failed");
    assert_eq!(world.profile_text(), mine);
    assert!(!output.contains("[Y/n]"), "nothing is asked: {output}");
}

#[test]
fn flags_drive_init_without_questions_and_secrets_come_from_stdin() {
    let world = World::new();

    let (outcome, output) = world.init(
        &[
            "--yes",
            "--home-city",
            "Lyon",
            "--countries",
            "France, Schengen",
            "--train-limit",
            "3",
            "--flight-budget",
            "150",
            "--themes",
            "climate",
            "--student",
            "true",
            "--age",
            "24",
            "--agent",
            "claude-code",
            "--serpapi-key-stdin",
            "--alert-channel",
            "ntfy",
            "--ntfy-topic",
            "radar-flags",
            "--ntfy-server",
            "https://ntfy.example.org/",
            "--ntfy-token-stdin",
        ],
        &["serp_piped_key", "tk_piped_token"],
    );
    let outcome = outcome.expect("init failed");

    let profile = world.profile();
    assert_eq!(profile.home.city, "Lyon");
    assert_eq!(profile.home.airports, ["LYS"], "suggested from the city");
    assert_eq!(profile.home.country.as_deref(), Some("France"));
    assert_eq!(profile.countries, ["France", "Schengen"]);
    assert_eq!(profile.travel.train_limit_minutes, 180);
    assert_eq!(profile.travel.flight_budget, Some(150));
    assert!(profile.travel.travel_funded_anywhere, "on by default");
    assert_eq!(profile.themes, ["climate"]);
    assert_eq!(profile.eligibility.student, Some(true));
    assert_eq!(profile.eligibility.age, Some(24));
    assert_eq!(
        profile.alert_channel,
        Some(AlertChannelSetting::Ntfy(NtfySetting {
            server: "https://ntfy.example.org".into(),
            topic: "radar-flags".into(),
        }))
    );
    assert!(
        !world.profile_text().contains("currency"),
        "it follows Home"
    );

    assert_eq!(
        world.keychain.get("serpapi-api-key").as_deref(),
        Some("serp_piped_key")
    );
    assert_eq!(
        world.keychain.get("ntfy-token").as_deref(),
        Some("tk_piped_token")
    );
    assert_never_shown(&world, &output, &["serp_piped_key", "tk_piped_token"]);

    // Nothing was asked, the test Alert used the stored token, and --yes
    // leaves the schedule alone.
    assert!(
        !output.contains("[Y/n]") && !output.contains("]: "),
        "{output}"
    );
    let requests = world.http.requests.borrow();
    assert_eq!(requests[0].url, "https://ntfy.example.org/radar-flags");
    assert!(
        requests[0]
            .headers
            .contains(&("Authorization".into(), "Bearer tk_piped_token".into()))
    );
    assert_eq!(outcome.test_alert, Some(Ok(())));
    assert_eq!(outcome.schedule, None);
    assert!(world.schedules.borrow().is_empty());
    assert!(
        output.contains("run `hackathon-radar schedule install`"),
        "{output}"
    );
}

#[test]
fn missing_answers_are_listed_together_and_nothing_is_written() {
    let world = World::new();

    let (outcome, _) = world.init(&["--yes"], &[]);
    let message = outcome.unwrap_err().to_string();
    assert_eq!(
        message,
        "init needs more answers, so nothing was written.\nMissing:\n  \
         - --home-city <CITY>: the city you travel from\n  \
         - --airports <CODES>: the airports you fly out of, such as BRU,CRL\n  \
         - --countries <LIST>: the countries or regions you'll travel to at your own cost"
    );
    assert!(!world.profile_path().exists());

    // A city init doesn't know needs its airports and Countries spelt out;
    // Photon needs a number and credentials; Codex needs the opt-in; and a
    // value that makes no sense is reported with the rest.
    let (outcome, _) = world.init(
        &[
            "--yes",
            "--home-city",
            "Smallville",
            "--train-limit",
            "a lot",
            "--agent",
            "codex",
            "--alert-channel",
            "photon",
        ],
        &[],
    );
    let message = outcome.unwrap_err().to_string();
    for expected in [
        "- --airports <CODES>",
        "- --countries <LIST>",
        "- --experimental: the Codex adapter is experimental",
        "- --photon-to <NUMBER>: the phone number Photon texts",
        "- --photon-credentials-stdin: your Photon project ID and secret",
        "Not understood:\n  - --train-limit <HOURS> \"a lot\": Give a number of hours",
    ] {
        assert!(
            message.contains(expected),
            "{expected:?} not in:\n{message}"
        );
    }
    assert!(!world.profile_path().exists());
    assert!(world.http.requests.borrow().is_empty());
}

#[test]
fn photon_setup_stores_the_credentials_installs_the_sidecar_and_sends_a_test_alert() {
    let world = World::new();

    let (outcome, output) = world.init(
        &[
            "--yes",
            "--home-city",
            "Brussels",
            "--alert-channel",
            "photon",
            "--photon-to",
            "+32470123456",
            "--photon-credentials-stdin",
            "--install-schedule",
            "true",
        ],
        &["proj_piped_id", "photon_piped_secret"],
    );
    let outcome = outcome.expect("init failed");

    assert_eq!(world.profile().countries, ["Belgium"], "Home's country");
    assert!(
        world
            .profile_text()
            .contains("[alert_channel]\nkind = \"photon\"\nto = \"+32470123456\"\n")
    );
    assert_eq!(
        world.keychain.get("photon-project-id").as_deref(),
        Some("proj_piped_id")
    );
    assert_eq!(
        world.keychain.get("photon-project-secret").as_deref(),
        Some("photon_piped_secret")
    );
    assert_never_shown(&world, &output, &["proj_piped_id", "photon_piped_secret"]);

    // `npm ci` in the sidecar, then the test Alert through it.
    let commands = world.runner.commands.borrow();
    assert_eq!(world.programs_run()[0], "npm ci");
    assert_eq!(
        commands[0].working_dir.as_deref(),
        Some(world.sidecar.dir.as_path())
    );
    assert_eq!(commands[1].program, "node");
    let request: serde_json::Value = serde_json::from_str(&commands[1].stdin).unwrap();
    assert_eq!(request["to"], "+32470123456");
    assert_eq!(request["text"], TEST_ALERT);
    assert_eq!(
        commands[1].env[1],
        (
            "PHOTON_PROJECT_SECRET".to_owned(),
            "photon_piped_secret".to_owned()
        )
    );
    assert_eq!(outcome.test_alert, Some(Ok(())));
    assert!(
        output.contains("Sent a test Alert through iMessage through Photon."),
        "{output}"
    );
    assert_eq!(*world.schedules.borrow(), [world.profile_path()]);
}

#[test]
fn interactive_photon_setup_explains_signup_and_texting_the_line() {
    let mut world = World::new();
    world.runner.sidecar_reply = "not-texted-yet.json";
    // The sidecar's dependencies are already installed.
    std::fs::create_dir_all(world.sidecar.dir.join("node_modules")).unwrap();

    let (outcome, output) = world.init(
        &["--home-city", "Brussels", "--countries", "Belgium"],
        &[
            "",
            "", // airports, Home country
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "", // currency … Agent
            "photon",
            "+32470123456",
            "proj_typed_id",
            "", // the secret is needed, so it is asked again
            "photon_typed_secret",
            "",  // test Alert: yes
            "n", // no schedule
        ],
    );
    let outcome = outcome.expect("init failed");

    for expected in [
        "Sign up at https://app.photon.codes, create a project and turn on iMessage (Spectrum).",
        "Text your Photon line once from your phone",
        "Photon project ID (hidden): ",
        "Photon project secret (hidden): ",
        "This one is needed: it's in the Photon dashboard",
        "have you texted your Photon line from your phone?",
        "already installed",
    ] {
        assert!(output.contains(expected), "{expected:?} not in:\n{output}");
    }
    assert_never_shown(&world, &output, &["proj_typed_id", "photon_typed_secret"]);
    let programs: Vec<String> = world
        .runner
        .commands
        .borrow()
        .iter()
        .map(|c| c.program.clone())
        .collect();
    assert_eq!(programs, ["node"], "no npm ci; one sidecar run");

    // The test Alert failed: the Profile and keys are kept, and the person is
    // told why and what to do.
    assert!(!outcome.succeeded());
    assert!(matches!(&outcome.test_alert, Some(Err(e)) if e.contains("texted it first")));
    assert!(output.contains("Your Profile is saved"), "{output}");
    assert!(output.contains("hackathon-radar test-alert"), "{output}");
    assert!(matches!(
        world.profile().alert_channel,
        Some(AlertChannelSetting::Photon(_))
    ));
    assert_eq!(
        world.keychain.get("photon-project-secret").as_deref(),
        Some("photon_typed_secret")
    );
}

#[test]
fn without_a_keychain_secrets_go_to_the_owner_only_file() {
    let world = World::with_keychain(FakeKeychain::unavailable());

    let (outcome, output) = world.init(
        &[
            "--yes",
            "--home-city",
            "Berlin",
            "--flight-budget",
            "180",
            "--serpapi-key-stdin",
        ],
        &["serp_headless_key"],
    );
    let outcome = outcome.expect("init failed");

    let file = world.dir.path().join("config/secrets.toml");
    assert!(matches!(&outcome.stored[0].1, Stored::File { path, .. } if *path == file));
    assert!(output.contains("No keychain could be reached (no keychain on this machine)"));
    assert!(output.contains("readable only by you"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    assert_never_shown(&world, &output, &["serp_headless_key"]);
    assert_eq!(world.profile().currency.code(), "EUR");
    assert_eq!(world.profile().countries, ["Germany"]);
}

#[test]
fn codex_needs_an_explicit_opt_in() {
    let mut world = World::new();
    world.programs = vec!["codex"];

    let (outcome, output) = world.init(
        &[
            "--home-city",
            "Paris",
            "--countries",
            "France",
            "--alert-channel",
            "none",
        ],
        &[
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "", // airports … age
            "codex",
            "n",           // won't opt in, so asked again
            "claude-code", // and picks Claude Code
            "n",           // no schedule
        ],
    );
    outcome.expect("init failed");
    assert!(output.contains("claude isn't on your PATH."), "{output}");
    assert!(output.contains("Found codex at /usr/local/bin/codex."));
    assert!(output.contains("unverified against a real Codex CLI"));
    assert!(output.contains("Opt in to the experimental Codex adapter? [y/N]: "));
    assert!(output.contains("Install Claude Code"), "{output}");
    assert_eq!(world.profile().agent.adapter, AgentAdapter::ClaudeCode);

    let (outcome, _) = world.init(&["--yes", "--agent", "codex", "--experimental"], &[]);
    outcome.expect("init failed");
    let profile = world.profile();
    assert_eq!(profile.agent.adapter, AgentAdapter::Codex);
    assert!(profile.agent.experimental);
}

#[test]
fn a_command_agent_takes_its_template() {
    let world = World::new();

    let (outcome, _) = world.init(
        &[
            "--yes",
            "--home-city",
            "Amsterdam",
            "--agent",
            "command",
            "--agent-command",
            "my-agent --json --schema {schema_file}",
        ],
        &[],
    );
    outcome.expect("init failed");
    let command = world.profile().agent.command.expect("a command template");
    assert_eq!(command.program(), "my-agent");
    assert!(
        world
            .profile_text()
            .contains("command = [\"my-agent\", \"--json\", \"--schema\", \"{schema_file}\"]")
    );

    // Switching back to Claude Code removes the template, which only goes with a command.
    let (outcome, _) = world.init(&["--yes", "--agent", "claude-code"], &[]);
    outcome.expect("init failed");
    assert_eq!(world.profile().agent.command, None);
}

#[test]
fn switching_alert_channels_rewrites_the_section() {
    let world = World::new();
    world.write_profile(
        "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n\
         [alert_channel]\nkind = \"ntfy\"\ntopic = \"radar-old\"\n",
    );

    let (outcome, _) = world.init(
        &[
            "--yes",
            "--alert-channel",
            "photon",
            "--photon-to",
            "+32470123456",
            "--photon-credentials-stdin",
            "--test-alert",
            "false",
            "--skip-npm-ci",
        ],
        &["proj_x", "secret_x"],
    );
    outcome.expect("init failed");
    assert!(
        world
            .profile_text()
            .ends_with("[alert_channel]\nkind = \"photon\"\nto = \"+32470123456\"\n")
    );
    assert!(
        world.programs_run().is_empty(),
        "--skip-npm-ci and no test Alert"
    );

    let (outcome, _) = world.init(&["--yes", "--alert-channel", "none"], &[]);
    let outcome = outcome.expect("init failed");
    assert!(!world.profile_text().contains("alert_channel"));
    assert_eq!(outcome.test_alert, None);
}

#[test]
fn input_that_ends_early_writes_nothing() {
    let world = World::new();
    let (outcome, _) = world.init(&[], &["Brussels", ""]);
    let message = outcome.unwrap_err().to_string();
    assert!(message.contains("nothing was written"), "{message}");
    assert!(!world.profile_path().exists());
}
