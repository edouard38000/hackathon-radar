//! The Profile: one person's settings, kept in a human-readable TOML file with no secrets.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{NaiveTime, Weekday};
use serde::Deserialize;

use crate::agent::command::CommandTemplate;

mod travel;

pub use travel::{DEFAULT_TRAIN_LIMIT_MINUTES, FlightOverride, TravelOverride, TravelSettings};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub home: Home,
    /// Country names, such as `Belgium` or `France`.
    pub countries: Vec<String>,
    /// Where Alerts go. Without one, Alerts are printed instead of sent.
    pub alert_channel: Option<AlertChannelSetting>,
    /// When scheduled Runs happen.
    pub schedule: Schedule,
    pub eligibility: Eligibility,
    pub agent: AgentSettings,
    /// The Train limit and the person's corrections to travel estimates.
    pub travel: TravelSettings,
    /// Extra Sources and how much the built-in ones fetch.
    pub sources: SourceSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    pub city: String,
    /// The country Home is in. Optional: without it, only trips within Home's
    /// city count as local.
    pub country: Option<String>,
    /// IATA codes of the airports the person flies out of, upper-cased.
    pub airports: Vec<String>,
}

/// The Alert channel a Profile names, with its settings. Secrets never live here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertChannelSetting {
    Ntfy(NtfySetting),
    Photon(PhotonSetting),
}

/// iMessage from the person's own Photon line (ADR 0005). The project
/// credentials are secrets, so they live outside the Profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotonSetting {
    /// Who the Alert goes to: a phone number in E.164 format, such as
    /// `+32470123456`, or a Photon space ID.
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfySetting {
    /// The server's base URL, without a trailing slash.
    pub server: String,
    pub topic: String,
}

pub const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";
/// The facts about the person that organisers restrict on. Each is optional;
/// the Agent treats a fact that isn't given as no obstacle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Eligibility {
    /// Whether the person is a student.
    pub student: Option<bool>,
    /// Nationalities, such as `French`.
    #[serde(default)]
    pub nationalities: Vec<String>,
    pub age: Option<u32>,
}

/// Which Agent judges Candidates, and how far it may go on one Run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSettings {
    #[serde(default)]
    pub adapter: AgentAdapter,
    /// The most Agent steps (turns) one Run may use.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// The command template for `adapter = "command"`; set only with that adapter.
    #[serde(default)]
    pub command: Option<CommandTemplate>,
    /// Opts in to an experimental adapter (Codex); without it, the Profile refuses one.
    #[serde(default)]
    pub experimental: bool,
    /// The most Agent steps the travel estimates request may use (ADR 0004).
    #[serde(default = "default_travel_max_turns")]
    pub travel_max_turns: u32,
}

/// The person's own Sources, and limits on the built-in ones.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSettings {
    /// ICS calendar feeds the person adds, such as a community's calendar.
    #[serde(default)]
    pub ics: Vec<String>,
    /// The most new hackathonhub.eu event pages one Run fetches.
    #[serde(default = "default_hackathonhub_pages_per_run")]
    pub hackathonhub_pages_per_run: usize,
}

fn default_hackathonhub_pages_per_run() -> usize {
    crate::sources::hackathonhub::DEFAULT_PAGES_PER_RUN
}

impl Default for SourceSettings {
    fn default() -> Self {
        Self {
            ics: Vec::new(),
            hackathonhub_pages_per_run: default_hackathonhub_pages_per_run(),
        }
    }
}

/// The Agents hackathon-radar can run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentAdapter {
    /// Claude Code, run headless with `claude -p`.
    #[default]
    ClaudeCode,
    /// Codex, run headless with `codex exec` in a read-only sandbox.
    Codex,
    /// Any other coding agent, run from the command template in `agent.command`.
    Command,
}

impl AgentAdapter {
    /// The name the Profile uses for it.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentAdapter::ClaudeCode => "claude-code",
            AgentAdapter::Codex => "codex",
            AgentAdapter::Command => "command",
        }
    }
}

/// Enough for a Run's batch of Candidates, each needing a page read or a search or two.
pub const DEFAULT_MAX_TURNS: u32 = 50;

fn default_max_turns() -> u32 {
    DEFAULT_MAX_TURNS
}

/// Enough to estimate a Run's batch of destination cities, most of which need
/// no more than a search.
pub const DEFAULT_TRAVEL_MAX_TURNS: u32 = 15;

fn default_travel_max_turns() -> u32 {
    DEFAULT_TRAVEL_MAX_TURNS
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            adapter: AgentAdapter::default(),
            max_turns: DEFAULT_MAX_TURNS,
            command: None,
            experimental: false,
            travel_max_turns: DEFAULT_TRAVEL_MAX_TURNS,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error(
        "no Profile at {}; create one (see examples/profile.toml) or pass --profile <PATH>",
        path.display()
    )]
    NotFound { path: PathBuf },
    #[error("could not read the Profile at {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("the Profile at {} is not valid TOML: {message}", path.display())]
    Syntax { path: PathBuf, message: String },
    #[error("the Profile at {} is invalid:\n{}", path.display(), Problems(problems))]
    Invalid {
        path: PathBuf,
        problems: Vec<String>,
    },
}

struct Problems<'a>(&'a [String]);

impl fmt::Display for Problems<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<String> = self.0.iter().map(|p| format!("  - {p}")).collect();
        f.write_str(&lines.join("\n"))
    }
}

// The file's shape. Unknown keys are refused so a typo is reported instead of ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    home: HomeFile,
    countries: Vec<String>,
    alert_channel: Option<AlertChannelFile>,
    #[serde(default)]
    schedule: ScheduleFile,
    #[serde(default)]
    eligibility: Eligibility,
    #[serde(default)]
    agent: AgentSettings,
    #[serde(default)]
    travel: travel::TravelFile,
    #[serde(default)]
    sources: SourceSettings,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeFile {
    city: String,
    country: Option<String>,
    airports: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlertChannelFile {
    kind: String,
    topic: Option<String>,
    server: Option<String>,
    to: Option<String>,
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = std::fs::read_to_string(path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                ProfileError::NotFound {
                    path: path.to_owned(),
                }
            } else {
                ProfileError::Read {
                    path: path.to_owned(),
                    source,
                }
            }
        })?;
        Self::from_toml(&text, path)
    }

    /// Parses and validates Profile text; `path` only labels errors.
    pub fn from_toml(text: &str, path: &Path) -> Result<Self, ProfileError> {
        let file: ProfileFile = toml::from_str(text).map_err(|e| ProfileError::Syntax {
            path: path.to_owned(),
            message: e.to_string().trim_end().to_owned(),
        })?;
        let mut problems = Vec::new();

        let city = file.home.city.trim().to_owned();
        if city.is_empty() {
            problems.push("home.city is empty; give the city you travel from".to_owned());
        }
        let home_country = file.home.country.as_deref().map(str::trim);
        if home_country == Some("") {
            problems.push(
                "home.country is empty; give the country Home is in, or leave it out".to_owned(),
            );
        }
        let home_country = home_country.map(str::to_owned);

        let airports: Vec<String> = file
            .home
            .airports
            .iter()
            .map(|a| a.trim().to_ascii_uppercase())
            .collect();
        if airports.is_empty() {
            problems.push(
                "home.airports is empty; list at least one airport code, such as \"BRU\""
                    .to_owned(),
            );
        }
        for (raw, code) in file.home.airports.iter().zip(&airports) {
            if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
                problems.push(format!(
                    "home.airports: {raw:?} is not a 3-letter IATA airport code"
                ));
            }
        }

        let countries: Vec<String> = file.countries.iter().map(|c| c.trim().to_owned()).collect();
        if countries.is_empty() {
            problems.push(
                "countries is empty; list at least one country, such as \"Belgium\"".to_owned(),
            );
        }
        if countries.iter().any(String::is_empty) {
            problems.push("countries has an empty entry".to_owned());
        }

        let alert_channel = file
            .alert_channel
            .and_then(|channel| alert_channel(channel, &mut problems));

        let schedule = Schedule::from_file(file.schedule, &mut problems);
        let mut eligibility = file.eligibility;
        eligibility.nationalities = eligibility
            .nationalities
            .iter()
            .map(|n| n.trim().to_owned())
            .collect();
        if eligibility.nationalities.iter().any(String::is_empty) {
            problems.push("eligibility.nationalities has an empty entry".to_owned());
        }

        if file.agent.max_turns == 0 {
            problems.push("agent.max_turns is 0; the Agent needs at least one step".to_owned());
        }
        if file.agent.adapter == AgentAdapter::Codex && !file.agent.experimental {
            problems.push(
                "agent.adapter = \"codex\" is experimental: the Codex adapter's read-only lockdown \
                 is unverified against a real Codex CLI. Add experimental = true under [agent] to \
                 opt in knowingly, or use adapter = \"claude-code\""
                    .to_owned(),
            );
        }
        match (file.agent.adapter, &file.agent.command) {
            (AgentAdapter::Command, None) => problems.push(
                "agent.command is missing; adapter = \"command\" runs the command you give, \
                 such as command = [\"my-agent\", \"--schema\", \"{schema_file}\"]"
                    .to_owned(),
            ),
            (AgentAdapter::Command, Some(_)) | (_, None) => {}
            (other, Some(_)) => problems.push(format!(
                "agent.command is only used with adapter = \"command\", but agent.adapter is \
                 {:?}; remove agent.command or set adapter = \"command\"",
                other.as_str()
            )),
        }
        if file.agent.travel_max_turns == 0 {
            problems
                .push("agent.travel_max_turns is 0; the Agent needs at least one step".to_owned());
        }
        let travel = TravelSettings::from_file(file.travel, &mut problems);

        let mut sources = file.sources;
        sources.ics = sources.ics.iter().map(|u| u.trim().to_owned()).collect();
        for url in &sources.ics {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                problems.push(format!(
                    "sources.ics: {url:?} is not a link; give the calendar's https:// address"
                ));
            }
        }

        if !problems.is_empty() {
            return Err(ProfileError::Invalid {
                path: path.to_owned(),
                problems,
            });
        }
        Ok(Profile {
            home: Home {
                city,
                country: home_country,
                airports,
            },
            countries,
            alert_channel,
            schedule,
            eligibility,
            agent: file.agent,
            travel,
            sources,
        })
    }

    pub fn includes_country(&self, country: &str) -> bool {
        let country = country.trim();
        self.countries
            .iter()
            .any(|c| c.to_lowercase() == country.to_lowercase())
    }
}

fn alert_channel(
    file: AlertChannelFile,
    problems: &mut Vec<String>,
) -> Option<AlertChannelSetting> {
    match file.kind.trim() {
        "ntfy" => ntfy(file, problems).map(AlertChannelSetting::Ntfy),
        "photon" => photon(file, problems).map(AlertChannelSetting::Photon),
        other => {
            problems.push(format!(
                "alert_channel.kind: {other:?} is not an Alert channel; use \"photon\" or \"ntfy\""
            ));
            None
        }
    }
}

fn photon(file: AlertChannelFile, problems: &mut Vec<String>) -> Option<PhotonSetting> {
    let before = problems.len();
    for (key, value) in [("topic", &file.topic), ("server", &file.server)] {
        if value.is_some() {
            problems.push(format!(
                "alert_channel.{key} is only for kind = \"ntfy\"; Photon needs just `to`"
            ));
        }
    }

    let to = file.to.as_deref().map(str::trim).unwrap_or_default();
    if to.is_empty() {
        problems.push(
            "alert_channel.to is missing; Photon needs your phone number in E.164 format, such as \"+32470123456\""
                .to_owned(),
        );
    } else if !is_photon_recipient(to) {
        problems.push(format!(
            "alert_channel.to: {to:?} is not a phone number in E.164 format, such as \"+32470123456\", \
             or a Photon space ID"
        ));
    }

    (problems.len() == before).then(|| PhotonSetting { to: to.to_owned() })
}

/// A phone number in E.164 format (`+`, then 7 to 15 digits with no leading
/// zero), or anything else without spaces, taken as a Photon space ID.
fn is_photon_recipient(to: &str) -> bool {
    match to.strip_prefix('+') {
        Some(digits) => {
            (7..=15).contains(&digits.len())
                && digits.bytes().all(|b| b.is_ascii_digit())
                && !digits.starts_with('0')
        }
        None => !to.contains(char::is_whitespace),
    }
}

fn ntfy(file: AlertChannelFile, problems: &mut Vec<String>) -> Option<NtfySetting> {
    let before = problems.len();
    if file.to.is_some() {
        problems.push(
            "alert_channel.to is only for kind = \"photon\"; ntfy sends to its topic".to_owned(),
        );
    }

    let topic = file.topic.as_deref().map(str::trim).unwrap_or_default();
    if topic.is_empty() {
        problems.push(
            "alert_channel.topic is missing; ntfy needs a topic, such as \"hackathon-radar-7f3k9q\""
                .to_owned(),
        );
    } else if topic.len() > 64
        || !topic
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        // ntfy's own rule for topic names, which also keeps the topic safe in a URL.
        problems.push(format!(
            "alert_channel.topic: {topic:?} is not an ntfy topic; use up to 64 letters, digits, - or _"
        ));
    }

    let server = file
        .server
        .as_deref()
        .map(|s| s.trim().trim_end_matches('/'))
        .unwrap_or(DEFAULT_NTFY_SERVER);
    let has_scheme = server.starts_with("https://") || server.starts_with("http://");
    if !has_scheme || server.contains(char::is_whitespace) {
        problems.push(format!(
            "alert_channel.server: {server:?} is not a URL; leave it out to use {DEFAULT_NTFY_SERVER}"
        ));
    }

    (problems.len() == before).then(|| NtfySetting {
        server: server.to_owned(),
        topic: topic.to_owned(),
    })
}

/// When scheduled Runs happen: on these days, at this local time. It is read from
/// the optional `[schedule]` section and defaults to Monday and Thursday at 08:00.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// Monday first, each day once.
    pub days: Vec<Weekday>,
    /// Local time, to the minute.
    pub time: NaiveTime,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            days: vec![Weekday::Mon, Weekday::Thu],
            time: NaiveTime::from_hms_opt(8, 0, 0).expect("08:00 is a valid time"),
        }
    }
}

impl fmt::Display for Schedule {
    /// Such as "every Monday and Thursday at 08:00".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.days.iter().map(|d| day_name(*d)).collect();
        let days = match names.split_last() {
            Some((last, [])) => (*last).to_owned(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
            None => String::new(),
        };
        write!(f, "every {days} at {}", self.time.format("%H:%M"))
    }
}

fn day_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ScheduleFile {
    days: Option<Vec<String>>,
    time: Option<String>,
}

impl Schedule {
    fn from_file(file: ScheduleFile, problems: &mut Vec<String>) -> Self {
        let default = Schedule::default();

        let days = match file.days {
            None => default.days,
            Some(raw_days) => {
                if raw_days.is_empty() {
                    problems.push(
                        "schedule.days is empty; list at least one day, such as \"Monday\""
                            .to_owned(),
                    );
                }
                let mut days = Vec::new();
                for raw in &raw_days {
                    match raw.trim().parse::<Weekday>() {
                        Ok(day) if !days.contains(&day) => days.push(day),
                        Ok(_) => {}
                        Err(_) => problems.push(format!(
                            "schedule.days: {raw:?} is not a day of the week, such as \"Monday\""
                        )),
                    }
                }
                days.sort_by_key(Weekday::num_days_from_monday);
                days
            }
        };

        let time = match file.time {
            None => default.time,
            Some(raw) => NaiveTime::parse_from_str(raw.trim(), "%H:%M").unwrap_or_else(|_| {
                problems.push(format!(
                    "schedule.time: {raw:?} is not a 24-hour local time such as \"08:00\""
                ));
                default.time
            }),
        };

        Schedule { days, time }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Profile, ProfileError> {
        Profile::from_toml(text, Path::new("profile.toml"))
    }

    #[test]
    fn reads_a_valid_profile() {
        let profile = parse(
            r#"
            countries = ["Belgium", " France "]

            [home]
            city = "Brussels"
            airports = ["bru", "CRL"]
            "#,
        )
        .unwrap();
        assert_eq!(
            profile,
            Profile {
                home: Home {
                    city: "Brussels".into(),
                    country: None,
                    airports: vec!["BRU".into(), "CRL".into()],
                },
                countries: vec!["Belgium".into(), "France".into()],
                alert_channel: None,
                schedule: Schedule::default(),
                eligibility: Eligibility::default(),
                agent: AgentSettings {
                    adapter: AgentAdapter::ClaudeCode,
                    max_turns: DEFAULT_MAX_TURNS,
                    command: None,
                    experimental: false,
                    travel_max_turns: DEFAULT_TRAVEL_MAX_TURNS,
                },
                travel: TravelSettings::default(),
                sources: SourceSettings::default(),
            }
        );
        assert!(profile.includes_country("belgium"));
        assert!(!profile.includes_country("Germany"));
    }

    #[test]
    fn lists_every_problem_at_once() {
        let err = parse(
            r#"
            countries = []

            [home]
            city = " "
            airports = ["BRUX"]
            "#,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - home.city is empty; give the city you travel from\n\
             \x20 - home.airports: \"BRUX\" is not a 3-letter IATA airport code\n\
             \x20 - countries is empty; list at least one country, such as \"Belgium\""
        );
    }

    #[test]
    fn requires_an_airport() {
        let err = parse("countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = []\n")
            .unwrap_err();
        assert!(err.to_string().contains("home.airports is empty"), "{err}");
    }

    #[test]
    fn missing_keys_are_named() {
        let err = parse("[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n").unwrap_err();
        assert!(matches!(err, ProfileError::Syntax { .. }));
        assert!(err.to_string().contains("countries"), "{err}");
    }

    #[test]
    fn unknown_keys_are_refused() {
        let err = parse(
            "countries = [\"France\"]\ncountry = \"France\"\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("country"), "{err}");
    }

    #[test]
    fn reads_eligibility_and_the_agent_setting() {
        let profile = parse(
            r#"
            countries = ["Belgium"]

            [home]
            city = "Brussels"
            airports = ["BRU"]

            [eligibility]
            student = true
            nationalities = [" French "]
            age = 24

            [agent]
            adapter = "claude-code"
            max_turns = 12
            travel_max_turns = 6
            "#,
        )
        .unwrap();
        assert_eq!(
            profile.eligibility,
            Eligibility {
                student: Some(true),
                nationalities: vec!["French".into()],
                age: Some(24),
            }
        );
        assert_eq!(
            profile.agent,
            AgentSettings {
                adapter: AgentAdapter::ClaudeCode,
                max_turns: 12,
                command: None,
                experimental: false,
                travel_max_turns: 6,
            }
        );
    }

    #[test]
    fn the_agent_setting_selects_codex_or_a_command() {
        let base = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";
        let codex = parse(&format!(
            "{base}[agent]\nadapter = \"codex\"\nexperimental = true\n"
        ))
        .unwrap();
        assert_eq!(codex.agent.adapter, AgentAdapter::Codex);
        assert!(codex.agent.experimental);
        assert_eq!(codex.agent.command, None);

        let command = parse(&format!(
            "{base}[agent]\nadapter = \"command\"\ncommand = [\"my-agent\", \"--schema\", \"{{schema_file}}\"]\n"
        ))
        .unwrap();
        assert_eq!(command.agent.adapter, AgentAdapter::Command);
        let template = command.agent.command.expect("a command template");
        assert_eq!(template.program(), "my-agent");
    }

    #[test]
    fn codex_is_refused_unless_the_profile_opts_in_to_experimental_adapters() {
        let base = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";
        for agent in [
            "adapter = \"codex\"\n",
            "adapter = \"codex\"\nexperimental = false\n",
        ] {
            let err = parse(&format!("{base}[agent]\n{agent}")).unwrap_err();
            let message = err.to_string();
            assert!(matches!(err, ProfileError::Invalid { .. }), "{message}");
            assert!(
                message.contains("agent.adapter = \"codex\" is experimental"),
                "{message}"
            );
            assert!(
                message.contains("unverified against a real Codex CLI"),
                "{message}"
            );
            assert!(message.contains("experimental = true"), "{message}");
            assert!(message.contains("adapter = \"claude-code\""), "{message}");
        }
        // Claude Code, the default, needs no opt-in.
        assert!(!parse(base).unwrap().agent.experimental);
    }

    #[test]
    fn a_command_adapter_needs_a_valid_command_and_only_it_takes_one() {
        let base = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";
        let missing = parse(&format!("{base}[agent]\nadapter = \"command\"\n")).unwrap_err();
        assert!(
            missing.to_string().contains("agent.command is missing"),
            "{missing}"
        );

        let empty = parse(&format!(
            "{base}[agent]\nadapter = \"command\"\ncommand = []\n"
        ))
        .unwrap_err();
        assert!(
            empty
                .to_string()
                .contains("agent.command: the command is empty"),
            "{empty}"
        );

        let unknown = parse(&format!(
            "{base}[agent]\nadapter = \"command\"\ncommand = [\"my-agent\", \"{{schema_path}}\"]\n"
        ))
        .unwrap_err();
        assert!(
            unknown
                .to_string()
                .contains("{schema_path} in \"{schema_path}\" is not a placeholder"),
            "{unknown}"
        );

        let stray = parse(&format!(
            "{base}[agent]\nadapter = \"codex\"\nexperimental = true\ncommand = [\"my-agent\"]\n"
        ))
        .unwrap_err();
        assert!(
            stray
                .to_string()
                .contains("agent.command is only used with adapter = \"command\""),
            "{stray}"
        );
    }

    #[test]
    fn an_unknown_agent_or_a_zero_step_cap_is_refused() {
        let base = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";
        let err = parse(&format!("{base}[agent]\nadapter = \"skynet\"\n")).unwrap_err();
        assert!(err.to_string().contains("claude-code"), "{err}");
        assert!(err.to_string().contains("codex"), "{err}");
        assert!(err.to_string().contains("command"), "{err}");
        let err = parse(&format!("{base}[agent]\nmax_turns = 0\n")).unwrap_err();
        assert!(err.to_string().contains("agent.max_turns is 0"), "{err}");
    }

    #[test]
    fn the_travel_step_cap_defaults_to_15_and_cannot_be_zero() {
        let base = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";
        let profile = parse(base).unwrap();
        assert_eq!(profile.agent.travel_max_turns, 15);
        assert_eq!(DEFAULT_TRAVEL_MAX_TURNS, 15);
        let set = parse(&format!("{base}[agent]\ntravel_max_turns = 4\n")).unwrap();
        assert_eq!(set.agent.travel_max_turns, 4);
        assert_eq!(set.agent.max_turns, DEFAULT_MAX_TURNS);
        let err = parse(&format!("{base}[agent]\ntravel_max_turns = 0\n")).unwrap_err();
        assert!(
            err.to_string()
                .contains("agent.travel_max_turns is 0; the Agent needs at least one step"),
            "{err}"
        );
        let negative = parse(&format!("{base}[agent]\ntravel_max_turns = -3\n")).unwrap_err();
        assert!(
            matches!(negative, ProfileError::Syntax { .. }),
            "{negative}"
        );
    }

    #[test]
    fn a_missing_file_says_how_to_fix_it() {
        let err = Profile::load(Path::new("/nonexistent/profile.toml")).unwrap_err();
        assert!(matches!(err, ProfileError::NotFound { .. }));
        assert!(err.to_string().contains("--profile"), "{err}");
    }

    const HOME: &str = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";

    #[test]
    fn ntfy_defaults_to_ntfy_sh() {
        let profile = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar-42\"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.alert_channel,
            Some(AlertChannelSetting::Ntfy(NtfySetting {
                server: "https://ntfy.sh".into(),
                topic: "radar-42".into(),
            }))
        );
    }

    #[test]
    fn ntfy_takes_a_self_hosted_server() {
        let profile = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar\"\nserver = \"https://ntfy.example.org/\"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.alert_channel,
            Some(AlertChannelSetting::Ntfy(NtfySetting {
                server: "https://ntfy.example.org".into(),
                topic: "radar".into(),
            }))
        );
    }

    #[test]
    fn ntfy_needs_a_valid_topic_and_server() {
        let missing = parse(&format!("{HOME}[alert_channel]\nkind = \"ntfy\"\n")).unwrap_err();
        assert!(
            missing
                .to_string()
                .contains("alert_channel.topic is missing"),
            "{missing}"
        );

        let bad = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"my topic/x\"\nserver = \"ntfy.sh\"\n"
        ))
        .unwrap_err();
        assert_eq!(
            bad.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - alert_channel.topic: \"my topic/x\" is not an ntfy topic; use up to 64 letters, digits, - or _\n\
             \x20 - alert_channel.server: \"ntfy.sh\" is not a URL; leave it out to use https://ntfy.sh"
        );
    }

    #[test]
    fn unknown_alert_channels_and_keys_are_refused() {
        let kind = parse(&format!("{HOME}[alert_channel]\nkind = \"pigeon\"\n")).unwrap_err();
        assert!(
            kind.to_string()
                .contains("\"pigeon\" is not an Alert channel"),
            "{kind}"
        );

        // A token is a secret, so it has no place in the Profile.
        let token = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar\"\ntoken = \"tk_x\"\n"
        ))
        .unwrap_err();
        assert!(matches!(token, ProfileError::Syntax { .. }), "{token}");
    }

    #[test]
    fn photon_sends_to_a_phone_number_or_a_space() {
        let profile = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"photon\"\nto = \" +32470123456 \"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.alert_channel,
            Some(AlertChannelSetting::Photon(PhotonSetting {
                to: "+32470123456".into(),
            }))
        );
        let profile = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"photon\"\nto = \"any;-;+32470123456\"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.alert_channel,
            Some(AlertChannelSetting::Photon(PhotonSetting {
                to: "any;-;+32470123456".into(),
            }))
        );
    }

    #[test]
    fn photon_needs_a_valid_recipient_and_no_ntfy_keys() {
        let missing = parse(&format!("{HOME}[alert_channel]\nkind = \"photon\"\n")).unwrap_err();
        assert!(
            missing.to_string().contains("alert_channel.to is missing"),
            "{missing}"
        );

        let bad = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"photon\"\nto = \"+32 470 12 34 56\"\ntopic = \"radar\"\n"
        ))
        .unwrap_err();
        assert_eq!(
            bad.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - alert_channel.topic is only for kind = \"ntfy\"; Photon needs just `to`\n\
             \x20 - alert_channel.to: \"+32 470 12 34 56\" is not a phone number in E.164 format, \
             such as \"+32470123456\", or a Photon space ID"
        );

        for to in ["+0470123456", "+12345", "+3247012345678901", "+32abc"] {
            let err = parse(&format!(
                "{HOME}[alert_channel]\nkind = \"photon\"\nto = \"{to}\"\n"
            ))
            .unwrap_err();
            assert!(err.to_string().contains("E.164"), "{to}: {err}");
        }

        let ntfy_with_to = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar\"\nto = \"+32470123456\"\n"
        ))
        .unwrap_err();
        assert!(
            ntfy_with_to
                .to_string()
                .contains("alert_channel.to is only for kind = \"photon\""),
            "{ntfy_with_to}"
        );
    }

    #[test]
    fn photon_credentials_have_no_place_in_the_profile() {
        let err = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"photon\"\nto = \"+32470123456\"\nproject_secret = \"x\"\n"
        ))
        .unwrap_err();
        assert!(matches!(err, ProfileError::Syntax { .. }), "{err}");
    }

    #[test]
    fn the_example_profile_is_valid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/profile.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        Profile::from_toml(&text, &path).unwrap();
        // Its commented-out ntfy block is valid too, once uncommented.
        let uncommented = text
            .replace("# [alert_channel]", "[alert_channel]")
            .replace("# kind = ", "kind = ")
            .replace("# topic = ", "topic = ");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert!(matches!(
            profile.alert_channel,
            Some(AlertChannelSetting::Ntfy(_))
        ));
        // And so is its commented-out schedule block.
        let uncommented = text
            .replace("# [schedule]", "[schedule]")
            .replace("# days = ", "days = ")
            .replace("# time = ", "time = ");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert_eq!(profile.schedule, Schedule::default());
        // And so are its commented-out Codex and command adapters, each in place
        // of Claude Code.
        let without_claude = text.replace("adapter = \"claude-code\"\n", "");
        let uncommented = without_claude
            .replace("# adapter = \"codex\"", "adapter = \"codex\"")
            .replace("# experimental = true", "experimental = true");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert_eq!(profile.agent.adapter, AgentAdapter::Codex);
        assert!(profile.agent.experimental);
        let uncommented = without_claude
            .replace("# adapter = \"command\"", "adapter = \"command\"")
            .replace("# command = ", "command = ");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert_eq!(profile.agent.adapter, AgentAdapter::Command);
        assert_eq!(
            profile.agent.command.map(|c| c.program().to_owned()),
            Some("my-agent".to_owned())
        );
    }

    #[test]
    fn the_schedule_defaults_to_monday_and_thursday_at_eight() {
        let profile = parse(HOME).unwrap();
        assert_eq!(profile.schedule.days, vec![Weekday::Mon, Weekday::Thu]);
        assert_eq!(
            profile.schedule.time,
            NaiveTime::from_hms_opt(8, 0, 0).unwrap()
        );
        assert_eq!(
            profile.schedule.to_string(),
            "every Monday and Thursday at 08:00"
        );
    }

    #[test]
    fn reads_a_custom_schedule() {
        let profile = parse(&format!(
            "{HOME}[schedule]\ndays = [\"sunday\", \" Wed \", \"Friday\", \"wednesday\"]\ntime = \"21:45\"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.schedule,
            Schedule {
                days: vec![Weekday::Wed, Weekday::Fri, Weekday::Sun],
                time: NaiveTime::from_hms_opt(21, 45, 0).unwrap(),
            }
        );
        assert_eq!(
            profile.schedule.to_string(),
            "every Wednesday, Friday and Sunday at 21:45"
        );
    }

    #[test]
    fn a_partial_schedule_keeps_the_other_default() {
        let profile = parse(&format!("{HOME}[schedule]\ntime = \"06:30\"\n")).unwrap();
        assert_eq!(
            profile.schedule.to_string(),
            "every Monday and Thursday at 06:30"
        );
        let profile = parse(&format!("{HOME}[schedule]\ndays = [\"Tue\"]\n")).unwrap();
        assert_eq!(profile.schedule.to_string(), "every Tuesday at 08:00");
    }

    #[test]
    fn a_bad_schedule_is_reported() {
        let err = parse(&format!(
            "{HOME}[schedule]\ndays = [\"Mondey\"]\ntime = \"8am\"\n"
        ))
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - schedule.days: \"Mondey\" is not a day of the week, such as \"Monday\"\n\
             \x20 - schedule.time: \"8am\" is not a 24-hour local time such as \"08:00\""
        );
    }

    #[test]
    fn a_schedule_needs_a_day() {
        let err = parse(&format!("{HOME}[schedule]\ndays = []\n")).unwrap_err();
        assert!(err.to_string().contains("schedule.days is empty"), "{err}");
    }

    #[test]
    fn unknown_schedule_keys_are_refused() {
        let err = parse(&format!("{HOME}[schedule]\nhour = 8\n")).unwrap_err();
        assert!(err.to_string().contains("hour"), "{err}");
    }

    #[test]
    fn reads_the_persons_calendars_and_the_page_cap() {
        let profile = parse(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n\
             [sources]\nics = [\" https://example.org/hacks.ics \"]\nhackathonhub_pages_per_run = 5\n",
        )
        .unwrap();
        assert_eq!(profile.sources.ics, ["https://example.org/hacks.ics"]);
        assert_eq!(profile.sources.hackathonhub_pages_per_run, 5);

        let defaults =
            parse("countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n")
                .unwrap();
        assert_eq!(defaults.sources, SourceSettings::default());
        assert_eq!(defaults.sources.hackathonhub_pages_per_run, 40);
    }

    #[test]
    fn a_calendar_that_isnt_a_link_is_refused() {
        let error = parse(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n\
             [sources]\nics = [\"my calendar\"]\n",
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("sources.ics: \"my calendar\" is not a link"),
            "{error}"
        );
    }
}
