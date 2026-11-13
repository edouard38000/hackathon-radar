//! The Scheduler: installs and removes the recurring Run, as a launchd agent on
//! macOS or a systemd user timer on Linux.
//!
//! The files are rendered by pure functions ([`launchd::render_plist`],
//! [`systemd::render_service`], [`systemd::render_timer`]). Every `launchctl` and
//! `systemctl` call goes through a [`CommandRunner`], so tests use a fake one and
//! never touch the real system.

pub mod launchd;
pub mod systemd;

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

use crate::profile::Schedule;

/// The launchd label, also used as the plist's file name.
pub const LABEL: &str = "io.github.edouard38000.hackathon-radar";

/// What a scheduled Run executes, when, and where it logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledRun {
    /// The absolute path of the `hackathon-radar` binary.
    pub binary: PathBuf,
    /// The absolute path of the Profile, passed as `--profile`.
    pub profile: PathBuf,
    /// The absolute path of the database, passed as `--db`.
    pub db: PathBuf,
    /// Where each Run's stdout and stderr are appended.
    pub log: PathBuf,
    pub schedule: Schedule,
    /// The `PATH` each Run gets, so it finds the Agent's CLI (`claude`, `codex`):
    /// launchd and systemd otherwise start jobs with a minimal one.
    pub path: String,
}

/// The `PATH` used when the installing shell has none.
pub const MACOS_DEFAULT_PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";
/// The `PATH` used when the installing shell has none.
pub const LINUX_DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// The fallback `PATH` for this platform.
pub fn default_path() -> &'static str {
    if cfg!(target_os = "macos") {
        MACOS_DEFAULT_PATH
    } else {
        LINUX_DEFAULT_PATH
    }
}

/// The `PATH` for scheduled Runs, taken from the installing shell's `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPath {
    pub value: String,
    /// True when the shell's `PATH` was unset, empty or not UTF-8, so
    /// [`default_path`] is used instead.
    pub defaulted: bool,
}

impl RunPath {
    /// `from_env` is the installing shell's `PATH`, such as `std::env::var_os("PATH")`.
    pub fn from_env(from_env: Option<std::ffi::OsString>) -> Self {
        match from_env
            .and_then(|p| p.into_string().ok())
            .filter(|p| !p.trim().is_empty())
        {
            Some(value) => RunPath {
                value,
                defaulted: false,
            },
            None => RunPath {
                value: default_path().to_owned(),
                defaulted: true,
            },
        }
    }
}

impl ScheduledRun {
    /// The `PATH` as text the scheduler files can hold: on one line.
    pub fn path_env(&self) -> anyhow::Result<&str> {
        if self.path.chars().any(char::is_control) {
            bail!("PATH {:?} contains a control character", self.path);
        }
        Ok(&self.path)
    }

    /// The program and its arguments: `<binary> run --profile <profile> --db <db>`.
    pub fn command_line(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            path_text(&self.binary)?,
            "run".to_owned(),
            "--profile".to_owned(),
            path_text(&self.profile)?,
            "--db".to_owned(),
            path_text(&self.db)?,
        ])
    }

    pub fn log_text(&self) -> anyhow::Result<String> {
        path_text(&self.log)
    }
}

/// A path as text the scheduler files can hold: absolute, UTF-8, on one line.
fn path_text(path: &Path) -> anyhow::Result<String> {
    let text = path
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", path.display()))?;
    if !path.is_absolute() {
        bail!("{text} is not an absolute path");
    }
    if text.chars().any(char::is_control) {
        bail!("{text:?} contains a control character");
    }
    Ok(text.to_owned())
}

/// Where the schedule is installed on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    /// macOS: a LaunchAgent plist in `agents_dir`, loaded into `gui/<uid>`.
    Launchd { agents_dir: PathBuf, uid: u32 },
    /// Linux: a systemd user service and timer in `unit_dir`.
    Systemd { unit_dir: PathBuf },
}

impl Platform {
    /// The platform this binary runs on, with its default locations.
    pub fn detect(runner: &mut dyn CommandRunner) -> anyhow::Result<Self> {
        if cfg!(target_os = "macos") {
            let output = runner.run("id", &["-u".to_owned()])?;
            let uid = output
                .stdout
                .trim()
                .parse()
                .ok()
                .filter(|_| output.success)
                .context("could not read the user id from `id -u`")?;
            Ok(Platform::Launchd {
                agents_dir: crate::paths::launch_agents_dir()?,
                uid,
            })
        } else if cfg!(target_os = "linux") {
            Ok(Platform::Systemd {
                unit_dir: crate::paths::systemd_user_dir()?,
            })
        } else {
            bail!("scheduling Runs is only supported on macOS (launchd) and Linux (systemd)")
        }
    }
}

/// Runs an external command and captures its output.
pub trait CommandRunner {
    fn run(&mut self, program: &str, args: &[String]) -> io::Result<CommandOutput>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs commands for real.
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&mut self, program: &str, args: &[String]) -> io::Result<CommandOutput> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Runs a command and fails, with its stderr, unless it succeeds.
fn run_checked(
    runner: &mut dyn CommandRunner,
    program: &str,
    args: &[String],
) -> anyhow::Result<()> {
    let output = runner
        .run(program, args)
        .with_context(|| format!("could not run `{program}`"))?;
    if !output.success {
        let detail = [output.stderr.trim(), output.stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("no output");
        bail!("`{program} {}` failed: {detail}", args.join(" "));
    }
    Ok(())
}

/// Whether a command succeeds; only failing to start it is an error.
fn succeeds(
    runner: &mut dyn CommandRunner,
    program: &str,
    args: &[String],
) -> anyhow::Result<bool> {
    Ok(runner
        .run(program, args)
        .with_context(|| format!("could not run `{program}`"))?
        .success)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// True when a schedule was already installed and has been replaced.
    pub replaced: bool,
    /// The files written.
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uninstalled {
    Removed { files: Vec<PathBuf> },
    NotInstalled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The scheduler's files are in place.
    pub installed: bool,
    /// launchd has the agent loaded, or systemd has the timer active.
    pub active: bool,
    /// Where the files are, or would be.
    pub files: Vec<PathBuf>,
}

/// Installs the schedule, replacing any earlier one, so installing twice gives
/// the same result as installing once.
pub fn install(
    platform: &Platform,
    job: &ScheduledRun,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Installed> {
    if let Some(dir) = job.log.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create the log directory {}", dir.display()))?;
    }
    match platform {
        Platform::Launchd { agents_dir, uid } => launchd::install(agents_dir, *uid, job, runner),
        Platform::Systemd { unit_dir } => systemd::install(unit_dir, job, runner),
    }
}

/// Removes the schedule; when none is installed, that is reported, not an error.
pub fn uninstall(
    platform: &Platform,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Uninstalled> {
    match platform {
        Platform::Launchd { agents_dir, uid } => launchd::uninstall(agents_dir, *uid, runner),
        Platform::Systemd { unit_dir } => systemd::uninstall(unit_dir, runner),
    }
}

pub fn status(platform: &Platform, runner: &mut dyn CommandRunner) -> anyhow::Result<Status> {
    match platform {
        Platform::Launchd { agents_dir, uid } => launchd::status(agents_dir, *uid, runner),
        Platform::Systemd { unit_dir } => systemd::status(unit_dir, runner),
    }
}

/// What an installed schedule runs, read back from its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRun {
    /// The file it was read from: the plist, or the systemd service.
    pub file: PathBuf,
    /// The program and its arguments, as [`ScheduledRun::command_line`] wrote them.
    pub command_line: Vec<String>,
    /// The `PATH` each Run gets, when the file sets one.
    pub path: Option<String>,
}

impl InstalledRun {
    /// The binary each Run starts.
    pub fn binary(&self) -> Option<&str> {
        self.command_line.first().map(String::as_str)
    }

    /// The value after `flag`, such as the Profile after `--profile`.
    pub fn flag(&self, flag: &str) -> Option<&str> {
        self.command_line
            .iter()
            .position(|arg| arg == flag)
            .and_then(|i| self.command_line.get(i + 1))
            .map(String::as_str)
    }
}

/// Reads back what the installed schedule runs: `Ok(None)` when its file
/// isn't there, and an error when it can't be read or isn't one this binary
/// wrote.
pub fn installed_run(platform: &Platform) -> anyhow::Result<Option<InstalledRun>> {
    let file = match platform {
        Platform::Launchd { agents_dir, .. } => launchd::plist_path(agents_dir),
        Platform::Systemd { unit_dir } => unit_dir.join(systemd::SERVICE),
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("could not read {}", file.display())),
    };
    let parsed = match platform {
        Platform::Launchd { .. } => launchd::parse_plist(&text),
        Platform::Systemd { .. } => systemd::parse_service(&text),
    };
    let (command_line, path) = parsed.with_context(|| {
        format!(
            "{} has no command line hackathon-radar can read",
            file.display()
        )
    })?;
    Ok(Some(InstalledRun {
        file,
        command_line,
        path,
    }))
}

/// Writes a scheduler file readable by everyone and writable only by its owner,
/// as launchd requires.
fn write_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    std::fs::write(path, contents)
        .with_context(|| format!("could not write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
            .with_context(|| format!("could not set the permissions of {}", path.display()))?;
    }
    Ok(())
}

/// Removes a file, returning whether it was there.
fn remove_file(path: &Path) -> anyhow::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("could not remove {}", path.display())),
    }
}

/// Whether `binary` is a Cargo build output, which `cargo clean` or the next
/// build would remove or change under the schedule.
pub fn is_cargo_build_output(binary: &Path) -> bool {
    let parts: Vec<&std::ffi::OsStr> = binary.iter().collect();
    parts
        .windows(2)
        .any(|w| w[0] == "target" && (w[1] == "debug" || w[1] == "release"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_cargo_build_outputs() {
        assert!(is_cargo_build_output(Path::new(
            "/home/ada/hackathon-radar/target/debug/hackathon-radar"
        )));
        assert!(is_cargo_build_output(Path::new(
            "/home/ada/hackathon-radar/target/release/hackathon-radar"
        )));
        assert!(!is_cargo_build_output(Path::new(
            "/home/ada/.cargo/bin/hackathon-radar"
        )));
    }

    #[test]
    fn run_path_comes_from_the_installing_shell() {
        assert_eq!(
            RunPath::from_env(Some("/home/ada/.local/bin:/usr/bin".into())),
            RunPath {
                value: "/home/ada/.local/bin:/usr/bin".into(),
                defaulted: false,
            }
        );
    }

    #[test]
    fn an_unset_or_empty_path_falls_back_to_the_platform_default() {
        let expected = if cfg!(target_os = "macos") {
            "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin"
        } else {
            "/usr/local/bin:/usr/bin:/bin"
        };
        for from_env in [None, Some("".into()), Some("  ".into())] {
            assert_eq!(
                RunPath::from_env(from_env),
                RunPath {
                    value: expected.into(),
                    defaulted: true,
                }
            );
        }
    }

    #[test]
    fn scheduler_paths_must_be_absolute_and_on_one_line() {
        assert!(path_text(Path::new("relative/profile.toml")).is_err());
        assert!(path_text(Path::new("/home/ada/a\nb")).is_err());
        assert_eq!(
            path_text(Path::new("/home/ada/radar & co")).unwrap(),
            "/home/ada/radar & co"
        );
    }
}
