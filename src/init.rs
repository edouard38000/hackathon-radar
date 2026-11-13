//! `hackathon-radar init`: guided setup of the Profile, the secrets, the Alert
//! channel and the schedule. It is the one place setup logic lives (ADR 0007):
//! a person runs it in their terminal, and the setup skill drives it with
//! flags and `--yes`.
//!
//! Every question has a flag. A flag answers its question; `--yes` answers
//! every other one with its default, which is the value the Profile already
//! has, so a re-run only changes what it is told to. Questions with no
//! default are reported together, and nothing is written until every answer
//! is in. Secrets never come from a flag (shell history keeps flags): they are
//! typed at a hidden prompt, or piped on stdin with the `--…-stdin` flags.

pub mod airports;
mod terminal;

pub use terminal::{IoTerminal, StdTerminal, Terminal};

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;

use crate::alert::photon::{self, Sidecar};
use crate::countries;
use crate::http::HttpPost;
use crate::money::Currency;
use crate::process::{Command, Output, ProcessRunner};
use crate::profile::{
    self, DEFAULT_NTFY_SERVER, DEFAULT_TRAIN_LIMIT_MINUTES, Profile, ProfileDocument,
};
use crate::secrets::{Origin, Secret, SecretName, Secrets, Stored};
use crate::sources::places;
use crate::wiring;

/// SerpApi's sign-up page; its free plan has 250 searches a month.
pub const SERPAPI_SIGN_UP: &str = "https://serpapi.com/users/sign_up";

/// How long `npm ci` in the Photon sidecar may take.
const NPM_CI_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The answers `init` can take as flags. Lists are comma-separated, and
/// `none` clears an optional value.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct InitFlags {
    /// Ask nothing: take the flags, keep what the Profile already has, use each other question's default, and fail with a list of the answers still missing
    #[arg(long)]
    pub yes: bool,

    /// The city you travel from (Home)
    #[arg(long, value_name = "CITY")]
    pub home_city: Option<String>,

    /// Home's country, or `none` to leave it out
    #[arg(long, value_name = "COUNTRY")]
    pub home_country: Option<String>,

    /// The airports you fly out of, as IATA codes [default: the ones near Home, when init knows the city]
    #[arg(long, value_name = "CODES")]
    pub airports: Option<String>,

    /// The countries or regions (such as Schengen or US West Coast) you'll travel to at your own cost [default: Home's country]
    #[arg(long, value_name = "LIST")]
    pub countries: Option<String>,

    /// The currency you count money in, as an ISO code [default: Home country's]
    #[arg(long, value_name = "CODE")]
    pub currency: Option<String>,

    /// The Train limit: the longest train journey from Home that still counts as going by train [default: 4]
    #[arg(long, value_name = "HOURS")]
    pub train_limit: Option<String>,

    /// The Flight budget: the most you'd pay for a return flight, in your currency, or `none`
    #[arg(long, value_name = "AMOUNT")]
    pub flight_budget: Option<String>,

    /// The Travel-funded rule: whether Travel-funded Hackathons count anywhere in the world [default: true for a new Profile]
    #[arg(long, value_name = "BOOL")]
    pub travel_funded_anywhere: Option<bool>,

    /// The Themes you want Hackathons about, or `none`
    #[arg(long, value_name = "LIST")]
    pub themes: Option<String>,

    /// Eligibility: whether you are a student (true, false, or none to leave it out)
    #[arg(long, value_name = "BOOL")]
    pub student: Option<String>,

    /// Eligibility: your nationalities, or `none`
    #[arg(long, value_name = "LIST")]
    pub nationalities: Option<String>,

    /// Eligibility: your age, or `none`
    #[arg(long, value_name = "AGE")]
    pub age: Option<String>,

    /// The Agent that judges listings [default: claude-code]
    #[arg(long, value_name = "ADAPTER", value_parser = ["claude-code", "codex", "command"])]
    pub agent: Option<String>,

    /// Opt in to the experimental Codex adapter (needed with --agent codex)
    #[arg(long)]
    pub experimental: bool,

    /// The command template for --agent command, split on spaces, such as "my-agent --schema {schema_file}"
    #[arg(long, value_name = "COMMAND")]
    pub agent_command: Option<String>,

    /// Read your SerpApi key from stdin
    #[arg(long)]
    pub serpapi_key_stdin: bool,

    /// Where Alerts go
    #[arg(long, value_name = "CHANNEL", value_parser = ["photon", "ntfy", "none"])]
    pub alert_channel: Option<String>,

    /// The ntfy topic you subscribed to in the ntfy app
    #[arg(long, value_name = "TOPIC")]
    pub ntfy_topic: Option<String>,

    /// Your own ntfy server [default: https://ntfy.sh]
    #[arg(long, value_name = "URL")]
    pub ntfy_server: Option<String>,

    /// Read an ntfy access token from stdin
    #[arg(long)]
    pub ntfy_token_stdin: bool,

    /// Your phone number for Photon, in E.164 format such as +32470123456
    #[arg(long, value_name = "NUMBER")]
    pub photon_to: Option<String>,

    /// Read your Photon project ID, then its secret, from stdin, one per line
    #[arg(long)]
    pub photon_credentials_stdin: bool,

    /// Don't run `npm ci` in the Photon sidecar
    #[arg(long)]
    pub skip_npm_ci: bool,

    /// Send a test Alert once the Profile is written [default: true]
    #[arg(long, value_name = "BOOL")]
    pub test_alert: Option<bool>,

    /// Install the schedule once the Profile is written [default: false with --yes]
    #[arg(long, value_name = "BOOL")]
    pub install_schedule: Option<bool>,
}

/// Runs another program; a reference so `init` (and `doctor`) can lend it to
/// the Alert channel.
pub(crate) struct Runner<'a>(pub(crate) &'a dyn ProcessRunner);

impl ProcessRunner for Runner<'_> {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.0.run(command)
    }
}

/// Everything `init` touches outside the Profile file, so tests can fake it.
pub struct InitEnv<'a> {
    pub terminal: &'a mut dyn Terminal,
    pub secrets: &'a Secrets,
    /// Finds a program on `PATH`, such as `claude`.
    pub find_program: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// Runs `npm ci` and the Photon sidecar.
    pub runner: &'a dyn ProcessRunner,
    /// Sends the ntfy test Alert.
    pub http: &'a dyn HttpPost,
    pub sidecar: Sidecar,
    /// Installs the schedule for the Profile at this path, as
    /// `hackathon-radar schedule install --profile <path>` does.
    pub install_schedule: &'a mut dyn FnMut(&Path) -> anyhow::Result<()>,
}

/// What `init` did.
#[derive(Debug)]
pub struct InitOutcome {
    pub profile: Profile,
    /// True when there was no Profile before.
    pub created: bool,
    /// The secrets it stored, and where.
    pub stored: Vec<(SecretName, Stored)>,
    /// Whether the test Alert was sent, when one was asked for.
    pub test_alert: Option<Result<(), String>>,
    /// Whether the schedule was installed, when that was asked for.
    pub schedule: Option<Result<(), String>>,
}

impl InitOutcome {
    /// Whether everything asked for worked.
    pub fn succeeded(&self) -> bool {
        !matches!(self.test_alert, Some(Err(_))) && !matches!(self.schedule, Some(Err(_)))
    }
}

/// Finds `program` in the directories of `path` (a `PATH` value).
pub fn find_on_path(program: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

pub(crate) fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    metadata.is_file()
}

/// Sets up or edits the Profile at `path`.
pub fn init(path: &Path, flags: &InitFlags, env: InitEnv<'_>) -> anyhow::Result<InitOutcome> {
    let existing = ProfileDocument::load(path)?;
    let created = existing.is_none();
    let doc = existing.unwrap_or_default();
    let mut session = Session {
        flags,
        env,
        doc,
        interactive: !flags.yes,
        missing: Vec::new(),
        problems: Vec::new(),
        pending: Vec::new(),
    };
    session.run(path, created)
}

/// A question's flag and what it asks for, for the list of missing answers.
struct Question {
    prompt: String,
    flag: &'static str,
    what: &'static str,
}

fn question(prompt: impl Into<String>, flag: &'static str, what: &'static str) -> Question {
    Question {
        prompt: prompt.into(),
        flag,
        what,
    }
}

struct Session<'f, 'e> {
    flags: &'f InitFlags,
    env: InitEnv<'e>,
    doc: ProfileDocument,
    /// False with `--yes`: nothing is asked.
    interactive: bool,
    /// Required answers that no flag, Profile value or default gave.
    missing: Vec<String>,
    /// Flag values that don't make sense.
    problems: Vec<String>,
    /// Secrets to store once the Profile is written.
    pending: Vec<(SecretName, Secret)>,
}

type Parse<'a, T> = &'a dyn Fn(&str) -> Result<T, String>;

impl Session<'_, '_> {
    fn say(&mut self, text: &str) -> anyhow::Result<()> {
        self.env
            .terminal
            .say(text)
            .context("writing to the terminal")
    }

    fn say_if_asking(&mut self, text: &str) -> anyhow::Result<()> {
        if self.interactive {
            self.say(text)?;
        }
        Ok(())
    }

    fn read(&mut self, prompt: &str, secret: bool) -> anyhow::Result<String> {
        let answer = if secret {
            self.env.terminal.ask_secret(prompt)
        } else {
            self.env.terminal.ask(prompt)
        }
        .context("reading from the terminal")?;
        answer.map(|a| a.trim().to_owned()).ok_or_else(|| {
            anyhow::anyhow!("the input ended before init finished, so nothing was written")
        })
    }

    /// Answers a question: from its flag, else the default with `--yes`, else
    /// by asking until the answer makes sense. `None` means no answer and no
    /// default, which is noted as missing.
    fn answer<T>(
        &mut self,
        q: Question,
        flag: Option<&str>,
        default: Option<T>,
        show: &dyn Fn(&T) -> String,
        parse: Parse<'_, T>,
    ) -> anyhow::Result<Option<T>> {
        if let Some(raw) = flag {
            return Ok(match parse(raw.trim()) {
                Ok(value) => Some(value),
                Err(why) => {
                    self.problems.push(format!("{} {raw:?}: {why}", q.flag));
                    default
                }
            });
        }
        if !self.interactive {
            if default.is_none() {
                self.missing.push(format!("{}: {}", q.flag, q.what));
            }
            return Ok(default);
        }
        let prompt = match &default {
            Some(value) => format!("{} [{}]: ", q.prompt, show(value)),
            None => format!("{}: ", q.prompt),
        };
        loop {
            let raw = self.read(&prompt, false)?;
            if raw.is_empty() {
                match default {
                    Some(value) => return Ok(Some(value)),
                    None => {
                        self.say(&format!("  This one is needed: {}.", q.what))?;
                        continue;
                    }
                }
            }
            match parse(&raw) {
                Ok(value) => return Ok(Some(value)),
                Err(why) => self.say(&format!("  {why}"))?,
            }
        }
    }

    fn yes_no(&mut self, prompt: &str, flag: Option<bool>, default: bool) -> anyhow::Result<bool> {
        if let Some(value) = flag {
            return Ok(value);
        }
        if !self.interactive {
            return Ok(default);
        }
        let prompt = format!("{prompt} [{}]: ", if default { "Y/n" } else { "y/N" });
        loop {
            let raw = self.read(&prompt, false)?;
            if raw.is_empty() {
                return Ok(default);
            }
            match parse_bool(&raw) {
                Ok(value) => return Ok(value),
                Err(why) => self.say(&format!("  {why}"))?,
            }
        }
    }

    /// A secret typed at a hidden prompt; empty keeps what is stored.
    fn ask_secret(&mut self, prompt: &str) -> anyhow::Result<Option<Secret>> {
        let raw = self.read(prompt, true)?;
        Ok(Secret::new(&raw))
    }

    fn run(&mut self, path: &Path, created: bool) -> anyhow::Result<InitOutcome> {
        self.intro(path, created)?;
        self.secrets_from_stdin()?;
        let city_changed = self.home()?;
        self.countries(city_changed)?;
        self.money()?;
        self.travel_funded()?;
        self.themes()?;
        self.eligibility()?;
        self.agent()?;
        self.serpapi()?;
        self.alert_channel()?;

        if !self.missing.is_empty() || !self.problems.is_empty() {
            let mut message = String::from("init needs more answers, so nothing was written.");
            if !self.missing.is_empty() {
                message.push_str("\nMissing:");
                for line in &self.missing {
                    message.push_str(&format!("\n  - {line}"));
                }
            }
            if !self.problems.is_empty() {
                message.push_str("\nNot understood:");
                for line in &self.problems {
                    message.push_str(&format!("\n  - {line}"));
                }
            }
            anyhow::bail!(message);
        }

        let profile = self.doc.save(path)?;
        self.say(&format!(
            "\n{} your Profile at {}.",
            if created { "Wrote" } else { "Updated" },
            path.display()
        ))?;

        let mut stored = Vec::new();
        for (name, secret) in std::mem::take(&mut self.pending) {
            let place = self.env.secrets.store(name, &secret)?;
            let message = match &place {
                Stored::Keychain => format!("Stored {} in {place}.", name.describe()),
                Stored::File { why, .. } => format!(
                    "No keychain could be reached ({why}), so {} is stored in {place}.",
                    name.describe()
                ),
            };
            self.say(&message)?;
            stored.push((name, place));
        }

        self.install_sidecar(&profile)?;
        let test_alert = self.test_alert(&profile)?;
        let schedule = self.schedule(path, &profile)?;
        if created {
            self.say(
                "Tracking hackathons in a spreadsheet already? `hackathon-radar import-sheet \
                 <file.csv>` brings them in.",
            )?;
        }
        Ok(InitOutcome {
            profile,
            created,
            stored,
            test_alert,
            schedule,
        })
    }

    fn intro(&mut self, path: &Path, created: bool) -> anyhow::Result<()> {
        let text = if created {
            format!(
                "Let's set up hackathon-radar. Your answers go in your Profile at {}.\n\
                 Press Enter to take the suggestion in [brackets]. Secrets you type are hidden, \
                 and go to your keychain, never the Profile.",
                path.display()
            )
        } else {
            format!(
                "Let's update your Profile at {}. Its current values are in [brackets]: press \
                 Enter to keep them. Anything init doesn't ask about stays as it is.",
                path.display()
            )
        };
        self.say_if_asking(&text)
    }

    /// Reads the secrets piped on stdin, one line each, in a fixed order.
    fn secrets_from_stdin(&mut self) -> anyhow::Result<()> {
        let wanted: Vec<(SecretName, &str)> = [
            (
                self.flags.serpapi_key_stdin,
                SecretName::SerpApiKey,
                "--serpapi-key-stdin",
            ),
            (
                self.flags.ntfy_token_stdin,
                SecretName::NtfyToken,
                "--ntfy-token-stdin",
            ),
            (
                self.flags.photon_credentials_stdin,
                SecretName::PhotonProjectId,
                "--photon-credentials-stdin",
            ),
            (
                self.flags.photon_credentials_stdin,
                SecretName::PhotonProjectSecret,
                "--photon-credentials-stdin",
            ),
        ]
        .into_iter()
        .filter(|(on, _, _)| *on)
        .map(|(_, name, flag)| (name, flag))
        .collect();
        for (name, flag) in wanted {
            let line = self
                .env
                .terminal
                .read_line()
                .context("reading a secret from stdin")?;
            match line.as_deref().and_then(Secret::new) {
                Some(secret) => self.pending.push((name, secret)),
                None => self.problems.push(format!(
                    "{flag}: expected {} on its own line on stdin",
                    name.describe()
                )),
            }
        }
        Ok(())
    }

    fn pending(&self, name: SecretName) -> bool {
        self.pending.iter().any(|(n, _)| *n == name)
    }

    /// Home: the city, its airports and its country. Returns whether the city changed.
    fn home(&mut self) -> anyhow::Result<bool> {
        self.say_if_asking("\nHome: where you travel from.")?;
        let current_city = self.doc.get_str("home.city");
        let city = self
            .answer(
                question(
                    "Home city",
                    "--home-city <CITY>",
                    "the city you travel from",
                ),
                self.flags.home_city.as_deref(),
                current_city.clone(),
                &|c: &String| c.clone(),
                &|raw| non_empty(raw, "Give the city you travel from."),
            )?
            .unwrap_or_default();
        if !city.is_empty() {
            self.doc.set_str("home.city", &city);
        }
        let changed = current_city.as_deref().map(places::fold) != Some(places::fold(&city));
        let nearby = airports::near_city(&city);
        // Suggestions apply to a new Home; otherwise the Profile's values stand.
        let suggest = changed || self.doc.is_new();

        let current_airports = self
            .doc
            .get_strings("home.airports")
            .filter(|a| !a.is_empty());
        let suggested = nearby.map(|n| n.airports.iter().map(|a| a.to_string()).collect());
        let default = if suggest {
            suggested.or(current_airports)
        } else {
            current_airports.or(suggested)
        };
        if let (true, Some(nearby)) = (self.interactive, nearby) {
            self.say(&format!(
                "  Airports near {}: {}.",
                nearby.city,
                nearby.airports.join(", ")
            ))?;
        }
        if let Some(codes) = self.answer(
            question(
                "Airports you fly out of (IATA codes, comma-separated)",
                "--airports <CODES>",
                "the airports you fly out of, such as BRU,CRL",
            ),
            self.flags.airports.as_deref(),
            default,
            &|a: &Vec<String>| a.join(", "),
            &parse_airports,
        )? {
            self.doc.set_strings("home.airports", &codes);
        }

        let current_country = self.doc.get_str("home.country");
        let suggested = nearby.map(|n| n.country.to_owned());
        let default = if suggest {
            suggested.or(current_country.clone())
        } else {
            current_country.clone()
        };
        let country = self
            .answer(
                question(
                    "Home country (or none)",
                    "--home-country <COUNTRY>",
                    "Home's country",
                ),
                self.flags.home_country.as_deref(),
                Some(default),
                &|c: &Option<String>| c.clone().unwrap_or_else(|| "none".into()),
                &|raw| optional(raw, |v| Ok(v.to_owned())),
            )?
            .flatten();
        match country {
            Some(country) => self.doc.set_str("home.country", &country),
            None => self.doc.remove("home.country"),
        }
        Ok(changed)
    }

    fn home_country(&self) -> Option<String> {
        let country = self.doc.get_str("home.country");
        match country {
            Some(c) => Some(places::country_name(&c).map_or(c.clone(), str::to_owned)),
            None => places::known_city(&self.doc.get_str("home.city").unwrap_or_default())
                .map(|(_, country)| country.to_owned()),
        }
    }

    fn countries(&mut self, city_changed: bool) -> anyhow::Result<()> {
        if self.interactive {
            let regions: Vec<&str> = countries::region_names().collect();
            self.say(&format!(
                "\nCountries: where you'll travel at your own cost, as long as the trip also \
                 passes your Train limit or Flight budget. Name countries, US states, or \
                 regions: {}.",
                regions.join(", ")
            ))?;
        }
        let current = self.doc.get_strings("countries").filter(|c| !c.is_empty());
        let home = self.home_country().map(|c| vec![c]);
        // A new Home suggests its own country; otherwise the Profile's Countries stand.
        let default = if city_changed && self.doc.is_new() {
            home.or(current)
        } else {
            current.or(home)
        };
        if let Some(countries) = self.answer(
            question(
                "Countries or regions (comma-separated)",
                "--countries <LIST>",
                "the countries or regions you'll travel to at your own cost",
            ),
            self.flags.countries.as_deref(),
            default,
            &|c: &Vec<String>| c.join(", "),
            &|raw| {
                let list = parse_list(raw);
                if list.is_empty() {
                    Err("Name at least one country or region, such as Belgium or Schengen.".into())
                } else {
                    Ok(list)
                }
            },
        )? {
            self.doc.set_strings("countries", &countries);
        }
        Ok(())
    }

    /// The currency, the Train limit and the Flight budget.
    fn money(&mut self) -> anyhow::Result<()> {
        let from_home = Currency::of_country(self.home_country().as_deref());
        let current = self.doc.get_str("currency");
        self.say_if_asking("\nGetting there.")?;
        let currency = self
            .answer(
                question(
                    "Currency you count money in (ISO code)",
                    "--currency <CODE>",
                    "your currency",
                ),
                self.flags.currency.as_deref(),
                Some(
                    current
                        .clone()
                        .unwrap_or_else(|| from_home.code().to_owned()),
                ),
                &|c: &String| c.clone(),
                &|raw| {
                    Currency::parse(raw)
                        .map(|c| c.code().to_owned())
                        .ok_or_else(|| "Give a three-letter code, such as EUR or USD.".into())
                },
            )?
            .unwrap_or_else(|| from_home.code().to_owned());
        // Left out when it is Home's, so the currency follows Home.
        if current.is_some() || currency != from_home.code() {
            self.doc.set_str("currency", &currency);
        }

        let fallback = f64::from(DEFAULT_TRAIN_LIMIT_MINUTES) / 60.0;
        let current = self.doc.get_number("travel.train_limit_hours");
        if let Some(hours) = self.answer(
            question(
                "Train limit: the longest train journey you'd take, in hours",
                "--train-limit <HOURS>",
                "your Train limit",
            ),
            self.flags.train_limit.as_deref(),
            Some(current.unwrap_or(fallback)),
            &|h: &f64| h.to_string(),
            &|raw| match raw.parse::<f64>() {
                Ok(h) if h > 0.0 && h <= 48.0 => Ok(h),
                _ => Err("Give a number of hours, such as 4 or 5.5.".into()),
            },
        )? {
            self.put(current.is_some(), hours == fallback, |doc| {
                doc.set_number("travel.train_limit_hours", hours)
            });
        }

        let current = self.doc.get_int("travel.flight_budget");
        let budget = self.answer(
            question(
                format!(
                    "Flight budget: the most you'd pay for a return flight, in {currency} (or none)"
                ),
                "--flight-budget <AMOUNT>",
                "your Flight budget",
            ),
            self.flags.flight_budget.as_deref(),
            Some(current),
            &|b: &Option<i64>| b.map_or_else(|| "none".into(), |b| b.to_string()),
            &|raw| {
                optional(raw, |v| match v.parse::<u32>() {
                    Ok(n) if n > 0 => Ok(i64::from(n)),
                    _ => Err("Give a whole amount, such as 200, or none.".into()),
                })
            },
        )?;
        match budget.flatten() {
            Some(amount) => self.doc.set_int("travel.flight_budget", amount),
            None => self.doc.remove("travel.flight_budget"),
        }
        Ok(())
    }

    /// Writes a value unless it would only spell out what the Profile already
    /// means by leaving the key out.
    fn put(&mut self, present: bool, is_fallback: bool, set: impl FnOnce(&mut ProfileDocument)) {
        if present || !is_fallback || self.doc.is_new() {
            set(&mut self.doc);
        }
    }

    fn travel_funded(&mut self) -> anyhow::Result<()> {
        self.say_if_asking(
            "\nThe Travel-funded rule: some organisers pay for part or all of your travel or \
             lodging. With this rule on, a Travel-funded Hackathon counts anywhere in the world, \
             even outside your Countries and over your Flight budget. Your Agent then also \
             checks listings from every other country, so Runs take a little longer.",
        )?;
        let current = self.doc.get_bool("travel.travel_funded_anywhere");
        // On by default in a new Profile: it is what people tracking
        // travel-funded Hackathons want. An existing Profile keeps its rule.
        let fallback = self.doc.is_new();
        let on = self.yes_no(
            "Count Travel-funded Hackathons anywhere in the world?",
            self.flags.travel_funded_anywhere,
            current.unwrap_or(fallback),
        )?;
        self.put(current.is_some(), !on, |doc| {
            doc.set_bool("travel.travel_funded_anywhere", on)
        });
        Ok(())
    }

    fn themes(&mut self) -> anyhow::Result<()> {
        self.say_if_asking("\nThemes: subjects you want Hackathons about.")?;
        let current = self.doc.get_strings("themes").unwrap_or_default();
        let themes = self
            .answer(
                question(
                    "Themes, such as AI agents, climate (comma-separated, or none)",
                    "--themes <LIST>",
                    "your Themes",
                ),
                self.flags.themes.as_deref(),
                Some(current),
                &|t: &Vec<String>| show_list(t),
                &|raw| Ok(parse_list(raw)),
            )?
            .unwrap_or_default();
        if themes.is_empty() {
            self.doc.remove("themes");
        } else {
            self.doc.set_strings("themes", &themes);
        }
        Ok(())
    }

    fn eligibility(&mut self) -> anyhow::Result<()> {
        self.say_if_asking(
            "\nEligibility: facts organisers restrict on. Answer none for any you'd rather not \
             give; your Agent then treats it as no obstacle.",
        )?;
        let current = self.doc.get_bool("eligibility.student");
        let student = self.answer(
            question(
                "Are you a student? (yes, no or none)",
                "--student <BOOL>",
                "whether you are a student",
            ),
            self.flags.student.as_deref(),
            Some(current),
            &|s: &Option<bool>| match s {
                Some(true) => "yes".into(),
                Some(false) => "no".into(),
                None => "none".into(),
            },
            &|raw| optional(raw, parse_bool),
        )?;
        match student.flatten() {
            Some(student) => self.doc.set_bool("eligibility.student", student),
            None => self.doc.remove("eligibility.student"),
        }

        let current = self
            .doc
            .get_strings("eligibility.nationalities")
            .unwrap_or_default();
        let nationalities = self
            .answer(
                question(
                    "Nationalities, such as Belgian (comma-separated, or none)",
                    "--nationalities <LIST>",
                    "your nationalities",
                ),
                self.flags.nationalities.as_deref(),
                Some(current),
                &|n: &Vec<String>| show_list(n),
                &|raw| Ok(parse_list(raw)),
            )?
            .unwrap_or_default();
        if nationalities.is_empty() {
            self.doc.remove("eligibility.nationalities");
        } else {
            self.doc
                .set_strings("eligibility.nationalities", &nationalities);
        }

        let current = self.doc.get_int("eligibility.age");
        let age = self.answer(
            question("Age (or none)", "--age <AGE>", "your age"),
            self.flags.age.as_deref(),
            Some(current),
            &|a: &Option<i64>| a.map_or_else(|| "none".into(), |a| a.to_string()),
            &|raw| {
                optional(raw, |v| match v.parse::<u32>() {
                    Ok(age) if age > 0 && age < 130 => Ok(i64::from(age)),
                    _ => Err("Give your age in years, or none.".into()),
                })
            },
        )?;
        match age.flatten() {
            Some(age) => self.doc.set_int("eligibility.age", age),
            None => self.doc.remove("eligibility.age"),
        }
        Ok(())
    }

    fn agent(&mut self) -> anyhow::Result<()> {
        let claude = (self.env.find_program)("claude");
        let codex = (self.env.find_program)("codex");
        if self.interactive {
            let found = |name: &str, at: &Option<PathBuf>| match at {
                Some(path) => format!("  Found {name} at {}.", path.display()),
                None => format!("  {name} isn't on your PATH."),
            };
            let text = format!(
                "\nThe Agent: your own coding agent, which judges listings on each Run. It may \
                 only search and read the web.\n{}\n{}\n  Choose claude-code (Claude Code), \
                 codex (Codex, experimental) or command (any other coding agent, from a command \
                 template).",
                found("claude", &claude),
                found("codex", &codex)
            );
            self.say(&text)?;
        }
        let current = self.doc.get_str("agent.adapter");
        let experimental =
            self.flags.experimental || self.doc.get_bool("agent.experimental") == Some(true);
        let adapter = loop {
            let adapter = self
                .answer(
                    question(
                        "Agent (claude-code, codex or command)",
                        "--agent <ADAPTER>",
                        "your Agent",
                    ),
                    self.flags.agent.as_deref(),
                    Some(current.clone().unwrap_or_else(|| "claude-code".into())),
                    &|a: &String| a.clone(),
                    &|raw| match raw {
                        "claude-code" | "codex" | "command" => Ok(raw.to_owned()),
                        "claude" => Ok("claude-code".to_owned()),
                        _ => Err("Answer claude-code, codex or command.".into()),
                    },
                )?
                .unwrap_or_else(|| "claude-code".into());
            if adapter != "codex" || experimental {
                break adapter;
            }
            if !self.interactive {
                self.missing.push(
                    "--experimental: the Codex adapter is experimental (its read-only lockdown \
                     is unverified against a real Codex CLI), so it needs your explicit opt-in"
                        .into(),
                );
                break adapter;
            }
            self.say(
                "  The Codex adapter is experimental: its read-only lockdown comes from OpenAI's \
                 documentation and is unverified against a real Codex CLI. If Codex ignored one \
                 of its settings, it could keep a shell that reads your whole disk while it reads \
                 arbitrary web pages.",
            )?;
            if self.yes_no("Opt in to the experimental Codex adapter?", None, false)? {
                break adapter;
            }
            if self.flags.agent.is_some() {
                self.problems
                    .push("--agent codex: you didn't opt in to the experimental adapter".into());
                break adapter;
            }
        };
        let present = current.is_some();
        self.put(present, adapter == "claude-code", |doc| {
            doc.set_str("agent.adapter", &adapter)
        });

        match adapter.as_str() {
            "claude-code" => {
                if claude.is_none() {
                    self.say(
                        "  Install Claude Code (https://claude.com/claude-code) before your first \
                         Run, so `claude` is on your PATH.",
                    )?;
                }
                self.say_if_asking(
                    "  Claude Code runs on your own `claude` sign-in: run `claude` once yourself \
                     and sign in with your subscription or an Anthropic Console account. \
                     hackathon-radar never stores your Anthropic key. ANTHROPIC_API_KEY in your \
                     shell works for Runs you start by hand, but scheduled Runs don't see your \
                     shell's environment, so sign `claude` in for those.",
                )?;
            }
            "codex" => {
                self.doc.set_bool("agent.experimental", true);
                if codex.is_none() {
                    self.say("  Install Codex before your first Run, so `codex` is on your PATH.")?;
                }
                self.say_if_asking(
                    "  Codex runs on your own `codex` sign-in: run `codex login` once yourself.",
                )?;
            }
            _ => {}
        }

        if adapter == "command" {
            self.say_if_asking(
                "  Your command runs without a shell, gets the prompt on stdin (or {prompt}), and \
                 must print only JSON matching the schema. It must be read-only: web search and \
                 page reading, nothing else. Placeholders: {schema_file}, {schema}, {prompt}, \
                 {max_turns}.",
            )?;
            let current = self
                .doc
                .get_strings("agent.command")
                .filter(|c| !c.is_empty());
            if let Some(command) = self.answer(
                question(
                    "Your agent's command (split on spaces)",
                    "--agent-command <COMMAND>",
                    "the command that runs your agent",
                ),
                self.flags.agent_command.as_deref(),
                current,
                &|c: &Vec<String>| c.join(" "),
                &|raw| {
                    let parts: Vec<String> = raw.split_whitespace().map(str::to_owned).collect();
                    if parts.is_empty() {
                        Err("Give the program and its arguments.".into())
                    } else {
                        Ok(parts)
                    }
                },
            )? {
                self.doc.set_strings("agent.command", &command);
            }
        } else {
            self.doc.remove("agent.command");
        }
        Ok(())
    }

    fn found(&self, name: SecretName) -> anyhow::Result<Option<Origin>> {
        Ok(self.env.secrets.find(name)?.map(|found| found.origin))
    }

    fn serpapi(&mut self) -> anyhow::Result<()> {
        if !self.interactive || self.pending(SecretName::SerpApiKey) {
            return Ok(());
        }
        if self.doc.get_int("travel.flight_budget").is_none() {
            return Ok(());
        }
        let origin = self.found(SecretName::SerpApiKey)?;
        self.say(&format!(
            "\nFares: with your own SerpApi key, Fares are real Google Flights prices; without \
             one, your Agent estimates them and they are marked unverified. SerpApi's free plan \
             has 250 searches a month, which is enough: {SERPAPI_SIGN_UP}"
        ))?;
        if let Some(origin) = &origin {
            self.say(&format!("  Your SerpApi key is in {origin}."))?;
            if let Origin::Env(var) = origin {
                self.say(&format!(
                    "  {var} wins over the keychain, but scheduled Runs don't see your shell, so \
                     store the key here too."
                ))?;
            }
        }
        let prompt = if origin.is_some() {
            "SerpApi key (hidden; Enter keeps the one you have): "
        } else {
            "SerpApi key (hidden; Enter to skip): "
        };
        if let Some(key) = self.ask_secret(prompt)? {
            self.pending.push((SecretName::SerpApiKey, key));
        }
        Ok(())
    }

    fn alert_channel(&mut self) -> anyhow::Result<()> {
        self.say_if_asking(
            "\nThe Alert channel: where Alerts reach you.\n  photon: an iMessage from your own \
             Photon line, free, with a sign-up (the recommended one).\n  ntfy: a push \
             notification through the ntfy app, no account needed.\n  none: Alerts are only \
             printed.",
        )?;
        let current = self.doc.get_str("alert_channel.kind");
        let kind = self
            .answer(
                question(
                    "Alert channel (photon, ntfy or none)",
                    "--alert-channel <CHANNEL>",
                    "your Alert channel",
                ),
                self.flags.alert_channel.as_deref(),
                Some(current.unwrap_or_else(|| "none".into())),
                &|k: &String| k.clone(),
                &|raw| match raw {
                    "photon" | "ntfy" | "none" => Ok(raw.to_owned()),
                    _ => Err("Answer photon, ntfy or none.".into()),
                },
            )?
            .unwrap_or_else(|| "none".into());
        match kind.as_str() {
            "photon" => self.photon(),
            "ntfy" => self.ntfy(),
            _ => {
                self.doc.remove("alert_channel");
                Ok(())
            }
        }
    }

    fn photon(&mut self) -> anyhow::Result<()> {
        let dashboard = photon::DASHBOARD;
        self.say_if_asking(&format!(
            "  Photon setup:\n  1. Sign up at {dashboard}, create a project and turn on \
             iMessage (Spectrum). Note the project's ID and secret, and the iMessage line \
             Photon gives you.\n  2. Text your Photon line once from your phone, with \
             anything. A shared (free) line can only message a number that has texted it \
             first.\n  3. Give your number and the project's ID and secret below."
        ))?;
        let was_photon = self.doc.get_str("alert_channel.kind").as_deref() == Some("photon");
        let current = was_photon
            .then(|| self.doc.get_str("alert_channel.to"))
            .flatten();
        let to =
            self.answer(
                question(
                    "Your phone number, in E.164 format such as +32470123456",
                    "--photon-to <NUMBER>",
                    "the phone number Photon texts",
                ),
                self.flags.photon_to.as_deref(),
                current,
                &|t: &String| t.clone(),
                &|raw| {
                    if profile::is_photon_recipient(raw) {
                        Ok(raw.to_owned())
                    } else {
                        Err("Give your number in E.164 format: +, the country code and the number, \
                         without spaces, such as +32470123456."
                        .into())
                    }
                },
            )?;
        self.doc.set_str("alert_channel.kind", "photon");
        if let Some(to) = to {
            self.doc.set_str("alert_channel.to", &to);
        }
        self.doc.remove("alert_channel.topic");
        self.doc.remove("alert_channel.server");

        let piped = self.pending(SecretName::PhotonProjectId);
        if piped {
            return Ok(());
        }
        let id = self.found(SecretName::PhotonProjectId)?;
        let secret = self.found(SecretName::PhotonProjectSecret)?;
        let stored = id.is_some() && secret.is_some();
        if !self.interactive {
            if !stored {
                self.missing.push(
                    "--photon-credentials-stdin: your Photon project ID and secret, piped on \
                     stdin one per line (they're in the Photon dashboard)"
                        .into(),
                );
            }
            return Ok(());
        }
        if let (Some(id), true) = (&id, stored) {
            self.say(&format!(
                "  Your Photon project credentials are in {id}; press Enter to keep them."
            ))?;
        }
        for (name, label) in [
            (SecretName::PhotonProjectId, "Photon project ID"),
            (SecretName::PhotonProjectSecret, "Photon project secret"),
        ] {
            let has = self.found(name)?.is_some();
            loop {
                let prompt = if has {
                    format!("{label} (hidden; Enter keeps it): ")
                } else {
                    format!("{label} (hidden): ")
                };
                match self.ask_secret(&prompt)? {
                    Some(value) => {
                        self.pending.push((name, value));
                        break;
                    }
                    None if has => break,
                    None => self.say(&format!(
                        "  This one is needed: it's in the Photon dashboard ({dashboard})."
                    ))?,
                }
            }
        }
        Ok(())
    }

    fn ntfy(&mut self) -> anyhow::Result<()> {
        self.say_if_asking(
            "  ntfy setup: install the ntfy app on your phone and subscribe to a topic. Anyone \
             who knows a topic on ntfy.sh can read it, so pick a long, hard-to-guess one, such \
             as the suggestion below.",
        )?;
        let was_ntfy = self.doc.get_str("alert_channel.kind").as_deref() == Some("ntfy");
        let current_topic = was_ntfy
            .then(|| self.doc.get_str("alert_channel.topic"))
            .flatten();
        // A suggestion is only useful to someone who can copy it into the app.
        let default = current_topic.or_else(|| self.interactive.then(random_topic));
        let topic = self.answer(
            question(
                "ntfy topic",
                "--ntfy-topic <TOPIC>",
                "the ntfy topic you subscribed to",
            ),
            self.flags.ntfy_topic.as_deref(),
            default,
            &|t: &String| t.clone(),
            &|raw| {
                if profile::is_ntfy_topic(raw) {
                    Ok(raw.to_owned())
                } else {
                    Err("Use up to 64 letters, digits, - or _.".into())
                }
            },
        )?;
        let current_server = was_ntfy
            .then(|| self.doc.get_str("alert_channel.server"))
            .flatten();
        let server = self
            .answer(
                question("ntfy server", "--ntfy-server <URL>", "your ntfy server"),
                self.flags.ntfy_server.as_deref(),
                Some(
                    current_server
                        .clone()
                        .unwrap_or_else(|| DEFAULT_NTFY_SERVER.into()),
                ),
                &|s: &String| s.clone(),
                &|raw| {
                    let server = raw.trim_end_matches('/');
                    if (server.starts_with("https://") || server.starts_with("http://"))
                        && !server.contains(char::is_whitespace)
                    {
                        Ok(server.to_owned())
                    } else {
                        Err("Give the server's address, such as https://ntfy.example.org.".into())
                    }
                },
            )?
            .unwrap_or_else(|| DEFAULT_NTFY_SERVER.into());

        self.doc.set_str("alert_channel.kind", "ntfy");
        if let Some(topic) = topic {
            self.doc.set_str("alert_channel.topic", &topic);
        }
        if current_server.is_some() || server != DEFAULT_NTFY_SERVER {
            self.doc.set_str("alert_channel.server", &server);
        }
        self.doc.remove("alert_channel.to");

        // ntfy.sh needs no token, so it is only asked for on another server.
        let stored = self.found(SecretName::NtfyToken)?;
        if self.interactive
            && !self.pending(SecretName::NtfyToken)
            && (server != DEFAULT_NTFY_SERVER || stored.is_some())
        {
            let prompt = if stored.is_some() {
                "Access token for your ntfy server (hidden; Enter keeps the stored one): "
            } else {
                "Access token, if your ntfy server needs one (hidden; Enter to skip): "
            };
            if let Some(token) = self.ask_secret(prompt)? {
                self.pending.push((SecretName::NtfyToken, token));
            }
        }
        Ok(())
    }

    /// Installs the Photon sidecar's dependencies, once.
    fn install_sidecar(&mut self, profile: &Profile) -> anyhow::Result<()> {
        if !matches!(
            profile.alert_channel,
            Some(profile::AlertChannelSetting::Photon(_))
        ) || self.flags.skip_npm_ci
        {
            return Ok(());
        }
        let dir = self.env.sidecar.dir.clone();
        if dir.join("node_modules").is_dir() {
            self.say(&format!(
                "The Photon sidecar's dependencies are already installed in {}.",
                dir.display()
            ))?;
            return Ok(());
        }
        if (self.env.find_program)("npm").is_none() {
            self.say(&format!(
                "Photon Alerts need Node.js {} or later, and npm isn't on your PATH. Install \
                 Node from https://nodejs.org, then run `npm ci` in {}.",
                photon::MIN_NODE_VERSION,
                dir.display()
            ))?;
            return Ok(());
        }
        self.say(&format!(
            "Installing the Photon sidecar's pinned dependencies (`npm ci` in {})…",
            dir.display()
        ))?;
        let result = self.env.runner.run(&Command {
            program: "npm".into(),
            args: vec!["ci".into()],
            stdin: String::new(),
            working_dir: Some(dir.clone()),
            timeout: NPM_CI_TIMEOUT,
            env: Vec::new(),
        });
        let message = match result {
            Ok(output) if output.success() => "Installed them.".to_owned(),
            Ok(output) => format!(
                "`npm ci` failed (exit code {}): {}. Run it yourself in {}.",
                output.code.map_or("none".into(), |c| c.to_string()),
                last_line(&output.stderr),
                dir.display()
            ),
            Err(error) => format!(
                "Could not run `npm ci`: {error:#}. Run it yourself in {}.",
                dir.display()
            ),
        };
        self.say(&message)
    }

    fn test_alert(&mut self, profile: &Profile) -> anyhow::Result<Option<Result<(), String>>> {
        if profile.alert_channel.is_none() {
            self.say(
                "No Alert channel is set, so Alerts are only printed; run init again to add one.",
            )?;
            return Ok(None);
        }
        let photon = matches!(
            profile.alert_channel,
            Some(profile::AlertChannelSetting::Photon(_))
        );
        if photon && self.interactive && self.flags.test_alert.is_none() {
            self.say(
                "Before the test Alert: have you texted your Photon line from your phone? Photon \
                 can only message a number that has texted it first.",
            )?;
        }
        if !self.yes_no("Send a test Alert now?", self.flags.test_alert, true)? {
            return Ok(None);
        }
        let name = wiring::channel_name(profile);
        let sent = wiring::alert_channel(
            profile,
            self.env.secrets,
            false,
            self.env.http,
            Runner(self.env.runner),
            self.env.sidecar.clone(),
        )
        .and_then(|mut channel| wiring::send_test_alert(channel.as_mut()));
        Ok(Some(match sent {
            Ok(()) => {
                self.say(&format!(
                    "Sent a test Alert through {name}. Check your phone: if it didn't arrive, \
                     run `hackathon-radar test-alert` once you've fixed the cause."
                ))?;
                Ok(())
            }
            Err(error) => {
                let error = format!("{error:#}");
                self.say(&format!(
                    "The test Alert through {name} failed: {error}\nYour Profile is saved; fix \
                     this, then run `hackathon-radar test-alert`."
                ))?;
                Err(error)
            }
        }))
    }

    fn schedule(
        &mut self,
        path: &Path,
        profile: &Profile,
    ) -> anyhow::Result<Option<Result<(), String>>> {
        // `--yes` never changes launchd or systemd unless asked to.
        let default = self.interactive;
        let prompt = format!("Install the schedule now: a Run {}?", profile.schedule);
        if !self.yes_no(&prompt, self.flags.install_schedule, default)? {
            self.say("When you're ready, run `hackathon-radar schedule install`.")?;
            return Ok(None);
        }
        Ok(Some(match (self.env.install_schedule)(path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let error = format!("{error:#}");
                self.say(&format!("Could not install the schedule: {error}"))?;
                Err(error)
            }
        }))
    }
}

fn non_empty(raw: &str, why: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        Err(why.to_owned())
    } else {
        Ok(raw.to_owned())
    }
}

/// `none` means no value; anything else goes through `parse`.
fn optional<T>(raw: &str, parse: impl Fn(&str) -> Result<T, String>) -> Result<Option<T>, String> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("none") || raw.is_empty() {
        Ok(None)
    } else {
        parse(raw).map(Some)
    }
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "true" | "on" => Ok(true),
        "n" | "no" | "false" | "off" => Ok(false),
        _ => Err("Answer yes or no.".into()),
    }
}

/// A comma-separated list, trimmed, without empty entries; `none` is empty.
fn parse_list(raw: &str) -> Vec<String> {
    if raw.trim().eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn show_list(list: &[String]) -> String {
    if list.is_empty() {
        "none".into()
    } else {
        list.join(", ")
    }
}

fn parse_airports(raw: &str) -> Result<Vec<String>, String> {
    let codes: Vec<String> = raw
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    if codes.is_empty() {
        return Err("Give at least one airport code, such as BRU.".into());
    }
    if let Some(bad) = codes
        .iter()
        .find(|c| c.len() != 3 || !c.bytes().all(|b| b.is_ascii_uppercase()))
    {
        return Err(format!(
            "{bad} is not a 3-letter IATA airport code, such as BRU."
        ));
    }
    Ok(codes)
}

fn last_line(text: &str) -> String {
    text.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no output")
        .trim()
        .to_owned()
}

/// A hard-to-guess ntfy topic, such as `hackathon-radar-k3f9q2m8x1zt`.
fn random_topic() -> String {
    use std::hash::{BuildHasher, Hasher};
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    hasher.write_u128(nanos);
    hasher.write_u32(std::process::id());
    let mut value = hasher.finish();
    let mut topic = String::from("hackathon-radar-");
    for _ in 0..12 {
        topic.push(ALPHABET[(value % 36) as usize] as char);
        value /= 36;
    }
    topic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_airports_and_booleans() {
        assert_eq!(
            parse_list(" AI agents, ,climate "),
            ["AI agents", "climate"]
        );
        assert!(parse_list("None").is_empty());
        assert_eq!(parse_airports("bru, crl").unwrap(), ["BRU", "CRL"]);
        assert_eq!(parse_airports("BRU CRL").unwrap(), ["BRU", "CRL"]);
        assert!(parse_airports("BRUX").is_err());
        assert_eq!(parse_bool("Yes"), Ok(true));
        assert_eq!(parse_bool("n"), Ok(false));
        assert!(parse_bool("maybe").is_err());
        assert_eq!(optional("none", parse_bool), Ok(None));
    }

    #[test]
    fn random_topics_are_valid_and_differ() {
        let a = random_topic();
        assert!(profile::is_ntfy_topic(&a), "{a}");
        assert!(a.starts_with("hackathon-radar-"));
        assert_ne!(a, random_topic());
    }

    #[test]
    fn finds_programs_on_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("claude");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([Path::new("/nonexistent"), dir.path()]).unwrap();
        assert_eq!(find_on_path("claude", Some(&path)), Some(program));
        assert_eq!(find_on_path("codex", Some(&path)), None);
        assert_eq!(find_on_path("claude", None), None);
    }
}
