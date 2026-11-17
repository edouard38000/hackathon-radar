//! The live end-to-end test: the manual release check, run against the real
//! world. It is ignored by default, so `cargo test` and CI never run it; the
//! maintainer runs it on purpose before tagging a release:
//!
//! ```sh
//! cargo test --locked --test live -- --ignored
//! ```
//!
//! It drives the built binary through `init`, `doctor`, two dry runs, a real
//! Run and `schedule install`/`uninstall`, with a temporary Profile, database
//! and XDG directories. It needs the network and a signed-in `claude` on the
//! `PATH`, and passes with a "skipped" line when either is missing. It is the
//! only test that touches the network.
//!
//! What it leaves behind: a few ntfy messages on its own random topic on
//! ntfy.sh, and nothing else. It refuses to start when a hackathon-radar
//! schedule is already installed, because installing its own would replace
//! that one, and a drop guard removes the schedule it installs even when a
//! step panics. On macOS the schedule commands get a temporary `HOME`, so the
//! plist is written inside the temporary directory too; on Linux the systemd
//! units have to go where the user's systemd reads them, and are removed again.
//!
//! The Profile has no secrets of its own: the secret environment variables are
//! cleared for every command and nothing is written to the keychain. Runs still
//! read the keychain like any Run would, so a SerpApi key stored there by
//! `hackathon-radar init` is used for Fares.

use std::collections::BTreeSet;
use std::hash::{BuildHasher, Hasher};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use hackathon_radar::http::{HttpClient, HttpGet};
use hackathon_radar::scheduler::{LABEL, launchd, systemd};
use hackathon_radar::store::Store;
use hackathon_radar::{paths, wiring};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_hackathon-radar");
const NTFY: &str = "https://ntfy.sh";

/// Secrets a command could pick up from the shell; cleared so the test's
/// Profile never uses the person's own.
const SECRET_ENV_VARS: &[&str] = &[
    "SERPAPI_API_KEY",
    "HACKATHON_RADAR_NTFY_TOKEN",
    "PHOTON_PROJECT_ID",
    "PHOTON_PROJECT_SECRET",
];

#[test]
#[ignore = "live: needs the network and a signed-in `claude`; run with `cargo test --test live -- --ignored`"]
fn live_end_to_end() {
    if let Some(reason) = missing_prerequisite() {
        say(&format!("SKIPPED: {reason}"));
        return;
    }
    if let Some(existing) = installed_schedule(&RealLocations::of_this_user()) {
        panic!(
            "refusing to run: a hackathon-radar schedule is already installed ({existing}). \
             This test installs and removes the schedule, which would replace yours. \
             Run it on a machine without one, or `hackathon-radar schedule uninstall` first \
             and `hackathon-radar schedule install` again afterwards."
        );
    }

    let sandbox = Sandbox::new();
    // Declared after the sandbox, so it is dropped first: the schedule goes
    // before the temporary directory, whatever happens in between.
    let _guard = ScheduleGuard {
        sandbox: &sandbox,
        real: RealLocations::of_this_user(),
    };
    say(&format!(
        "temporary directory {}, ntfy topic {}",
        sandbox.dir.path().display(),
        sandbox.topic
    ));

    init_sends_a_test_alert(&sandbox);
    doctor_is_ok(&sandbox);
    let first_dry_run = dry_run(&sandbox, "step 3: run --dry-run (this takes a few minutes)");
    assert!(
        first_dry_run.contains("welcome!"),
        "the first Alert is a Welcome Alert:\n{first_dry_run}"
    );
    let announced = run_sends_the_welcome_alert(&sandbox);
    second_dry_run_announces_nothing_again(&sandbox, &announced);
    schedule_installs_and_uninstalls(&sandbox);
    say("PASSED: every step of the live end-to-end test");
}

/// Step 1: `init --yes` writes the Profile and a test Alert reaches ntfy.sh.
fn init_sends_a_test_alert(sandbox: &Sandbox) {
    say("step 1: init --yes, with a test Alert over ntfy");
    let started = Instant::now();
    let output = sandbox.run(&[
        "init",
        "--yes",
        "--home-city",
        "Brussels",
        "--home-country",
        "Belgium",
        "--airports",
        "BRU,CRL",
        "--countries",
        "Belgium, France, Netherlands, Luxembourg, Germany, Switzerland, United Kingdom",
        "--currency",
        "EUR",
        "--train-limit",
        "4",
        "--flight-budget",
        "200",
        "--travel-funded-anywhere",
        "true",
        "--themes",
        "none",
        "--student",
        "false",
        "--nationalities",
        "French",
        "--age",
        "none",
        "--agent",
        "claude-code",
        "--alert-channel",
        "ntfy",
        "--ntfy-topic",
        &sandbox.topic,
        "--test-alert",
        "true",
        "--install-schedule",
        "false",
    ]);
    succeeded("init", &output);
    assert!(sandbox.profile.exists(), "init wrote no Profile");
    let messages = sandbox.wait_for_messages(|m| m.iter().any(|m| m == wiring::TEST_ALERT));
    say(&format!(
        "  the test Alert reached ntfy.sh ({} message(s) on the topic) in {}",
        messages.len(),
        elapsed(started)
    ));
}

/// Step 2: `doctor --json` finds nothing failing.
fn doctor_is_ok(sandbox: &Sandbox) {
    say("step 2: doctor --json");
    let output = sandbox.run(&["doctor", "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("doctor --json printed no JSON ({e}):\n{stdout}"));
    let checks = report["checks"].as_array().cloned().unwrap_or_default();
    let with = |status: &str| -> Vec<String> {
        checks
            .iter()
            .filter(|c| c["status"] == status)
            .map(|c| {
                format!(
                    "{}: {}",
                    c["id"].as_str().unwrap_or("?"),
                    c["reason"].as_str().unwrap_or("")
                )
            })
            .collect()
    };
    let failed = with("fail");
    assert!(
        report["ok"] == true && output.status.success() && failed.is_empty(),
        "doctor failed checks:\n{}",
        failed.join("\n")
    );
    say(&format!("  ok: true, counts {}", report["counts"]));
    for warning in with("warn") {
        say(&format!("  warn {warning}"));
    }
}

/// Steps 3 and 5: `run --dry-run` collects Candidates; returns the Alert it
/// would send, or an empty string when there is nothing to send.
fn dry_run(sandbox: &Sandbox, step: &str) -> String {
    say(step);
    let started = Instant::now();
    let output = sandbox.run(&["run", "--dry-run"]);
    let stdout = succeeded("run --dry-run", &output);
    let collected = collected(&stdout);
    assert!(
        collected > 0,
        "the dry run collected no Candidates:\n{stdout}"
    );
    say_summary(&stdout, started);
    match stdout.split_once("The Alert would be:\n\n") {
        Some((_, alert)) => {
            say_indented(alert.trim_end());
            alert.trim_end().to_owned()
        }
        None => {
            assert!(
                stdout.contains("Dry run: nothing to send."),
                "the dry run printed neither an Alert nor nothing to send:\n{stdout}"
            );
            say("  nothing to send");
            String::new()
        }
    }
}

/// Step 4: a real `run` sends the Welcome Alert over ntfy, and the Hackathons
/// it lists are Announced. Returns every Announced Hackathon's name and link.
fn run_sends_the_welcome_alert(sandbox: &Sandbox) -> Announced {
    say("step 4: run, sending the Welcome Alert over ntfy");
    let started = Instant::now();
    let output = sandbox.run(&["run"]);
    let stdout = succeeded("run", &output);
    assert!(collected(&stdout) > 0, "the Run collected no Candidates");
    assert!(
        stdout.contains("Sent the Alert through ntfy."),
        "the Run sent no Alert:\n{stdout}"
    );
    say_summary(&stdout, started);

    let messages = sandbox.wait_for_messages(|m| m.iter().any(|m| is_welcome_alert(m)));
    let sent = messages
        .iter()
        .find(|m| is_welcome_alert(m))
        .expect("waited for it");
    say("  the Welcome Alert reached ntfy.sh:");
    say_indented(sent);

    let announced = Announced::read(&sandbox.db);
    assert!(
        !announced.names.is_empty(),
        "the Welcome Alert was sent but no Hackathon is Announced"
    );
    for entry in entries(sent) {
        assert!(
            announced.lists(&entry),
            "the Welcome Alert listed {:?}, which is not Announced",
            entry.name
        );
    }
    say(&format!(
        "  {} Hackathons are Announced",
        announced.names.len()
    ));
    announced
}

/// Step 5: a second dry run lists no Hackathon the Welcome Alert announced,
/// except as a Deadline reminder or as newly Travel-funded, which are the only
/// reasons an Alert mentions an Announced Hackathon again.
fn second_dry_run_announces_nothing_again(sandbox: &Sandbox, announced: &Announced) {
    let alert = dry_run(
        sandbox,
        "step 5: a second run --dry-run, which lists nothing already Announced",
    );
    assert!(
        !alert.contains("welcome!"),
        "a second Alert is not a Welcome Alert:\n{alert}"
    );
    let again: Vec<String> = entries(&alert)
        .into_iter()
        .filter(|entry| announced.lists(entry) && !entry.is_reminder_or_newly_funded())
        .map(|entry| entry.block)
        .collect();
    assert!(
        again.is_empty(),
        "the second dry run announces these Hackathons again:\n\n{}",
        again.join("\n\n")
    );
    assert_eq!(
        Announced::read(&sandbox.db).names,
        announced.names,
        "a dry run must not change what is Announced"
    );
    say("  no Announced Hackathon was listed again");
}

/// Step 6: `schedule install` then `schedule uninstall`, leaving nothing behind.
fn schedule_installs_and_uninstalls(sandbox: &Sandbox) {
    say("step 6: schedule install, status and uninstall");
    let profile = sandbox.profile.to_str().unwrap();
    let db = sandbox.db.to_str().unwrap();
    let output = sandbox.schedule(&["install", "--profile", profile, "--db", db]);
    let stdout = succeeded("schedule install", &output);
    say_indented(stdout.trim_end());
    let used = sandbox.schedule_locations();
    assert!(
        installed_schedule(&used).is_some(),
        "schedule install left no schedule in place"
    );
    let status = succeeded("schedule status", &sandbox.schedule(&["status"]));
    assert!(
        status.contains("installed and active"),
        "the installed schedule is not active:\n{status}"
    );
    say("  installed and active");

    let output = sandbox.schedule(&["uninstall"]);
    let stdout = succeeded("schedule uninstall", &output);
    say_indented(stdout.trim_end());
    assert_eq!(
        installed_schedule(&used),
        None,
        "the schedule is still there"
    );
    let real = RealLocations::of_this_user();
    assert_eq!(
        installed_schedule(&real),
        None,
        "a schedule is left in the person's own scheduler"
    );
    say("  uninstalled; no launchd agent or systemd unit is left");
}

/// The temporary home of one live test: its directories, Profile, database and
/// ntfy topic.
struct Sandbox {
    dir: TempDir,
    profile: PathBuf,
    db: PathBuf,
    topic: String,
}

impl Sandbox {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let profile = dir.path().join("config/hackathon-radar/profile.toml");
        let db = dir
            .path()
            .join("data/hackathon-radar/hackathon-radar.sqlite");
        Self {
            dir,
            profile,
            db,
            topic: format!("hr-e2e-{:016x}{:016x}", random(), random()),
        }
    }

    /// The binary with the temporary XDG directories, no secrets from the
    /// shell, and `--profile` and `--db` pointing into the temporary directory.
    fn command(&self, args: &[&str], with_paths: bool) -> Command {
        let mut command = Command::new(BINARY);
        command.args(args);
        if with_paths {
            command
                .arg("--profile")
                .arg(&self.profile)
                .arg("--db")
                .arg(&self.db);
        }
        command
            .env("XDG_CONFIG_HOME", self.dir.path().join("config"))
            .env("XDG_DATA_HOME", self.dir.path().join("data"))
            .env("XDG_STATE_HOME", self.dir.path().join("state"))
            .stdin(Stdio::null());
        for var in SECRET_ENV_VARS {
            command.env_remove(var);
        }
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args, true)
            .output()
            .unwrap_or_else(|e| panic!("could not start {BINARY}: {e}"))
    }

    /// A `schedule` command. On macOS it gets a temporary `HOME`, so the plist
    /// goes in the temporary directory; launchd loads it from there. On Linux
    /// systemd only reads units from the user's own unit directory, so the
    /// real `HOME` and `XDG_CONFIG_HOME` are kept.
    fn schedule(&self, args: &[&str]) -> Output {
        let mut full = vec!["schedule"];
        full.extend_from_slice(args);
        let mut command = self.command(&full, false);
        if cfg!(target_os = "macos") {
            command.env("HOME", self.home());
        } else {
            match std::env::var_os("XDG_CONFIG_HOME") {
                Some(dir) => command.env("XDG_CONFIG_HOME", dir),
                None => command.env_remove("XDG_CONFIG_HOME"),
            };
        }
        command
            .output()
            .unwrap_or_else(|e| panic!("could not start {BINARY}: {e}"))
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    /// Where the `schedule` commands install the schedule.
    fn schedule_locations(&self) -> RealLocations {
        if cfg!(target_os = "macos") {
            RealLocations {
                launch_agents: Some(self.home().join("Library/LaunchAgents")),
                systemd_units: None,
            }
        } else {
            RealLocations::of_this_user()
        }
    }

    /// Polls the topic until `done` holds for its messages, for up to a minute.
    fn wait_for_messages(&self, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let messages = ntfy_messages(&self.topic);
            if done(&messages) {
                return messages;
            }
            assert!(
                Instant::now() < deadline,
                "the expected message did not reach ntfy.sh within a minute; the topic has:\n{}",
                messages.join("\n---\n")
            );
            std::thread::sleep(Duration::from_secs(3));
        }
    }
}

/// Where a schedule can be: the launchd agents directory (macOS) or the systemd
/// user unit directory (Linux).
#[derive(Debug)]
struct RealLocations {
    launch_agents: Option<PathBuf>,
    systemd_units: Option<PathBuf>,
}

impl RealLocations {
    /// This user's own scheduler directories, from this process's environment.
    fn of_this_user() -> Self {
        if cfg!(target_os = "macos") {
            Self {
                launch_agents: paths::launch_agents_dir().ok(),
                systemd_units: None,
            }
        } else {
            Self {
                launch_agents: None,
                systemd_units: paths::systemd_user_dir().ok(),
            }
        }
    }
}

/// What makes a hackathon-radar schedule installed here, if anything: its
/// files, or the scheduler having it loaded.
fn installed_schedule(at: &RealLocations) -> Option<String> {
    let mut found = Vec::new();
    if let Some(dir) = &at.launch_agents {
        let plist = launchd::plist_path(dir);
        if plist.exists() {
            found.push(plist.display().to_string());
        }
    }
    if cfg!(target_os = "macos") && succeeds("launchctl", &["print", &launchd_target()]) {
        found.push(format!("launchd has {LABEL} loaded"));
    }
    if let Some(dir) = &at.systemd_units {
        for unit in [systemd::SERVICE, systemd::TIMER] {
            if dir.join(unit).exists() {
                found.push(dir.join(unit).display().to_string());
            }
        }
    }
    if cfg!(target_os = "linux")
        && succeeds(
            "systemctl",
            &["--user", "is-enabled", "--quiet", systemd::TIMER],
        )
    {
        found.push(format!("systemd has {} enabled", systemd::TIMER));
    }
    (!found.is_empty()).then(|| found.join(", "))
}

/// Removes the schedule the test installed, even when a step panics. The test
/// refused to start if a schedule was already there, so whatever it finds is
/// the test's own. It never panics, since it may run while unwinding.
struct ScheduleGuard<'a> {
    sandbox: &'a Sandbox,
    real: RealLocations,
}

impl Drop for ScheduleGuard<'_> {
    fn drop(&mut self) {
        let used = self.sandbox.schedule_locations();
        if installed_schedule(&used).is_none() && installed_schedule(&self.real).is_none() {
            return;
        }
        say("cleanup: removing the schedule the test installed");
        let _ = self.sandbox.schedule(&["uninstall"]);
        if cfg!(target_os = "macos") {
            if succeeds("launchctl", &["print", &launchd_target()]) {
                let _ = succeeds("launchctl", &["bootout", &launchd_target()]);
            }
            if let Some(dir) = &used.launch_agents {
                let _ = std::fs::remove_file(launchd::plist_path(dir));
            }
        } else if let Some(dir) = &used.systemd_units {
            let _ = succeeds("systemctl", &["--user", "disable", "--now", systemd::TIMER]);
            for unit in [systemd::SERVICE, systemd::TIMER] {
                let _ = std::fs::remove_file(dir.join(unit));
            }
            let _ = succeeds("systemctl", &["--user", "daemon-reload"]);
        }
        for at in [&used, &self.real] {
            if let Some(left) = installed_schedule(at) {
                say(&format!(
                    "cleanup FAILED: remove it by hand with `hackathon-radar schedule uninstall`: {left}"
                ));
            }
        }
    }
}

/// `gui/<uid>/<label>`, the launchd service the schedule is loaded as.
fn launchd_target() -> String {
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    format!("gui/{uid}/{LABEL}")
}

/// The Hackathons an Alert has announced, by name and link.
#[derive(Debug, PartialEq, Eq)]
struct Announced {
    names: BTreeSet<String>,
    links: BTreeSet<String>,
}

impl Announced {
    fn read(db: &Path) -> Self {
        let announced: Vec<_> = Store::open(db)
            .unwrap()
            .hackathons()
            .unwrap()
            .into_iter()
            .filter(|h| h.announced_on.is_some())
            .collect();
        Self {
            names: announced.iter().map(|h| h.name.clone()).collect(),
            links: announced.iter().filter_map(|h| h.link.clone()).collect(),
        }
    }

    /// Whether this Alert entry is for an Announced Hackathon.
    fn lists(&self, entry: &Entry) -> bool {
        self.names.contains(&entry.name)
            || entry.link.as_ref().is_some_and(|l| self.links.contains(l))
    }
}

/// One Hackathon listed in an Alert: its name, its link and its whole block.
struct Entry {
    name: String,
    link: Option<String>,
    block: String,
}

impl Entry {
    /// A Deadline reminder shows the days left; a known Hackathon that became
    /// Travel-funded says so.
    fn is_reminder_or_newly_funded(&self) -> bool {
        [
            "newly Travel-funded",
            "days left",
            "1 day left",
            "closes today",
        ]
        .iter()
        .any(|marker| self.block.contains(marker))
    }
}

/// The entries of an Alert's text: the blocks after the heading, without the
/// "+ X more" line.
fn entries(alert: &str) -> Vec<Entry> {
    alert
        .split("\n\n")
        .skip(1)
        .filter(|block| !block.trim().is_empty() && !block.starts_with("+ "))
        .map(|block| {
            let lines: Vec<&str> = block.lines().collect();
            Entry {
                name: lines[0].trim().to_owned(),
                link: lines
                    .last()
                    .filter(|l| l.starts_with("http"))
                    .map(|l| l.trim().to_owned()),
                block: block.to_owned(),
            }
        })
        .collect()
}

fn is_welcome_alert(message: &str) -> bool {
    message.starts_with("hackathon-radar: welcome!")
}

/// The messages on an ntfy.sh topic, oldest first.
fn ntfy_messages(topic: &str) -> Vec<String> {
    let url = format!("{NTFY}/{topic}/json?poll=1&since=all");
    let body = HttpClient::with_timeout(Duration::from_secs(20))
        .get_text(&url)
        .unwrap_or_else(|e| panic!("could not poll ntfy.sh: {e:#}"));
    body.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == "message")
        .filter_map(|event| event["message"].as_str().map(str::to_owned))
        .collect()
}

/// Why the test can't run here, if it can't: no network, or no signed-in
/// `claude` on the `PATH`.
fn missing_prerequisite() -> Option<String> {
    if let Err(e) =
        HttpClient::with_timeout(Duration::from_secs(10)).get_text(&format!("{NTFY}/v1/health"))
    {
        return Some(format!("no network: could not reach ntfy.sh ({e:#})"));
    }
    let Some(claude) = on_path("claude") else {
        return Some("no `claude` on the PATH; install Claude Code and sign in".into());
    };
    // `claude auth status` exits non-zero when signed out, with the same JSON.
    let signed_in = Command::new(&claude)
        .args(["auth", "status"])
        .stdin(Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
            let start = stdout.find('{')?;
            serde_json::from_str::<serde_json::Value>(&stdout[start..]).ok()
        })
        .is_some_and(|status| status["loggedIn"] == true);
    if !signed_in {
        return Some(format!(
            "{} is not signed in; run `claude` once and sign in",
            claude.display()
        ));
    }
    None
}

fn on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}

/// Whether a command runs and exits 0; its output is discarded.
fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Asserts the command exited 0 and returns its stdout.
fn succeeded(what: &str, output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "`hackathon-radar {what}` failed ({}):\n--- stdout\n{stdout}\n--- stderr\n{stderr}",
        output.status
    );
    for line in stderr.lines().filter(|l| !l.trim().is_empty()) {
        say(&format!("  stderr: {line}"));
    }
    stdout
}

/// The number in "Collected N Candidates".
fn collected(stdout: &str) -> usize {
    stdout
        .split_once("Collected ")
        .and_then(|(_, rest)| rest.split_once(' '))
        .and_then(|(n, _)| n.parse().ok())
        .unwrap_or(0)
}

/// The Run's summary lines, before its Alert.
fn say_summary(stdout: &str, started: Instant) {
    for line in stdout.lines().take_while(|l| !l.is_empty()) {
        say(&format!("  {line}"));
    }
    say(&format!("  took {}", elapsed(started)));
}

fn say_indented(text: &str) {
    for line in text.lines() {
        say(&format!("    | {line}"));
    }
}

/// Progress goes straight to stderr, past the test harness's capture, so a
/// skip or a slow step shows without `--nocapture`.
fn say(message: &str) {
    let _ = writeln!(std::io::stderr(), "live: {message}");
}

fn elapsed(since: Instant) -> String {
    let seconds = since.elapsed().as_secs();
    format!("{}m{:02}s", seconds / 60, seconds % 60)
}

/// A random number, from the standard library's randomly keyed hasher.
fn random() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    hasher.finish()
}
