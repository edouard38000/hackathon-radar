//! `hackathon-radar doctor`: checks everything a Run depends on and says
//! plainly what is wrong and how to fix it.
//!
//! Each check reports `pass`, `warn`, `fail` or `skip`, a one-line reason and,
//! unless it passed, a one-line fix. Doctor exits 0 when nothing failed and 1
//! when something did, and `--json` prints the same results for the setup
//! skill (ADR 0007), in the shape [`Report::json`] documents.
//!
//! Checks are cheap: no Agent judges anything, no Alert is sent unless asked
//! with `--send-test-alert`, and the network is only touched for one light
//! request per Source and one to the ntfy server (none with `--offline`).
//! Everything outside the process goes through [`DoctorEnv`], so tests fake
//! the keychain, programs, schedule, network, clock and Run history.

mod agent;
mod alert;
mod last_run;
mod schedule;
mod secrets;
mod sources;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::alert::photon::Sidecar;
use crate::http::{HttpGet, HttpPost};
use crate::process::ProcessRunner;
use crate::profile::{AgentAdapter, AlertChannelSetting, Profile, ProfileError};
use crate::scheduler::{CommandRunner, Platform};
use crate::secrets::Secrets;
use crate::store::RunRecord;

/// The version of the `--json` shape; it changes only when a field is
/// removed or changes meaning.
pub const JSON_VERSION: u32 = 1;

/// What a check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    /// Not checked, such as the Sources with `--offline`.
    Skip,
    /// Worth knowing, but Runs still work.
    Warn,
    /// Runs fail, or Alerts don't reach the person, until it is fixed.
    Fail,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Skip => "skip",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

/// The part of the setup a check belongs to, in the order doctor runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Section {
    Profile,
    Secrets,
    Agent,
    AlertChannel,
    Schedule,
    Sources,
    LastRun,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Profile => "Profile",
            Section::Secrets => "Secrets",
            Section::Agent => "Agent",
            Section::AlertChannel => "Alert channel",
            Section::Schedule => "Schedule",
            Section::Sources => "Sources",
            Section::LastRun => "Last Run",
        }
    }
}

/// One check's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// A stable identifier, such as `agent.sign-in` or `source.devpost`.
    pub id: String,
    pub section: Section,
    /// What was checked, for people: "Claude Code sign-in".
    pub name: String,
    pub status: Status,
    /// What was found, in one line.
    pub reason: String,
    /// What to do about it, in one line; `None` when there is nothing to do.
    pub fix: Option<String>,
    /// More lines when one isn't enough, such as each problem in the Profile.
    pub details: Vec<String>,
}

impl Check {
    fn new(
        section: Section,
        id: impl Into<String>,
        name: impl Into<String>,
        status: Status,
        reason: impl AsRef<str>,
    ) -> Self {
        Self {
            id: id.into(),
            section,
            name: name.into(),
            status,
            reason: one_line(reason.as_ref()),
            fix: None,
            details: Vec::new(),
        }
    }

    pub(crate) fn pass(
        section: Section,
        id: impl Into<String>,
        name: impl Into<String>,
        reason: impl AsRef<str>,
    ) -> Self {
        Self::new(section, id, name, Status::Pass, reason)
    }

    pub(crate) fn skip(
        section: Section,
        id: impl Into<String>,
        name: impl Into<String>,
        reason: impl AsRef<str>,
    ) -> Self {
        Self::new(section, id, name, Status::Skip, reason)
    }

    pub(crate) fn warn(
        section: Section,
        id: impl Into<String>,
        name: impl Into<String>,
        reason: impl AsRef<str>,
        fix: impl AsRef<str>,
    ) -> Self {
        Self::new(section, id, name, Status::Warn, reason).fix(fix)
    }

    pub(crate) fn fail(
        section: Section,
        id: impl Into<String>,
        name: impl Into<String>,
        reason: impl AsRef<str>,
        fix: impl AsRef<str>,
    ) -> Self {
        Self::new(section, id, name, Status::Fail, reason).fix(fix)
    }

    pub(crate) fn fix(mut self, fix: impl AsRef<str>) -> Self {
        self.fix = Some(one_line(fix.as_ref()));
        self
    }

    pub(crate) fn details(mut self, details: impl IntoIterator<Item = String>) -> Self {
        self.details = details.into_iter().map(|d| one_line(&d)).collect();
        self
    }
}

/// Text on one line: line breaks and runs of spaces become single spaces.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every check's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub checks: Vec<Check>,
}

#[derive(Serialize)]
struct Counts {
    pass: usize,
    warn: usize,
    fail: usize,
    skip: usize,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    ok: bool,
    counts: Counts,
    checks: &'a [Check],
}

impl Report {
    /// True when nothing failed; warnings don't count against it.
    pub fn ok(&self) -> bool {
        self.count(Status::Fail) == 0
    }

    /// 0 when nothing failed, 1 when something did.
    pub fn exit_code(&self) -> u8 {
        if self.ok() { 0 } else { 1 }
    }

    pub fn count(&self, status: Status) -> usize {
        self.checks.iter().filter(|c| c.status == status).count()
    }

    /// The results as JSON, for the setup skill:
    ///
    /// ```json
    /// {
    ///   "version": 1,
    ///   "ok": true,
    ///   "counts": { "pass": 12, "warn": 1, "fail": 0, "skip": 0 },
    ///   "checks": [
    ///     {
    ///       "id": "agent.sign-in",
    ///       "section": "agent",
    ///       "name": "Claude Code sign-in",
    ///       "status": "pass",
    ///       "reason": "Claude Code is signed in (claude.ai).",
    ///       "fix": null,
    ///       "details": []
    ///     }
    ///   ]
    /// }
    /// ```
    ///
    /// `ok` is false exactly when a check has `"status": "fail"`, which is
    /// also when doctor exits non-zero.
    pub fn json(&self) -> String {
        let report = JsonReport {
            version: JSON_VERSION,
            ok: self.ok(),
            counts: Counts {
                pass: self.count(Status::Pass),
                warn: self.count(Status::Warn),
                fail: self.count(Status::Fail),
                skip: self.count(Status::Skip),
            },
            checks: &self.checks,
        };
        let mut text = serde_json::to_string_pretty(&report).expect("the report serialises");
        text.push('\n');
        text
    }

    /// The results for people, section by section.
    pub fn human(&self) -> String {
        let mut out = String::new();
        let mut section = None;
        for check in &self.checks {
            if section != Some(check.section) {
                if section.is_some() {
                    out.push('\n');
                }
                let _ = writeln!(out, "{}", check.section.title());
                section = Some(check.section);
            }
            let _ = writeln!(
                out,
                "  {}  {}: {}",
                check.status.as_str(),
                check.name,
                check.reason
            );
            for detail in &check.details {
                let _ = writeln!(out, "          - {detail}");
            }
            if let Some(fix) = &check.fix {
                let _ = writeln!(out, "        fix: {fix}");
            }
        }
        let _ = write!(
            out,
            "\n{} passed, {} warnings, {} failed",
            self.count(Status::Pass),
            self.count(Status::Warn),
            self.count(Status::Fail)
        );
        if self.count(Status::Skip) > 0 {
            let _ = write!(out, ", {} skipped", self.count(Status::Skip));
        }
        out.push_str(".\n");
        out.push_str(if self.ok() {
            "Nothing is broken: Runs have what they need.\n"
        } else {
            "Fix what failed, then run `hackathon-radar doctor` again.\n"
        });
        out
    }
}

/// What doctor checks, from the command line.
#[derive(Debug, Clone)]
pub struct DoctorOptions {
    pub profile: PathBuf,
    pub db: PathBuf,
    /// Where scheduled Runs log, for fixes that point there.
    pub log: PathBuf,
    /// Skip every check that uses the network.
    pub offline: bool,
    /// Send one test Alert through the Alert channel.
    pub send_test_alert: bool,
}

/// Finds a program: by name in the directories of a `PATH` value, or, when it
/// holds a `/`, at that path.
pub type FindProgram<'a> = &'a dyn Fn(&str, &str) -> Option<PathBuf>;

/// Everything doctor touches outside the Profile, so tests can fake it.
pub struct DoctorEnv<'a> {
    pub secrets: &'a Secrets,
    /// The shell's environment: `PATH`, `XDG_CONFIG_HOME`, `USER`.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub find_program: FindProgram<'a>,
    /// Runs `claude auth status`, `node --version` and, for a test Alert, the
    /// Photon sidecar.
    pub runner: &'a dyn ProcessRunner,
    /// Runs `launchctl`, `systemctl` and `loginctl`.
    pub scheduler: &'a mut dyn CommandRunner,
    /// Where the schedule is installed, or why scheduling isn't available.
    pub platform: Result<Platform, String>,
    /// This binary, which the schedule should start.
    pub binary: PathBuf,
    /// Plain requests, to the ntfy server.
    pub http: &'a dyn HttpGet,
    /// Polite requests to the Sources (robots.txt and pacing).
    pub sources_http: &'a dyn HttpGet,
    /// Requests for the person's own calendars, paced but without robots.txt.
    pub calendars_http: &'a dyn HttpGet,
    /// Sends the ntfy test Alert.
    pub http_post: &'a dyn HttpPost,
    pub sidecar: Sidecar,
    pub now: DateTime<Utc>,
    /// The latest Run in the database's history, if there is one.
    pub last_run: &'a dyn Fn() -> anyhow::Result<Option<RunRecord>>,
}

/// Runs every check.
pub fn doctor(options: &DoctorOptions, env: DoctorEnv<'_>) -> Report {
    let mut checks = Vec::new();
    let profile = check_profile(&options.profile, &mut checks);
    let profile = profile.as_ref();
    let shell_path = (env.env)("PATH").unwrap_or_default();

    secrets::check(profile, env.secrets, env.env, &mut checks);

    match profile {
        Some(profile) => agent::check(
            profile,
            &agent::Context {
                shell_path: &shell_path,
                find_program: env.find_program,
                runner: env.runner,
            },
            &mut checks,
        ),
        None => checks.push(needs_profile(Section::Agent, "agent")),
    }

    match profile {
        Some(profile) => alert::check(
            profile,
            &alert::Context {
                shell_path: &shell_path,
                find_program: env.find_program,
                runner: env.runner,
                http: env.http,
                http_post: env.http_post,
                secrets: env.secrets,
                sidecar: &env.sidecar,
                offline: options.offline,
                send_test_alert: options.send_test_alert,
            },
            &mut checks,
        ),
        None => checks.push(needs_profile(Section::AlertChannel, "alert")),
    }

    let installed = schedule::check(
        schedule::Context {
            platform: &env.platform,
            runner: env.scheduler,
            binary: &env.binary,
            profile_path: &options.profile,
            db_path: &options.db,
            programs: profile.map(programs_runs_need).unwrap_or_default(),
            find_program: env.find_program,
            env: env.env,
        },
        &mut checks,
    );

    match profile {
        Some(_) if options.offline => checks.push(Check::skip(
            Section::Sources,
            "sources",
            "Sources",
            "Skipped with --offline.",
        )),
        Some(profile) => sources::check(
            profile,
            env.sources_http,
            env.calendars_http,
            env.now.date_naive(),
            &mut checks,
        ),
        None => checks.push(needs_profile(Section::Sources, "sources")),
    }

    last_run::check(
        (env.last_run)(),
        &last_run::Context {
            now: env.now,
            schedule: profile.map(|p| p.schedule.clone()).unwrap_or_default(),
            installed,
            db: &options.db,
            log: &options.log,
        },
        &mut checks,
    );

    Report { checks }
}

/// The skip a section gets when the Profile didn't load.
fn needs_profile(section: Section, id: &str) -> Check {
    Check::skip(
        section,
        id,
        section.title(),
        "Not checked, because the Profile didn't load.",
    )
}

/// Loads the Profile and reports whether it is there, parses and is valid.
fn check_profile(path: &Path, checks: &mut Vec<Check>) -> Option<Profile> {
    let s = Section::Profile;
    let shown = path.display();
    let check = match Profile::load(path) {
        Ok(profile) => {
            checks.push(Check::pass(
                s,
                "profile",
                "Profile",
                format!("{shown} is valid."),
            ));
            return Some(profile);
        }
        Err(ProfileError::NotFound { .. }) => Check::fail(
            s,
            "profile",
            "Profile",
            format!("There is no Profile at {shown}."),
            "Run `hackathon-radar init` to create it, or pass --profile <PATH>.",
        ),
        Err(ProfileError::Read { source, .. }) => Check::fail(
            s,
            "profile",
            "Profile",
            format!("Could not read {shown}: {source}."),
            "Make the file readable by you, or pass --profile <PATH>.",
        ),
        Err(ProfileError::Syntax { message, .. }) => Check::fail(
            s,
            "profile",
            "Profile",
            format!("{shown} is not valid TOML."),
            "Fix the line named below, or run `hackathon-radar init` to rewrite the values.",
        )
        .details(
            message
                .lines()
                .map(str::to_owned)
                .filter(|l| !l.trim().is_empty()),
        ),
        Err(ProfileError::Invalid { problems, .. }) => Check::fail(
            s,
            "profile",
            "Profile",
            format!(
                "{shown} has {} {}.",
                problems.len(),
                if problems.len() == 1 {
                    "problem"
                } else {
                    "problems"
                }
            ),
            "Fix each problem listed, or run `hackathon-radar init` to set those values.",
        )
        .details(problems),
    };
    checks.push(check);
    None
}

/// The program the Profile's Agent runs: `claude`, `codex`, or the command
/// template's program.
pub(crate) fn agent_program(profile: &Profile) -> String {
    match profile.agent.adapter {
        AgentAdapter::ClaudeCode => crate::agent::claude_code::PROGRAM.to_owned(),
        AgentAdapter::Codex => crate::agent::codex::PROGRAM.to_owned(),
        AgentAdapter::Command => profile
            .agent
            .command
            .as_ref()
            .map_or_else(String::new, |c| c.program().to_owned()),
    }
}

/// The programs a Run starts: the Agent's, and `node` for Photon Alerts.
fn programs_runs_need(profile: &Profile) -> Vec<String> {
    let mut programs = vec![agent_program(profile)];
    if matches!(profile.alert_channel, Some(AlertChannelSetting::Photon(_))) {
        programs.push(crate::alert::photon::NODE.to_owned());
    }
    programs.retain(|p| !p.is_empty());
    programs
}

/// The real [`FindProgram`]: `program` in the directories of `path`, or, when
/// it holds a `/`, that file if it is executable.
pub fn which(program: &str, path: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let candidate = PathBuf::from(program);
        return crate::init::is_executable(&candidate).then_some(candidate);
    }
    crate::init::find_on_path(program, Some(std::ffi::OsStr::new(path)))
}

/// An error and its causes on one line, with `hide` (such as a private URL)
/// taken out.
pub(crate) fn error_line(error: &anyhow::Error, hide: Option<&str>) -> String {
    let text = format!("{error:#}");
    let text = match hide {
        Some(hide) if !hide.is_empty() => text.replace(hide, "[hidden]"),
        _ => text,
    };
    one_line(&text)
}

/// Why fetching `url` failed, on one line, without repeating the URL; with
/// `private`, the URL never shows.
pub(crate) fn fetch_error(error: &anyhow::Error, url: &str, private: bool) -> String {
    let text = format!("{error:#}").replace(&format!("fetching {url}: "), "");
    let text = if private {
        text.replace(url, "[hidden]")
    } else {
        text
    };
    one_line(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_order_from_best_to_worst() {
        assert!(Status::Pass < Status::Skip);
        assert!(Status::Skip < Status::Warn);
        assert!(Status::Warn < Status::Fail);
    }

    #[test]
    fn text_is_kept_on_one_line() {
        assert_eq!(one_line("a\n  b\tc "), "a b c");
    }
}
