//! `hackathon-radar doctor`, with fakes for everything it touches: a keychain
//! that can be locked or need approval, a fake PATH, a process runner that
//! answers `claude auth status`, `node --version` and the Photon sidecar (and
//! panics on anything else, so no Agent is ever asked to judge), a fake
//! `launchctl`/`systemctl`/`loginctl`, a fake network, a fixed clock and a
//! fixed Run history. Schedule and sidecar files are real files in a temp
//! dir; nothing touches the real keychain, network, launchd, systemd or npm.

mod support;

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, TimeZone, Utc};
use tempfile::TempDir;

use hackathon_radar::alert::photon::Sidecar;
use hackathon_radar::doctor::{self, DoctorEnv, DoctorOptions, Report, Section, Status};
use hackathon_radar::http::HttpGet;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::Schedule;
use hackathon_radar::scheduler::{
    CommandOutput, CommandRunner, Platform, ScheduledRun, launchd, systemd,
};
use hackathon_radar::secrets::{Keychain, Probe, Secrets, Unavailable, keychain_name};
use hackathon_radar::sources::{devpost, luma};
use hackathon_radar::store::{RunCounts, RunRecord};
use support::*;

const PHOTON_ID: &str = "proj_doctor_test_id";
const PHOTON_SECRET: &str = "PHOTON-SECRET-VALUE-1234567890";
const SERPAPI_KEY: &str = "SERPAPI-SECRET-VALUE-0987654321";
const NTFY_TOKEN: &str = "tk_NTFY-SECRET-VALUE-5555";
const NTFY_TOPIC: &str = "radar-doctor-secret-topic-77";
const PRIVATE_CALENDAR: &str =
    "https://calendar.example.org/private-0f9a8b7c6d5e/basic.ics?token=CAL-SECRET-42";
const SHELL_PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap()
}

const PHOTON_PROFILE: &str = "countries = [\"Belgium\"]

[home]
city = \"Brussels\"
country = \"Belgium\"
airports = [\"BRU\"]

[travel]
flight_budget = 200

[alert_channel]
kind = \"photon\"
to = \"+32470123456\"
";

fn ntfy_profile() -> String {
    format!(
        "countries = [\"Belgium\"]

[home]
city = \"Brussels\"
country = \"Belgium\"
airports = [\"BRU\"]

[alert_channel]
kind = \"ntfy\"
topic = \"{NTFY_TOPIC}\"
"
    )
}

/// A profile with no Alert channel and no Flight budget, so it needs no secrets.
const PLAIN_PROFILE: &str = "countries = [\"Belgium\"]

[home]
city = \"Brussels\"
country = \"Belgium\"
airports = [\"BRU\"]
";

/// The keychain: items as in [`FakeKeychain`], some of which need approval in
/// a prompt (as after a rebuild on macOS), or a locked one.
#[derive(Clone, Default)]
struct TestKeychain {
    items: FakeKeychain,
    needs_approval: Vec<&'static str>,
    locked: bool,
}

impl Keychain for TestKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, Unavailable> {
        if self.locked {
            return Err(Unavailable("the collection is locked".into()));
        }
        if self.needs_approval.contains(&account) {
            return Err(Unavailable("User interaction is not allowed.".into()));
        }
        Keychain::get(&self.items, account)
    }

    fn set(&self, _: &str, _: &str) -> Result<(), Unavailable> {
        panic!("doctor never stores a secret")
    }

    fn probe(&self, account: &str) -> Probe {
        if self.locked {
            return Probe::Unavailable("the collection is locked".into());
        }
        if self.needs_approval.contains(&account) {
            return Probe::NeedsApproval("User interaction is not allowed.".into());
        }
        match self.items.get(account) {
            Some(_) => Probe::Found,
            None => Probe::Missing,
        }
    }
}

/// Answers the few programs doctor may run, and panics on any other: no
/// Agent is ever asked to judge.
struct Machine {
    /// `claude auth status`: its exit code and output, or `None` when it can't start.
    auth_status: Option<(i32, String)>,
    /// `node --version`'s output.
    node_version: String,
    commands: RefCell<Vec<Command>>,
}

impl ProcessRunner for Machine {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
        let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
        let output = |code: i32, stdout: &str| {
            Ok(Output {
                code: Some(code),
                stdout: stdout.to_owned(),
                stderr: String::new(),
            })
        };
        match args.as_slice() {
            ["auth", "status"] if command.program.ends_with("claude") => match &self.auth_status {
                Some((code, stdout)) => output(*code, stdout),
                None => anyhow::bail!("could not start {}", command.program),
            },
            ["--version"] if command.program.ends_with("node") => output(0, &self.node_version),
            [script] if command.program == "node" && script.ends_with("send.mjs") => {
                output(0, &fixture("photon/sent.json"))
            }
            _ => panic!("doctor ran an unexpected program: {command:?}"),
        }
    }
}

/// `launchctl`, `systemctl` and `loginctl`.
struct System {
    /// Whether launchd has the agent loaded, or systemd the timer active.
    active: bool,
    /// What `loginctl show-user --property=Linger --value` prints.
    linger: &'static str,
    calls: Vec<String>,
}

impl CommandRunner for System {
    fn run(&mut self, program: &str, args: &[String]) -> io::Result<CommandOutput> {
        self.calls.push(format!("{program} {}", args.join(" ")));
        let answer = |success: bool, stdout: &str| CommandOutput {
            success,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        };
        Ok(match (program, args.first().map(String::as_str)) {
            ("launchctl", Some("print")) => answer(self.active, ""),
            ("systemctl", Some("--user")) if args.get(1).is_some_and(|a| a == "is-active") => {
                answer(self.active, "")
            }
            ("loginctl", Some("show-user")) => answer(true, &format!("{}\n", self.linger)),
            _ => panic!("doctor ran an unexpected command: {program} {args:?}"),
        })
    }
}

/// The network: every URL answers, except those containing a `down` entry.
#[derive(Default)]
struct Network {
    down: Vec<String>,
    requested: RefCell<Vec<String>>,
}

impl HttpGet for Network {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        self.requested.borrow_mut().push(url.to_owned());
        if self.down.iter().any(|d| url.contains(d.as_str())) {
            anyhow::bail!("fetching {url}: Connection refused");
        }
        Ok("ok".into())
    }
}

/// The machine doctor runs on.
struct World {
    dir: TempDir,
    profile: Option<String>,
    keychain: TestKeychain,
    /// Environment variables the secret store sees.
    secret_env: Vec<(&'static str, String)>,
    /// The shell's environment.
    shell_env: Vec<(&'static str, String)>,
    /// Programs that exist, by full path.
    programs: Vec<PathBuf>,
    machine: Machine,
    system: System,
    platform: Result<Platform, String>,
    network: Network,
    post: RecordingPost,
    sidecar: Sidecar,
    last_run: Option<RunRecord>,
    offline: bool,
    send_test_alert: bool,
}

impl World {
    /// Everything in order: a Photon Profile with a Flight budget, its secrets
    /// in the keychain, Claude Code signed in, Node 22, the sidecar installed,
    /// a schedule for this binary, every Source up, and a Run yesterday.
    fn healthy() -> Self {
        let dir = TempDir::new().unwrap();
        let sidecar = Sidecar {
            dir: dir.path().join("sidecar/photon"),
        };
        std::fs::create_dir_all(sidecar.dir.join("node_modules")).unwrap();
        std::fs::write(sidecar.script(), "// the sidecar\n").unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin/hackathon-radar"), "binary").unwrap();

        let keychain = TestKeychain {
            items: FakeKeychain::default()
                .with("photon-project-id", PHOTON_ID)
                .with("photon-project-secret", PHOTON_SECRET)
                .with("serpapi-api-key", SERPAPI_KEY),
            ..TestKeychain::default()
        };
        let world = Self {
            profile: Some(PHOTON_PROFILE.to_owned()),
            keychain,
            secret_env: Vec::new(),
            shell_env: vec![("PATH", SHELL_PATH.to_owned()), ("USER", "ada".to_owned())],
            programs: vec![
                "/usr/local/bin/claude".into(),
                "/opt/homebrew/bin/node".into(),
            ],
            machine: Machine {
                auth_status: Some((0, fixture("claude-code/auth-status-signed-in.json"))),
                node_version: "v22.11.0\n".into(),
                commands: RefCell::new(Vec::new()),
            },
            system: System {
                active: true,
                linger: "yes",
                calls: Vec::new(),
            },
            platform: Ok(Platform::Launchd {
                agents_dir: dir.path().join("LaunchAgents"),
                uid: 501,
            }),
            network: Network::default(),
            post: RecordingPost::ok(),
            sidecar,
            last_run: Some(run_record(now() - Duration::days(1), None)),
            offline: false,
            send_test_alert: false,
            dir,
        };
        world.install_schedule(world.job());
        world
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn profile_path(&self) -> PathBuf {
        self.path("config/profile.toml")
    }

    fn db_path(&self) -> PathBuf {
        self.path("data/hackathon-radar.sqlite")
    }

    fn binary(&self) -> PathBuf {
        self.path("bin/hackathon-radar")
    }

    fn secrets_file(&self) -> PathBuf {
        self.path("config/hackathon-radar/secrets.toml")
    }

    /// The scheduled Run `schedule install` would write from this shell.
    fn job(&self) -> ScheduledRun {
        ScheduledRun {
            binary: self.binary(),
            profile: self.profile_path(),
            db: self.db_path(),
            log: self.path("state/run.log"),
            schedule: Schedule::default(),
            path: SHELL_PATH.into(),
        }
    }

    /// Writes the scheduler's files for `job`, as `schedule install` does.
    fn install_schedule(&self, job: ScheduledRun) {
        match self.platform.as_ref().unwrap() {
            Platform::Launchd { agents_dir, .. } => {
                std::fs::create_dir_all(agents_dir).unwrap();
                std::fs::write(
                    launchd::plist_path(agents_dir),
                    launchd::render_plist(&job).unwrap(),
                )
                .unwrap();
            }
            Platform::Systemd { unit_dir } => {
                std::fs::create_dir_all(unit_dir).unwrap();
                std::fs::write(
                    unit_dir.join(systemd::SERVICE),
                    systemd::render_service(&job).unwrap(),
                )
                .unwrap();
                std::fs::write(
                    unit_dir.join(systemd::TIMER),
                    systemd::render_timer(&job.schedule),
                )
                .unwrap();
            }
        }
    }

    fn uninstall_schedule(&mut self) {
        let _ = std::fs::remove_dir_all(self.path("LaunchAgents"));
        let _ = std::fs::remove_dir_all(self.path("systemd"));
        self.system.active = false;
    }

    fn use_systemd(&mut self) {
        let _ = std::fs::remove_dir_all(self.path("LaunchAgents"));
        self.platform = Ok(Platform::Systemd {
            unit_dir: self.path("systemd"),
        });
        self.install_schedule(self.job());
    }

    /// Writes the owner-only secrets file with these entries and mode.
    fn secrets_file_with(&self, entries: &[(&str, &str)], mode: u32) {
        let path = self.secrets_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: String = entries
            .iter()
            .map(|(k, v)| format!("{k} = \"{v}\"\n"))
            .collect();
        std::fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
    }

    fn run(&mut self) -> Report {
        if let Some(text) = &self.profile {
            std::fs::create_dir_all(self.profile_path().parent().unwrap()).unwrap();
            std::fs::write(self.profile_path(), text).unwrap();
        } else {
            let _ = std::fs::remove_file(self.profile_path());
        }
        let options = DoctorOptions {
            profile: self.profile_path(),
            db: self.db_path(),
            log: self.path("state/run.log"),
            offline: self.offline,
            send_test_alert: self.send_test_alert,
        };
        let secret_env = self.secret_env.clone();
        let secrets = Secrets::new(
            Box::new(self.keychain.clone()),
            self.secrets_file(),
            Box::new(move |name| {
                secret_env
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.clone())
            }),
        );
        let shell_env = self.shell_env.clone();
        let env = move |name: &str| {
            shell_env
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.clone())
        };
        let programs = self.programs.clone();
        let find_program = move |program: &str, path: &str| -> Option<PathBuf> {
            if program.contains('/') {
                return programs
                    .iter()
                    .find(|p| p.as_path() == Path::new(program))
                    .cloned();
            }
            path.split(':')
                .map(|dir| Path::new(dir).join(program))
                .find(|candidate| programs.contains(candidate))
        };
        let last = self.last_run.clone();
        let last_run = move || Ok(last.clone());
        let binary = self.binary();
        doctor::doctor(
            &options,
            DoctorEnv {
                secrets: &secrets,
                env: &env,
                find_program: &find_program,
                runner: &self.machine,
                scheduler: &mut self.system,
                platform: self.platform.clone(),
                binary,
                http: &self.network,
                sources_http: &self.network,
                calendars_http: &self.network,
                http_post: &self.post,
                sidecar: self.sidecar.clone(),
                now: now(),
                last_run: &last_run,
            },
        )
    }

    /// Output with this world's temp dir and the platform's keychain name
    /// replaced, so it reads the same on every machine.
    fn normalise(&self, text: &str) -> String {
        text.replace(&self.dir.path().display().to_string(), "$TMP")
            .replace(keychain_name(), "$KEYCHAIN")
    }
}

fn run_record(finished_at: DateTime<Utc>, failure: Option<&str>) -> RunRecord {
    RunRecord {
        day: finished_at.date_naive(),
        started_at: finished_at - Duration::minutes(4),
        finished_at,
        failure: failure.map(str::to_owned),
        counts: RunCounts::default(),
        attention_sent: false,
    }
}

fn status_of(report: &Report, id: &str) -> Status {
    find(report, id).status
}

fn find<'r>(report: &'r Report, id: &str) -> &'r doctor::Check {
    report
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no check {id} in:\n{}", report.human()))
}

fn has(report: &Report, id: &str) -> bool {
    report.checks.iter().any(|c| c.id == id)
}

// The whole report.

#[test]
fn a_healthy_setup_passes_and_matches_the_golden_json() {
    let mut world = World::healthy();
    let report = world.run();

    let json = world.normalise(&report.json());
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/doctor/healthy.json");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden, &json).unwrap();
    }
    assert_eq!(json, std::fs::read_to_string(&golden).unwrap());
    assert!(report.ok(), "{}", report.human());
    assert_eq!(report.exit_code(), 0);
    assert_eq!(report.count(Status::Fail), 0);
    assert_eq!(report.count(Status::Warn), 0, "{}", report.human());
}

#[test]
fn the_json_has_a_stable_shape() {
    let mut world = World::healthy();
    world.profile = None;
    let report = world.run();
    let value: serde_json::Value = serde_json::from_str(&report.json()).unwrap();

    assert_eq!(value["version"], 1);
    assert_eq!(value["ok"], false);
    for key in ["pass", "warn", "fail", "skip"] {
        assert!(value["counts"][key].is_u64(), "counts.{key}");
    }
    let checks = value["checks"].as_array().unwrap();
    assert!(!checks.is_empty());
    for check in checks {
        let keys: Vec<&str> = check
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "details", "fix", "id", "name", "reason", "section", "status"
            ]
        );
        assert!(
            ["pass", "warn", "fail", "skip"].contains(&check["status"].as_str().unwrap()),
            "{check}"
        );
        assert!(!check["reason"].as_str().unwrap().contains('\n'));
    }
    assert_eq!(checks[0]["section"], "profile");
    assert_eq!(checks[0]["status"], "fail");
}

#[test]
fn the_human_output_gives_each_check_a_status_a_reason_and_a_fix() {
    let mut world = World::healthy();
    world.machine.auth_status = Some((1, fixture("claude-code/auth-status-signed-out.json")));
    let report = world.run();
    let text = world.normalise(&report.human());

    assert!(text.starts_with("Profile\n  pass  Profile: $TMP/config/profile.toml is valid.\n"));
    assert!(text.contains(
        "\nAgent\n  pass  Agent program: Claude Code runs claude, found at /usr/local/bin/claude.\n  \
         fail  Claude Code sign-in: Claude Code isn't signed in, so Runs can't judge anything.\n        \
         fix: Run `claude` once and sign in"
    ));
    assert!(text.contains("\nLast Run\n"));
    assert!(text.ends_with("Fix what failed, then run `hackathon-radar doctor` again.\n"));
    for check in &report.checks {
        if matches!(check.status, Status::Warn | Status::Fail) {
            assert!(check.fix.is_some(), "{} has no fix", check.id);
        }
    }
}

#[test]
fn the_exit_code_is_zero_unless_a_check_fails() {
    let mut world = World::healthy();
    assert_eq!(world.run().exit_code(), 0);

    // Warnings only: a Source down and no schedule.
    world.network.down = vec!["devpost.com".into()];
    world.uninstall_schedule();
    let report = world.run();
    assert!(report.count(Status::Warn) >= 2);
    assert_eq!(report.count(Status::Fail), 0);
    assert_eq!(report.exit_code(), 0);

    // One failure.
    world.sidecar.dir = world.path("nowhere");
    let report = world.run();
    assert_eq!(report.exit_code(), 1);
    assert!(!report.ok());
}

#[test]
fn no_secret_value_ever_appears_in_the_output() {
    let secrets = [
        PHOTON_ID,
        PHOTON_SECRET,
        SERPAPI_KEY,
        NTFY_TOKEN,
        NTFY_TOPIC,
        "CAL-SECRET-42",
        "private-0f9a8b7c6d5e",
        "ada.lovelace@example.com",
        "org-0123456789",
    ];
    let check = |report: &Report| {
        for text in [report.json(), report.human()] {
            for secret in secrets {
                assert!(!text.contains(secret), "{secret} leaked into:\n{text}");
            }
        }
    };

    // Photon, with the secrets in the keychain, the environment and the file,
    // a private calendar that is down, and a test Alert.
    let mut world = World::healthy();
    world.secret_env = vec![("PHOTON_PROJECT_SECRET", PHOTON_SECRET.into())];
    world.secrets_file_with(&[("serpapi-api-key", SERPAPI_KEY)], 0o600);
    world.profile = Some(format!(
        "{PHOTON_PROFILE}\n[sources]\nics = [\"{PRIVATE_CALENDAR}\"]\n"
    ));
    world.network.down = vec!["calendar.example.org".into()];
    world.send_test_alert = true;
    check(&world.run());

    // The same with the keychain needing approval and the file too open.
    world.keychain.needs_approval = vec!["photon-project-secret"];
    world.secrets_file_with(&[("photon-project-secret", PHOTON_SECRET)], 0o644);
    check(&world.run());

    // ntfy, with a token, an unreachable server and a failing test Alert.
    let mut world = World::healthy();
    world.profile = Some(ntfy_profile());
    world.keychain.items = FakeKeychain::default().with("ntfy-token", NTFY_TOKEN);
    world.network.down = vec!["ntfy.sh".into()];
    world.post.reply = Err(format!("POST https://ntfy.sh/{NTFY_TOPIC} failed"));
    world.send_test_alert = true;
    let report = world.run();
    assert_eq!(status_of(&report, "alert.test"), Status::Fail);
    check(&report);
}

#[test]
fn checks_are_cheap_no_agent_judges_and_no_alert_goes_out_unasked() {
    let mut world = World::healthy();
    world.run();

    let commands: Vec<String> = world
        .machine
        .commands
        .borrow()
        .iter()
        .map(|c| format!("{} {}", c.program, c.args.join(" ")))
        .collect();
    assert_eq!(
        commands,
        [
            "/usr/local/bin/claude auth status",
            "/opt/homebrew/bin/node --version"
        ]
    );
    assert!(world.post.requests.borrow().is_empty());
    // One request per Source.
    let requested = world.network.requested.borrow().clone();
    assert!(requested.contains(&devpost::API_URL.to_owned()));
    for url in luma::EUROPE_CALENDARS {
        assert!(requested.contains(&url.to_string()));
    }
    let mut unique = requested.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), requested.len());
}

// Profile.

#[test]
fn a_missing_profile_fails_and_skips_what_needs_it() {
    let mut world = World::healthy();
    world.profile = None;
    let report = world.run();

    let profile = find(&report, "profile");
    assert_eq!(profile.status, Status::Fail);
    assert!(
        profile
            .fix
            .as_deref()
            .unwrap()
            .contains("hackathon-radar init")
    );
    for id in ["agent", "alert", "sources"] {
        assert_eq!(status_of(&report, id), Status::Skip, "{id}");
    }
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn an_invalid_profile_lists_every_problem() {
    let mut world = World::healthy();
    world.profile = Some(
        "countries = []\n\n[home]\ncity = \"Brussels\"\nairports = [\"BRUX\"]\n\n\
         [schedule]\ndays = [\"Funday\"]\n"
            .into(),
    );
    let report = world.run();
    let profile = find(&report, "profile");

    assert_eq!(profile.status, Status::Fail);
    assert!(profile.details.len() >= 3, "{:?}", profile.details);
    assert!(
        profile
            .reason
            .contains(&format!("{} problems", profile.details.len()))
    );
    assert!(profile.details.iter().any(|d| d.contains("Funday")));
    assert!(world.normalise(&report.human()).contains("\n          - "));
}

#[test]
fn a_profile_that_is_not_toml_says_where() {
    let mut world = World::healthy();
    world.profile = Some("countries = [\n".into());
    let report = world.run();
    let profile = find(&report, "profile");

    assert_eq!(profile.status, Status::Fail);
    assert!(profile.reason.contains("not valid TOML"));
    assert!(!profile.details.is_empty());
}

#[test]
fn zero_judging_requests_per_run_fails_with_its_fix() {
    let mut world = World::healthy();
    world.profile = Some(format!(
        "{PLAIN_PROFILE}\n[agent]\njudging_requests_per_run = 0\n"
    ));
    let report = world.run();
    let profile = find(&report, "profile");

    assert_eq!(profile.status, Status::Fail);
    assert_eq!(
        profile.details,
        [
            "agent.judging_requests_per_run is 0, so no Candidate would ever be judged; \
             set it to at least 1 (3 by default)"
        ]
    );
    assert!(profile.reason.contains("1 problem"), "{}", profile.reason);
}

// Secrets.

#[test]
fn secrets_say_where_they_come_from() {
    let mut world = World::healthy();
    world.secret_env = vec![("SERPAPI_API_KEY", "from-the-shell".into())];
    let report = world.run();

    let id = find(&report, "secrets.photon-project-id");
    assert_eq!(id.status, Status::Pass);
    assert_eq!(id.reason, format!("In {}.", keychain_name()));
    let serpapi = find(&report, "secrets.serpapi-api-key");
    assert_eq!(serpapi.status, Status::Pass);
    assert!(serpapi.reason.contains("SERPAPI_API_KEY overrides it"));
    assert!(!has(&report, "secrets.keychain"));
    assert!(!has(&report, "secrets.file"));
}

#[test]
fn an_item_that_needs_approval_fails_since_a_scheduled_run_would_miss_it() {
    let mut world = World::healthy();
    world.keychain.needs_approval = vec!["photon-project-secret"];
    let report = world.run();
    let check = find(&report, "secrets.photon-project-secret");

    assert_eq!(check.status, Status::Fail);
    assert!(check.reason.contains("needs your approval in a prompt"));
    assert!(check.reason.contains("scheduled Run"));
    assert!(check.reason.contains("missing"));
    assert!(
        check
            .fix
            .as_deref()
            .unwrap()
            .contains("hackathon-radar init")
    );
    assert_eq!(report.exit_code(), 1);

    // With a copy in the file, Runs fall back to it: a warning.
    world.secrets_file_with(&[("photon-project-secret", PHOTON_SECRET)], 0o600);
    let report = world.run();
    assert_eq!(
        status_of(&report, "secrets.photon-project-secret"),
        Status::Warn
    );
}

#[test]
fn a_locked_keyring_falling_back_to_the_file_is_said_out_loud() {
    let mut world = World::healthy();
    world.keychain.locked = true;
    world.secrets_file_with(
        &[
            ("photon-project-id", PHOTON_ID),
            ("photon-project-secret", PHOTON_SECRET),
            ("serpapi-api-key", SERPAPI_KEY),
        ],
        0o600,
    );
    let report = world.run();

    let keychain = find(&report, "secrets.keychain");
    assert_eq!(keychain.status, Status::Warn);
    assert!(keychain.reason.contains("the collection is locked"));
    assert!(keychain.reason.contains("read from"));
    let file = find(&report, "secrets.file");
    assert_eq!(file.status, Status::Pass);
    assert!(file.reason.contains("holds 3 secrets"));
    assert!(file.reason.contains("mode 0600"));
    assert_eq!(
        status_of(&report, "secrets.photon-project-id"),
        Status::Pass
    );
    assert!(
        find(&report, "secrets.photon-project-id")
            .reason
            .contains("secrets.toml")
    );
    assert!(report.ok());
}

#[cfg(unix)]
#[test]
fn a_secrets_file_wider_than_0600_fails() {
    let mut world = World::healthy();
    world.keychain.locked = true;
    world.secrets_file_with(&[("photon-project-id", PHOTON_ID)], 0o644);
    let report = world.run();

    let file = find(&report, "secrets.file");
    assert_eq!(file.status, Status::Fail);
    assert!(file.reason.contains("mode 0644"));
    assert!(file.fix.as_deref().unwrap().starts_with("Run `chmod 600 "));
    assert_eq!(
        status_of(&report, "secrets.photon-project-id"),
        Status::Fail
    );
}

#[test]
fn missing_secrets_fail_when_required_and_warn_when_optional() {
    let mut world = World::healthy();
    world.keychain.items = FakeKeychain::default();
    let report = world.run();

    assert_eq!(
        status_of(&report, "secrets.photon-project-id"),
        Status::Fail
    );
    assert_eq!(
        status_of(&report, "secrets.photon-project-secret"),
        Status::Fail
    );
    let serpapi = find(&report, "secrets.serpapi-api-key");
    assert_eq!(serpapi.status, Status::Warn);
    assert!(serpapi.reason.contains("unverified"));
}

#[test]
fn a_secret_only_in_the_shell_warns_that_scheduled_runs_miss_it() {
    let mut world = World::healthy();
    world.keychain.items = FakeKeychain::default().with("photon-project-id", PHOTON_ID);
    world.secret_env = vec![
        ("PHOTON_PROJECT_SECRET", PHOTON_SECRET.into()),
        ("SERPAPI_API_KEY", SERPAPI_KEY.into()),
    ];
    let report = world.run();
    let check = find(&report, "secrets.photon-project-secret");

    assert_eq!(check.status, Status::Warn);
    assert!(
        check
            .reason
            .contains("Only in the environment variable PHOTON_PROJECT_SECRET")
    );
}

#[test]
fn an_ntfy_token_is_only_checked_when_set() {
    let mut world = World::healthy();
    world.profile = Some(ntfy_profile());
    assert!(!has(&world.run(), "secrets.ntfy-token"));

    world.keychain.items = FakeKeychain::default().with("ntfy-token", NTFY_TOKEN);
    assert_eq!(status_of(&world.run(), "secrets.ntfy-token"), Status::Pass);
}

#[test]
fn a_custom_xdg_config_home_warns() {
    let mut world = World::healthy();
    world
        .shell_env
        .push(("XDG_CONFIG_HOME", "/home/ada/dotfiles".into()));
    let report = world.run();
    let check = find(&report, "secrets.xdg-config-home");

    assert_eq!(check.status, Status::Warn);
    assert!(check.reason.contains("scheduled Runs don't see it"));
}

// Agent.

#[test]
fn a_missing_claude_fails() {
    let mut world = World::healthy();
    world.programs.retain(|p| !p.ends_with("claude"));
    let report = world.run();

    assert_eq!(status_of(&report, "agent.program"), Status::Fail);
    assert!(!has(&report, "agent.sign-in"));
}

#[test]
fn claude_signed_out_fails() {
    let mut world = World::healthy();
    world.machine.auth_status = Some((1, fixture("claude-code/auth-status-signed-out.json")));
    let report = world.run();
    let check = find(&report, "agent.sign-in");

    assert_eq!(check.status, Status::Fail);
    assert!(check.fix.as_deref().unwrap().contains("sign in"));
}

#[test]
fn claude_signed_in_says_how_without_the_account() {
    let report = World::healthy().run();
    assert_eq!(
        find(&report, "agent.sign-in").reason,
        "Claude Code is signed in (claude.ai)."
    );
}

#[test]
fn an_auth_status_doctor_cannot_read_warns() {
    let mut world = World::healthy();
    world.machine.auth_status = Some((1, "error: unknown command 'auth'\n".into()));
    assert_eq!(status_of(&world.run(), "agent.sign-in"), Status::Warn);

    world.machine.auth_status = None;
    assert_eq!(status_of(&world.run(), "agent.sign-in"), Status::Warn);
}

#[test]
fn codex_prints_the_experimental_warning() {
    let mut world = World::healthy();
    world.profile = Some(format!(
        "{PLAIN_PROFILE}\n[agent]\nadapter = \"codex\"\nexperimental = true\n"
    ));
    world.programs.push("/usr/local/bin/codex".into());
    let report = world.run();
    let check = find(&report, "agent.experimental");

    assert_eq!(check.status, Status::Warn);
    assert!(check.reason.contains("experimental"));
    assert!(check.reason.contains("unverified"));
    assert_eq!(status_of(&report, "agent.program"), Status::Pass);
    assert!(!has(&report, "agent.sign-in"));
}

#[test]
fn a_command_template_needs_its_program() {
    let mut world = World::healthy();
    world.profile = Some(format!(
        "{PLAIN_PROFILE}\n[agent]\nadapter = \"command\"\ncommand = [\"/opt/agents/my-agent\", \"--schema\", \"{{schema_file}}\"]\n"
    ));
    assert_eq!(status_of(&world.run(), "agent.program"), Status::Fail);

    world.programs.push("/opt/agents/my-agent".into());
    assert_eq!(status_of(&world.run(), "agent.program"), Status::Pass);
}

// Alert channel.

#[test]
fn no_alert_channel_warns() {
    let mut world = World::healthy();
    world.profile = Some(PLAIN_PROFILE.into());
    let report = world.run();

    assert_eq!(status_of(&report, "alert.channel"), Status::Warn);
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn ntfy_needs_its_server_to_answer() {
    let mut world = World::healthy();
    world.profile = Some(ntfy_profile());
    let report = world.run();
    assert_eq!(status_of(&report, "alert.ntfy.topic"), Status::Pass);
    assert_eq!(status_of(&report, "alert.ntfy.server"), Status::Pass);
    assert!(
        world
            .network
            .requested
            .borrow()
            .contains(&"https://ntfy.sh/".to_owned())
    );

    world.network.down = vec!["ntfy.sh".into()];
    assert_eq!(status_of(&world.run(), "alert.ntfy.server"), Status::Fail);

    world.offline = true;
    assert_eq!(status_of(&world.run(), "alert.ntfy.server"), Status::Skip);
}

#[test]
fn photon_needs_node() {
    let mut world = World::healthy();
    world.programs.retain(|p| !p.ends_with("node"));
    let report = world.run();
    let check = find(&report, "alert.photon.node");
    assert_eq!(check.status, Status::Fail);
    assert!(check.reason.contains("isn't on your PATH"));
}

#[test]
fn photon_needs_a_new_enough_node() {
    let mut world = World::healthy();
    world.machine.node_version = "v20.18.0\n".into();
    let check = world.run();
    let check = find(&check, "alert.photon.node");
    assert_eq!(check.status, Status::Fail);
    assert!(check.reason.contains("older than 20.18.1"));

    world.machine.node_version = "v20.18.1\n".into();
    assert_eq!(status_of(&world.run(), "alert.photon.node"), Status::Pass);
}

#[test]
fn photon_needs_the_sidecar_and_its_node_modules() {
    let mut world = World::healthy();
    std::fs::remove_dir_all(world.sidecar.dir.join("node_modules")).unwrap();
    let report = world.run();
    let check = find(&report, "alert.photon.dependencies");
    assert_eq!(check.status, Status::Fail);
    assert!(
        check
            .fix
            .as_deref()
            .unwrap()
            .starts_with("Run `npm ci` in ")
    );

    world.sidecar.dir = world.path("elsewhere");
    let report = world.run();
    assert_eq!(status_of(&report, "alert.photon.sidecar"), Status::Fail);
    assert!(!has(&report, "alert.photon.dependencies"));
}

#[test]
fn photon_reminds_the_person_to_text_their_line() {
    let report = World::healthy().run();
    let check = find(&report, "alert.photon.line");
    assert_eq!(check.status, Status::Pass);
    assert!(
        check
            .fix
            .as_deref()
            .unwrap()
            .contains("Text your Photon line")
    );
}

#[test]
fn a_test_alert_goes_through_the_test_alert_path_only_when_asked() {
    let mut world = World::healthy();
    world.profile = Some(ntfy_profile());
    world.run();
    assert!(world.post.requests.borrow().is_empty());

    world.send_test_alert = true;
    let report = world.run();
    assert_eq!(status_of(&report, "alert.test"), Status::Pass);
    let requests = world.post.requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, format!("https://ntfy.sh/{NTFY_TOPIC}"));
    assert_eq!(requests[0].body, hackathon_radar::wiring::TEST_ALERT);
}

#[test]
fn a_photon_test_alert_runs_the_sidecar() {
    let mut world = World::healthy();
    world.send_test_alert = true;
    let report = world.run();

    assert_eq!(status_of(&report, "alert.test"), Status::Pass);
    let commands = world.machine.commands.borrow();
    let sidecar = commands.iter().find(|c| c.program == "node").unwrap();
    assert!(sidecar.args[0].ends_with("send.mjs"));
}

// Schedule.

#[test]
fn no_schedule_warns_and_skips_the_age_check() {
    let mut world = World::healthy();
    world.uninstall_schedule();
    world.last_run = Some(run_record(now() - Duration::days(30), None));
    let report = world.run();

    assert_eq!(status_of(&report, "schedule.installed"), Status::Warn);
    assert!(!has(&report, "schedule.binary"));
    assert!(!has(&report, "last-run.age"));
}

#[test]
fn an_installed_schedule_that_is_not_loaded_fails() {
    let mut world = World::healthy();
    world.system.active = false;
    assert_eq!(status_of(&world.run(), "schedule.installed"), Status::Fail);
}

#[test]
fn a_schedule_pointing_to_an_old_binary_fails() {
    let mut world = World::healthy();
    let mut job = world.job();
    job.binary = world.path("old/target/release/hackathon-radar");
    world.install_schedule(job);
    let report = world.run();
    let check = find(&report, "schedule.binary");

    assert_eq!(check.status, Status::Fail);
    assert!(check.reason.contains("no longer exists"));
    assert!(check.fix.as_deref().unwrap().contains("schedule install"));
}

#[test]
fn a_schedule_for_another_binary_or_profile_warns() {
    let mut world = World::healthy();
    std::fs::create_dir_all(world.path("other")).unwrap();
    std::fs::write(world.path("other/hackathon-radar"), "another binary").unwrap();
    let mut job = world.job();
    job.binary = world.path("other/hackathon-radar");
    job.profile = world.path("config/other-profile.toml");
    world.install_schedule(job);
    let report = world.run();

    assert_eq!(status_of(&report, "schedule.binary"), Status::Warn);
    assert_eq!(status_of(&report, "schedule.profile"), Status::Warn);
    assert_eq!(status_of(&report, "schedule.db"), Status::Pass);
}

#[test]
fn a_schedule_path_missing_the_agent_fails() {
    let mut world = World::healthy();
    let mut job = world.job();
    job.path = "/opt/homebrew/bin:/usr/bin:/bin".into();
    world.install_schedule(job);
    let report = world.run();
    let check = find(&report, "schedule.path");

    assert_eq!(check.status, Status::Fail);
    assert!(check.reason.contains("doesn't find claude"));
    assert!(!check.reason.contains("node"));
    // The shell's PATH still has it.
    assert_eq!(status_of(&report, "agent.program"), Status::Pass);
}

#[test]
fn systemd_warns_without_linger() {
    let mut world = World::healthy();
    world.use_systemd();
    let report = world.run();
    assert_eq!(status_of(&report, "schedule.installed"), Status::Pass);
    assert_eq!(status_of(&report, "schedule.binary"), Status::Pass);
    assert_eq!(status_of(&report, "schedule.path"), Status::Pass);
    assert_eq!(status_of(&report, "schedule.linger"), Status::Pass);
    assert!(
        world
            .system
            .calls
            .contains(&"loginctl show-user ada --property=Linger --value".to_owned())
    );

    world.system.linger = "no";
    let report = world.run();
    let check = find(&report, "schedule.linger");
    assert_eq!(check.status, Status::Warn);
    assert_eq!(
        check.fix.as_deref(),
        Some("Run `loginctl enable-linger` so Runs go on while you're logged out.")
    );
}

#[test]
fn no_scheduler_on_this_platform_skips_the_schedule() {
    let mut world = World::healthy();
    world.platform = Err("scheduling Runs is only supported on macOS and Linux".into());
    let report = world.run();
    assert_eq!(status_of(&report, "schedule"), Status::Skip);
}

// Sources.

#[test]
fn a_source_that_is_down_warns_rather_than_fails() {
    let mut world = World::healthy();
    world.network.down = vec!["devpost.com".into()];
    let report = world.run();
    let check = find(&report, "source.devpost");

    assert_eq!(check.status, Status::Warn);
    assert!(check.reason.contains("Connection refused"));
    assert_eq!(status_of(&report, "source.mlh"), Status::Pass);
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn every_planned_source_is_checked() {
    let report = World::healthy().run();
    let sources: Vec<&str> = report
        .checks
        .iter()
        .filter(|c| c.section == Section::Sources)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        sources,
        [
            "source.devpost",
            "source.mlh",
            "source.ethglobal",
            "source.cassini",
            "source.dev-events",
            "source.hackathon-be",
            "source.hackathonhub",
            "source.luma.1",
            "source.luma.2",
        ]
    );
}

#[test]
fn offline_skips_every_network_check() {
    let mut world = World::healthy();
    world.offline = true;
    let report = world.run();

    assert_eq!(status_of(&report, "sources"), Status::Skip);
    assert!(world.network.requested.borrow().is_empty());
    assert!(report.ok());
}

#[test]
fn a_private_calendar_shows_only_its_host() {
    let mut world = World::healthy();
    world.profile = Some(format!(
        "{PHOTON_PROFILE}\n[sources]\nics = [\"{PRIVATE_CALENDAR}\"]\n"
    ));
    world.network.down = vec!["calendar.example.org".into()];
    let report = world.run();
    let check = find(&report, "source.ics.1");

    assert_eq!(check.status, Status::Warn);
    assert_eq!(check.name, "Your calendar on calendar.example.org");
    assert!(!check.reason.contains("private"), "{}", check.reason);
}

// Last Run.

#[test]
fn no_run_yet_warns() {
    let mut world = World::healthy();
    world.last_run = None;
    let check = world.run();
    let check = find(&check, "last-run");
    assert_eq!(check.status, Status::Warn);
    assert!(
        check
            .fix
            .as_deref()
            .unwrap()
            .contains("hackathon-radar run")
    );
}

#[test]
fn a_failed_last_run_reports_its_failure_line() {
    let mut world = World::healthy();
    world.last_run = Some(run_record(
        now() - Duration::hours(5),
        Some("the Agent could not judge: claude is not signed in"),
    ));
    let report = world.run();
    let check = find(&report, "last-run");

    assert_eq!(check.status, Status::Warn);
    assert_eq!(
        check.reason,
        "The last Run, 2026-11-13 04:00 UTC (5 hours ago), failed: the Agent could not judge: \
         claude is not signed in"
    );
    assert_eq!(status_of(&report, "last-run.age"), Status::Pass);
}

#[test]
fn a_stale_last_run_warns_that_scheduled_runs_may_not_fire() {
    let mut world = World::healthy();
    // Monday and Thursday: the longest gap is 4 days, so 8 is the limit.
    world.last_run = Some(run_record(now() - Duration::days(9), None));
    let report = world.run();
    let check = find(&report, "last-run.age");

    assert_eq!(check.status, Status::Warn);
    assert!(check.reason.contains("9 days ago"));
    assert!(check.reason.contains("may not be firing"));
    assert_eq!(status_of(&report, "last-run"), Status::Pass);

    world.last_run = Some(run_record(now() - Duration::days(7), None));
    assert_eq!(status_of(&world.run(), "last-run.age"), Status::Pass);
}
