//! Seam 2 for the Scheduler: the rendered launchd and systemd files against saved
//! golden copies, and install, reinstall and uninstall against a fake
//! `launchctl`/`systemctl` that records every call. Nothing here touches the real
//! system: files go to a temp dir and no command is really run.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{NaiveTime, Weekday};
use tempfile::TempDir;

use hackathon_radar::profile::Schedule;
use hackathon_radar::scheduler::{
    self, CommandOutput, CommandRunner, Installed, Platform, ScheduledRun, Uninstalled, launchd,
    systemd,
};

const UID: u32 = 501;
const PLIST: &str = "io.github.edouard38000.hackathon-radar.plist";

fn golden(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scheduler")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn custom_schedule() -> Schedule {
    Schedule {
        days: vec![Weekday::Wed, Weekday::Sun],
        time: NaiveTime::from_hms_opt(21, 45, 0).unwrap(),
    }
}

/// The default schedule with ordinary paths.
fn default_job() -> ScheduledRun {
    ScheduledRun {
        binary: "/home/ada/.cargo/bin/hackathon-radar".into(),
        profile: "/home/ada/.config/hackathon-radar/profile.toml".into(),
        db: "/home/ada/.local/share/hackathon-radar/hackathon-radar.sqlite".into(),
        log: "/home/ada/.local/state/hackathon-radar/run.log".into(),
        schedule: Schedule::default(),
    }
}

/// A custom schedule with paths that need escaping in both formats.
fn custom_job() -> ScheduledRun {
    ScheduledRun {
        binary: "/opt/hackathon radar/bin/hackathon-radar".into(),
        profile: "/home/ada/Radar & <Co>/profile.toml".into(),
        db: "/home/ada/100% $data/radar.sqlite".into(),
        log: "/home/ada/logs/100% radar.log".into(),
        schedule: custom_schedule(),
    }
}

/// A job whose log lives in the temp dir, since install creates its directory.
fn job_in(tmp: &TempDir, schedule: Schedule) -> ScheduledRun {
    ScheduledRun {
        log: tmp.path().join("state/hackathon-radar/run.log"),
        schedule,
        ..default_job()
    }
}

/// Stands in for `launchctl` and `systemctl`: records each call, with the temp
/// dir shown as `<tmp>`, and tracks whether the agent or timer is loaded.
struct FakeSystem {
    tmp: PathBuf,
    calls: Vec<String>,
    loaded: bool,
    /// Calls starting with this fail, as a broken `launchctl` or `systemctl` would.
    failing: Option<&'static str>,
}

impl FakeSystem {
    fn new(tmp: &TempDir) -> Self {
        FakeSystem {
            tmp: tmp.path().to_owned(),
            calls: Vec::new(),
            loaded: false,
            failing: None,
        }
    }

    fn take_calls(&mut self) -> Vec<String> {
        std::mem::take(&mut self.calls)
    }
}

impl CommandRunner for FakeSystem {
    fn run(&mut self, program: &str, args: &[String]) -> io::Result<CommandOutput> {
        let line = std::iter::once(program.to_owned())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
            .replace(&*self.tmp.to_string_lossy(), "<tmp>");
        self.calls.push(line.clone());
        if self.failing.is_some_and(|prefix| line.starts_with(prefix)) {
            return Ok(CommandOutput {
                success: false,
                stdout: String::new(),
                stderr: "Bootstrap failed: 5: Input/output error\n".into(),
            });
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let success = match (program, args.as_slice()) {
            ("launchctl", ["print", ..]) => self.loaded,
            ("launchctl", ["bootstrap", ..]) => !std::mem::replace(&mut self.loaded, true),
            ("launchctl", ["bootout", ..]) => std::mem::replace(&mut self.loaded, false),
            ("systemctl", ["--user", "enable", "--now", ..]) => {
                self.loaded = true;
                true
            }
            ("systemctl", ["--user", "disable", "--now", ..]) => {
                self.loaded = false;
                true
            }
            ("systemctl", ["--user", "is-active", ..]) => self.loaded,
            ("systemctl", ["--user", ..]) => true,
            _ => panic!("unexpected command: {line}"),
        };
        Ok(CommandOutput {
            success,
            ..CommandOutput::default()
        })
    }
}

fn launchd_platform(tmp: &TempDir) -> Platform {
    Platform::Launchd {
        agents_dir: tmp.path().join("LaunchAgents"),
        uid: UID,
    }
}

fn systemd_platform(tmp: &TempDir) -> Platform {
    Platform::Systemd {
        unit_dir: tmp.path().join("systemd/user"),
    }
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

// --- Rendering -------------------------------------------------------------

#[test]
fn renders_the_launchd_plist_for_the_default_schedule() {
    assert_eq!(
        launchd::render_plist(&default_job()).unwrap(),
        golden("launchd-default.plist")
    );
}

#[test]
fn renders_the_launchd_plist_for_a_custom_schedule() {
    assert_eq!(
        launchd::render_plist(&custom_job()).unwrap(),
        golden("launchd-custom.plist")
    );
}

#[test]
fn renders_the_systemd_units_for_the_default_schedule() {
    let job = default_job();
    assert_eq!(
        systemd::render_service(&job).unwrap(),
        golden("systemd-default.service")
    );
    assert_eq!(
        systemd::render_timer(&job.schedule),
        golden("systemd-default.timer")
    );
    assert_eq!(
        systemd::on_calendar(&job.schedule),
        "Mon,Thu *-*-* 08:00:00"
    );
}

#[test]
fn renders_the_systemd_units_for_a_custom_schedule() {
    let job = custom_job();
    assert_eq!(
        systemd::render_service(&job).unwrap(),
        golden("systemd-custom.service")
    );
    assert_eq!(
        systemd::render_timer(&job.schedule),
        golden("systemd-custom.timer")
    );
    assert_eq!(
        systemd::on_calendar(&job.schedule),
        "Wed,Sun *-*-* 21:45:00"
    );
}

#[test]
fn refuses_relative_paths() {
    let job = ScheduledRun {
        profile: "profile.toml".into(),
        ..default_job()
    };
    let err = launchd::render_plist(&job).unwrap_err();
    assert!(err.to_string().contains("not an absolute path"), "{err}");
    assert!(systemd::render_service(&job).is_err());
}

// --- launchd ----------------------------------------------------------------

#[test]
fn launchd_install_writes_the_plist_and_bootstraps_it() {
    let tmp = TempDir::new().unwrap();
    let platform = launchd_platform(&tmp);
    let job = job_in(&tmp, Schedule::default());
    let mut system = FakeSystem::new(&tmp);

    let installed = scheduler::install(&platform, &job, &mut system).unwrap();

    let plist = tmp.path().join("LaunchAgents").join(PLIST);
    assert_eq!(
        installed,
        Installed {
            replaced: false,
            files: vec![plist.clone()],
        }
    );
    assert_eq!(
        system.take_calls(),
        [
            "launchctl print gui/501/io.github.edouard38000.hackathon-radar",
            "launchctl bootstrap gui/501 <tmp>/LaunchAgents/io.github.edouard38000.hackathon-radar.plist",
        ]
    );
    assert_eq!(
        std::fs::read_to_string(&plist).unwrap(),
        launchd::render_plist(&job).unwrap()
    );
    assert!(tmp.path().join("state/hackathon-radar").is_dir());
}

#[test]
fn launchd_reinstall_reloads_and_gives_the_same_result() {
    let tmp = TempDir::new().unwrap();
    let platform = launchd_platform(&tmp);
    let job = job_in(&tmp, Schedule::default());
    let mut system = FakeSystem::new(&tmp);
    scheduler::install(&platform, &job, &mut system).unwrap();
    let first = std::fs::read_to_string(tmp.path().join("LaunchAgents").join(PLIST)).unwrap();
    system.take_calls();

    let installed = scheduler::install(&platform, &job, &mut system).unwrap();

    assert!(installed.replaced);
    assert_eq!(
        system.take_calls(),
        [
            "launchctl print gui/501/io.github.edouard38000.hackathon-radar",
            "launchctl bootout gui/501/io.github.edouard38000.hackathon-radar",
            "launchctl bootstrap gui/501 <tmp>/LaunchAgents/io.github.edouard38000.hackathon-radar.plist",
        ]
    );
    assert_eq!(files_in(&tmp.path().join("LaunchAgents")), [PLIST]);
    let second = std::fs::read_to_string(tmp.path().join("LaunchAgents").join(PLIST)).unwrap();
    assert_eq!(first, second);
    assert!(system.loaded);
}

#[test]
fn launchd_reinstall_picks_up_a_new_schedule() {
    let tmp = TempDir::new().unwrap();
    let platform = launchd_platform(&tmp);
    let mut system = FakeSystem::new(&tmp);
    scheduler::install(&platform, &job_in(&tmp, Schedule::default()), &mut system).unwrap();

    let changed = job_in(&tmp, custom_schedule());
    scheduler::install(&platform, &changed, &mut system).unwrap();

    assert_eq!(
        std::fs::read_to_string(tmp.path().join("LaunchAgents").join(PLIST)).unwrap(),
        launchd::render_plist(&changed).unwrap()
    );
}

#[test]
fn launchd_uninstall_boots_out_and_removes_the_plist() {
    let tmp = TempDir::new().unwrap();
    let platform = launchd_platform(&tmp);
    let mut system = FakeSystem::new(&tmp);
    scheduler::install(&platform, &job_in(&tmp, Schedule::default()), &mut system).unwrap();
    system.take_calls();

    let removed = scheduler::uninstall(&platform, &mut system).unwrap();

    assert_eq!(
        removed,
        Uninstalled::Removed {
            files: vec![tmp.path().join("LaunchAgents").join(PLIST)],
        }
    );
    assert_eq!(
        system.take_calls(),
        [
            "launchctl print gui/501/io.github.edouard38000.hackathon-radar",
            "launchctl bootout gui/501/io.github.edouard38000.hackathon-radar",
        ]
    );
    assert!(files_in(&tmp.path().join("LaunchAgents")).is_empty());
}

#[test]
fn launchd_uninstall_when_not_installed_succeeds() {
    let tmp = TempDir::new().unwrap();
    let mut system = FakeSystem::new(&tmp);

    let removed = scheduler::uninstall(&launchd_platform(&tmp), &mut system).unwrap();

    assert_eq!(removed, Uninstalled::NotInstalled);
    assert_eq!(
        system.take_calls(),
        ["launchctl print gui/501/io.github.edouard38000.hackathon-radar"]
    );
}

#[test]
fn launchd_status_follows_install_and_uninstall() {
    let tmp = TempDir::new().unwrap();
    let platform = launchd_platform(&tmp);
    let mut system = FakeSystem::new(&tmp);

    let before = scheduler::status(&platform, &mut system).unwrap();
    assert!(!before.installed && !before.active);

    scheduler::install(&platform, &job_in(&tmp, Schedule::default()), &mut system).unwrap();
    let during = scheduler::status(&platform, &mut system).unwrap();
    assert!(during.installed && during.active);
    assert_eq!(during.files, [tmp.path().join("LaunchAgents").join(PLIST)]);

    scheduler::uninstall(&platform, &mut system).unwrap();
    let after = scheduler::status(&platform, &mut system).unwrap();
    assert!(!after.installed && !after.active);
}

#[test]
fn a_failing_launchctl_is_reported() {
    let tmp = TempDir::new().unwrap();
    let mut system = FakeSystem::new(&tmp);
    system.failing = Some("launchctl bootstrap");

    let err = scheduler::install(
        &launchd_platform(&tmp),
        &job_in(&tmp, Schedule::default()),
        &mut system,
    )
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("failed: Bootstrap failed: 5: Input/output error"),
        "{err}"
    );
}

// --- systemd ----------------------------------------------------------------

#[test]
fn systemd_install_writes_the_units_and_enables_the_timer() {
    let tmp = TempDir::new().unwrap();
    let platform = systemd_platform(&tmp);
    let job = job_in(&tmp, Schedule::default());
    let mut system = FakeSystem::new(&tmp);

    let installed = scheduler::install(&platform, &job, &mut system).unwrap();

    let unit_dir = tmp.path().join("systemd/user");
    assert_eq!(
        installed,
        Installed {
            replaced: false,
            files: vec![
                unit_dir.join("hackathon-radar.service"),
                unit_dir.join("hackathon-radar.timer"),
            ],
        }
    );
    assert_eq!(
        system.take_calls(),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable --now hackathon-radar.timer",
        ]
    );
    assert_eq!(
        std::fs::read_to_string(unit_dir.join("hackathon-radar.service")).unwrap(),
        systemd::render_service(&job).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(unit_dir.join("hackathon-radar.timer")).unwrap(),
        systemd::render_timer(&job.schedule)
    );
    assert!(tmp.path().join("state/hackathon-radar").is_dir());
}

#[test]
fn systemd_reinstall_reloads_restarts_and_gives_the_same_result() {
    let tmp = TempDir::new().unwrap();
    let platform = systemd_platform(&tmp);
    let job = job_in(&tmp, Schedule::default());
    let mut system = FakeSystem::new(&tmp);
    scheduler::install(&platform, &job, &mut system).unwrap();
    system.take_calls();

    let installed = scheduler::install(&platform, &job, &mut system).unwrap();

    assert!(installed.replaced);
    assert_eq!(
        system.take_calls(),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable --now hackathon-radar.timer",
            "systemctl --user restart hackathon-radar.timer",
        ]
    );
    let unit_dir = tmp.path().join("systemd/user");
    assert_eq!(
        files_in(&unit_dir),
        ["hackathon-radar.service", "hackathon-radar.timer"]
    );
    assert_eq!(
        std::fs::read_to_string(unit_dir.join("hackathon-radar.timer")).unwrap(),
        systemd::render_timer(&job.schedule)
    );
}

#[test]
fn systemd_uninstall_disables_and_removes_the_units() {
    let tmp = TempDir::new().unwrap();
    let platform = systemd_platform(&tmp);
    let mut system = FakeSystem::new(&tmp);
    scheduler::install(&platform, &job_in(&tmp, Schedule::default()), &mut system).unwrap();
    system.take_calls();

    let removed = scheduler::uninstall(&platform, &mut system).unwrap();

    let unit_dir = tmp.path().join("systemd/user");
    assert_eq!(
        removed,
        Uninstalled::Removed {
            files: vec![
                unit_dir.join("hackathon-radar.service"),
                unit_dir.join("hackathon-radar.timer"),
            ],
        }
    );
    assert_eq!(
        system.take_calls(),
        [
            "systemctl --user disable --now hackathon-radar.timer",
            "systemctl --user daemon-reload",
        ]
    );
    assert!(files_in(&unit_dir).is_empty());
}

#[test]
fn systemd_uninstall_when_not_installed_succeeds() {
    let tmp = TempDir::new().unwrap();
    let mut system = FakeSystem::new(&tmp);

    let removed = scheduler::uninstall(&systemd_platform(&tmp), &mut system).unwrap();

    assert_eq!(removed, Uninstalled::NotInstalled);
    assert!(system.take_calls().is_empty());
}

#[test]
fn systemd_status_follows_install_and_uninstall() {
    let tmp = TempDir::new().unwrap();
    let platform = systemd_platform(&tmp);
    let mut system = FakeSystem::new(&tmp);

    let before = scheduler::status(&platform, &mut system).unwrap();
    assert!(!before.installed && !before.active);
    assert_eq!(
        system.take_calls(),
        ["systemctl --user is-active --quiet hackathon-radar.timer"]
    );

    scheduler::install(&platform, &job_in(&tmp, Schedule::default()), &mut system).unwrap();
    let during = scheduler::status(&platform, &mut system).unwrap();
    assert!(during.installed && during.active);

    scheduler::uninstall(&platform, &mut system).unwrap();
    let after = scheduler::status(&platform, &mut system).unwrap();
    assert!(!after.installed && !after.active);
}
