//! Linux: a systemd user service and timer in `~/.config/systemd/user`, enabled
//! with `systemctl --user enable --now` and disabled on uninstall.
//!
//! The timer is `Persistent=true`, so a Run missed while the machine was off or
//! asleep happens when it is next up.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use chrono::Weekday;

use super::{
    CommandRunner, Installed, ScheduledRun, Status, Uninstalled, remove_file, run_checked,
    succeeds, write_file,
};
use crate::profile::Schedule;

pub const SERVICE: &str = "hackathon-radar.service";
pub const TIMER: &str = "hackathon-radar.timer";

/// The oneshot service that performs one Run and appends its output to the log.
pub fn render_service(job: &ScheduledRun) -> anyhow::Result<String> {
    let exec: Vec<String> = job.command_line()?.iter().map(|a| exec_arg(a)).collect();
    let log = escape_specifiers(&job.log_text()?);
    let path = environment_assignment("PATH", job.path_env()?);
    Ok(format!(
        "[Unit]\n\
         Description=hackathon-radar Run\n\
         Documentation=https://github.com/edouard38000/hackathon-radar\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         Environment={path}\n\
         ExecStart={}\n\
         StandardOutput=append:{log}\n\
         StandardError=append:{log}\n",
        exec.join(" ")
    ))
}

/// The timer that starts the service on the scheduled days, in local time.
pub fn render_timer(schedule: &Schedule) -> String {
    format!(
        "[Unit]\n\
         Description=Scheduled hackathon-radar Runs ({schedule})\n\
         \n\
         [Timer]\n\
         OnCalendar={}\n\
         Persistent=true\n\
         Unit={SERVICE}\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n",
        on_calendar(schedule)
    )
}

/// Such as `Mon,Thu *-*-* 08:00:00`.
pub fn on_calendar(schedule: &Schedule) -> String {
    let days: Vec<&str> = schedule.days.iter().map(|d| short_day(*d)).collect();
    format!(
        "{} *-*-* {}",
        days.join(","),
        schedule.time.format("%H:%M:00")
    )
}

fn short_day(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// systemd expands `%` specifiers in most settings; `%%` is a literal `%`.
fn escape_specifiers(text: &str) -> String {
    text.replace('%', "%%")
}

/// One quoted `Environment=` assignment, such as `"PATH=/usr/bin:/bin"`. systemd
/// expands specifiers there but not `$` variables, so only `%` is doubled.
fn environment_assignment(name: &str, value: &str) -> String {
    let assignment = escape_specifiers(&format!("{name}={value}"));
    format!(
        "\"{}\"",
        assignment.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// One `ExecStart=` word: specifiers and `$` variables escaped, and quoted when
/// it holds anything systemd would otherwise split or interpret.
fn exec_arg(arg: &str) -> String {
    let needs_quotes = arg.is_empty()
        || arg
            .chars()
            .any(|c| c.is_whitespace() || "\"'\\;".contains(c));
    let arg = escape_specifiers(arg).replace('$', "$$");
    if needs_quotes {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg
    }
}

fn unit_paths(unit_dir: &Path) -> [PathBuf; 2] {
    [unit_dir.join(SERVICE), unit_dir.join(TIMER)]
}

fn systemctl(runner: &mut dyn CommandRunner, args: &[&str]) -> anyhow::Result<()> {
    let args: Vec<String> = std::iter::once("--user")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    run_checked(runner, "systemctl", &args)
}

pub(super) fn install(
    unit_dir: &Path,
    job: &ScheduledRun,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Installed> {
    let service = render_service(job)?;
    let timer = render_timer(&job.schedule);
    let [service_path, timer_path] = unit_paths(unit_dir);
    let existed = service_path.exists() || timer_path.exists();
    std::fs::create_dir_all(unit_dir)
        .with_context(|| format!("could not create {}", unit_dir.display()))?;
    write_file(&service_path, &service)?;
    write_file(&timer_path, &timer)?;

    systemctl(runner, &["daemon-reload"])?;
    systemctl(runner, &["enable", "--now", TIMER])?;
    if existed {
        // An active timer keeps its old trigger until it is restarted.
        systemctl(runner, &["restart", TIMER])?;
    }
    Ok(Installed {
        replaced: existed,
        files: vec![service_path, timer_path],
    })
}

pub(super) fn uninstall(
    unit_dir: &Path,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Uninstalled> {
    let [service_path, timer_path] = unit_paths(unit_dir);
    if !service_path.exists() && !timer_path.exists() {
        return Ok(Uninstalled::NotInstalled);
    }
    if timer_path.exists() {
        systemctl(runner, &["disable", "--now", TIMER])?;
    }
    remove_file(&timer_path)?;
    remove_file(&service_path)?;
    systemctl(runner, &["daemon-reload"])?;
    Ok(Uninstalled::Removed {
        files: vec![service_path, timer_path],
    })
}

pub(super) fn status(unit_dir: &Path, runner: &mut dyn CommandRunner) -> anyhow::Result<Status> {
    let [service_path, timer_path] = unit_paths(unit_dir);
    let installed = service_path.exists() && timer_path.exists();
    let active = succeeds(
        runner,
        "systemctl",
        &[
            "--user".into(),
            "is-active".into(),
            "--quiet".into(),
            TIMER.into(),
        ],
    )?;
    Ok(Status {
        installed,
        active,
        files: vec![service_path, timer_path],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_words_are_quoted_and_escaped() {
        assert_eq!(
            exec_arg("/usr/bin/hackathon-radar"),
            "/usr/bin/hackathon-radar"
        );
        assert_eq!(exec_arg("/home/ada/my radar"), "\"/home/ada/my radar\"");
        assert_eq!(exec_arg("/home/ada/100%"), "/home/ada/100%%");
        assert_eq!(exec_arg("/home/ada/$HOME"), "/home/ada/$$HOME");
        assert_eq!(exec_arg("/a\"b\\c"), "\"/a\\\"b\\\\c\"");
        assert_eq!(exec_arg("/a;b"), "\"/a;b\"");
    }

    #[test]
    fn environment_assignments_are_quoted_and_escaped() {
        assert_eq!(
            environment_assignment("PATH", "/usr/bin:/bin"),
            "\"PATH=/usr/bin:/bin\""
        );
        assert_eq!(
            environment_assignment("PATH", "/my bin/100%:/$x:/a\"b\\c"),
            "\"PATH=/my bin/100%%:/$x:/a\\\"b\\\\c\""
        );
    }
}
