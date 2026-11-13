//! The schedule: installed or not, and if it is, whether the launchd plist or
//! systemd service starts this binary with the same Profile and database, and
//! a PATH that finds the programs a Run starts. On Linux, whether linger keeps
//! user timers going while the person is logged out.

use std::path::{Path, PathBuf};

use super::{Check, FindProgram, Section, error_line};
use crate::scheduler::{self, CommandRunner, InstalledRun, Platform};

const S: Section = Section::Schedule;

const REINSTALL: &str =
    "Run `hackathon-radar schedule install` from the binary and shell you use now.";

pub(super) struct Context<'a> {
    pub platform: &'a Result<Platform, String>,
    pub runner: &'a mut dyn CommandRunner,
    pub binary: &'a Path,
    pub profile_path: &'a Path,
    pub db_path: &'a Path,
    /// The programs a Run starts, which the schedule's PATH must find.
    pub programs: Vec<String>,
    pub find_program: FindProgram<'a>,
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

/// Returns whether a schedule is installed, when that could be told.
pub(super) fn check(cx: Context<'_>, checks: &mut Vec<Check>) -> Option<bool> {
    let platform = match cx.platform {
        Ok(platform) => platform,
        Err(why) => {
            checks.push(Check::skip(
                S,
                "schedule",
                "Schedule",
                format!("Not checked: {why}."),
            ));
            return None;
        }
    };
    let status = match scheduler::status(platform, cx.runner) {
        Ok(status) => status,
        Err(error) => {
            checks.push(Check::warn(
                S,
                "schedule.installed",
                "Schedule",
                format!("Couldn't check the schedule: {}.", error_line(&error, None)),
                "Run `hackathon-radar schedule status` to see what's wrong.",
            ));
            return None;
        }
    };
    let file = status
        .files
        .first()
        .map(|f| f.display().to_string())
        .unwrap_or_default();
    let state = match (status.installed, status.active) {
        (false, false) => {
            checks.push(Check::warn(
                S,
                "schedule.installed",
                "Schedule",
                "No schedule is installed, so Runs only happen when you start them.",
                "Run `hackathon-radar schedule install`.",
            ));
            return Some(false);
        }
        (true, true) => Check::pass(
            S,
            "schedule.installed",
            "Schedule",
            format!("Installed and active ({file})."),
        ),
        (true, false) => Check::fail(
            S,
            "schedule.installed",
            "Schedule",
            format!("Installed ({file}) but not active, so scheduled Runs don't start."),
            "Run `hackathon-radar schedule install` to reload it.",
        ),
        (false, true) => Check::fail(
            S,
            "schedule.installed",
            "Schedule",
            format!("Active, but its files are missing ({file})."),
            "Run `hackathon-radar schedule install` to repair it.",
        ),
    };
    checks.push(state);

    match scheduler::installed_run(platform) {
        Ok(Some(run)) => installed(&run, &cx, checks),
        Ok(None) => {}
        Err(error) => checks.push(Check::fail(
            S,
            "schedule.file",
            "Schedule file",
            format!("{}.", error_line(&error, None)),
            REINSTALL,
        )),
    }

    if let Platform::Systemd { .. } = platform {
        checks.push(linger(cx.runner, cx.env));
    }
    Some(true)
}

/// Whether the installed Run matches this binary, Profile, database and PATH.
fn installed(run: &InstalledRun, cx: &Context<'_>, checks: &mut Vec<Check>) {
    let file = run.file.display();
    let binary = run.binary().unwrap_or_default();
    let current = cx.binary.display();
    checks.push(if !Path::new(binary).is_file() {
        Check::fail(
            S,
            "schedule.binary",
            "Scheduled binary",
            format!("Scheduled Runs start {binary}, which no longer exists, so they fail."),
            REINSTALL,
        )
    } else if !same_file(Path::new(binary), cx.binary) {
        Check::warn(
            S,
            "schedule.binary",
            "Scheduled binary",
            format!("Scheduled Runs start {binary}, not this binary ({current})."),
            REINSTALL,
        )
    } else if scheduler::is_cargo_build_output(Path::new(binary)) {
        Check::warn(
            S,
            "schedule.binary",
            "Scheduled binary",
            format!(
                "Scheduled Runs start {binary}, a Cargo build output that `cargo clean` removes."
            ),
            "Install the binary (`cargo install --path .`) and run `hackathon-radar schedule \
             install` from it.",
        )
    } else {
        Check::pass(
            S,
            "schedule.binary",
            "Scheduled binary",
            format!("Scheduled Runs start this binary ({binary})."),
        )
    });

    for (id, name, flag, here, what) in [
        (
            "schedule.profile",
            "Scheduled Profile",
            "--profile",
            cx.profile_path,
            "Profile",
        ),
        (
            "schedule.db",
            "Scheduled database",
            "--db",
            cx.db_path,
            "database",
        ),
    ] {
        let here = absolute(here);
        checks.push(match run.flag(flag) {
            None => Check::fail(
                S,
                id,
                name,
                format!("{file} doesn't say which {what} Runs use."),
                REINSTALL,
            ),
            Some(scheduled) if same_file(Path::new(scheduled), &here) => Check::pass(
                S,
                id,
                name,
                format!("Scheduled Runs use this {what} ({scheduled})."),
            ),
            Some(scheduled) => Check::warn(
                S,
                id,
                name,
                format!(
                    "Scheduled Runs use the {what} at {scheduled}, not {}, which doctor checked.",
                    here.display()
                ),
                format!(
                    "Run `hackathon-radar schedule install {flag} {}` to schedule this one, or \
                     run doctor with {flag} {scheduled}.",
                    here.display()
                ),
            ),
        });
    }

    if cx.programs.is_empty() {
        return;
    }
    let listed = cx.programs.join(" and ");
    checks.push(match &run.path {
        None => Check::fail(
            S,
            "schedule.path",
            "Scheduled PATH",
            format!("{file} sets no PATH, so scheduled Runs may not find {listed}."),
            REINSTALL,
        ),
        Some(path) => {
            let missing: Vec<&str> = cx
                .programs
                .iter()
                .filter(|p| (cx.find_program)(p, path).is_none())
                .map(String::as_str)
                .collect();
            if missing.is_empty() {
                Check::pass(
                    S,
                    "schedule.path",
                    "Scheduled PATH",
                    format!("Scheduled Runs' PATH finds {listed}."),
                )
            } else {
                let missing = missing.join(" and ");
                Check::fail(
                    S,
                    "schedule.path",
                    "Scheduled PATH",
                    format!("Scheduled Runs' PATH doesn't find {missing}, so they would fail."),
                    format!(
                        "Run `hackathon-radar schedule install` from a shell where {missing} is \
                         on your PATH; it saves that shell's PATH for Runs."
                    ),
                )
            }
        }
    });
}

/// systemd only runs user timers while the person is logged in, unless
/// linger is on.
fn linger(runner: &mut dyn CommandRunner, env: &dyn Fn(&str) -> Option<String>) -> Check {
    const ID: &str = "schedule.linger";
    const NAME: &str = "Linger";
    const FIX: &str = "Run `loginctl enable-linger` so Runs go on while you're logged out.";
    let Some(user) = env("USER").filter(|u| !u.is_empty()) else {
        return Check::warn(
            S,
            ID,
            NAME,
            "Couldn't tell whether linger is on: USER is not set.",
            FIX,
        );
    };
    let output = runner.run(
        "loginctl",
        &[
            "show-user".into(),
            user,
            "--property=Linger".into(),
            "--value".into(),
        ],
    );
    match output {
        Ok(output) if output.success && output.stdout.trim() == "yes" => Check::pass(
            S,
            ID,
            NAME,
            "Linger is on, so Runs go on while you're logged out.",
        ),
        Ok(output) if output.success && output.stdout.trim() == "no" => Check::warn(
            S,
            ID,
            NAME,
            "systemd only runs your timers while you're logged in, so Runs stop when you log out.",
            FIX,
        ),
        _ => Check::warn(
            S,
            ID,
            NAME,
            "Couldn't tell whether linger is on (`loginctl show-user` gave no answer).",
            FIX,
        ),
    }
}

/// The same file, comparing resolved paths when both exist.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}
