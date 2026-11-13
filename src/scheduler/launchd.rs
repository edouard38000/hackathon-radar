//! macOS: a LaunchAgent plist in `~/Library/LaunchAgents`, loaded with
//! `launchctl bootstrap gui/<uid>` and removed with `launchctl bootout`.
//!
//! launchd runs a calendar job missed while the Mac slept when it wakes, but not
//! one missed while it was shut down.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use chrono::{Timelike as _, Weekday};

use super::{
    CommandRunner, Installed, LABEL, ScheduledRun, Status, Uninstalled, remove_file, run_checked,
    succeeds, write_file,
};

pub fn plist_path(agents_dir: &Path) -> PathBuf {
    agents_dir.join(format!("{LABEL}.plist"))
}

/// The LaunchAgent plist: one `StartCalendarInterval` entry per scheduled day.
pub fn render_plist(job: &ScheduledRun) -> anyhow::Result<String> {
    let log = xml_escape(&job.log_text()?);
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n",
    );
    let _ = writeln!(out, "\t<key>Label</key>\n\t<string>{LABEL}</string>");
    out.push_str("\t<key>ProgramArguments</key>\n\t<array>\n");
    for arg in job.command_line()? {
        let _ = writeln!(out, "\t\t<string>{}</string>", xml_escape(&arg));
    }
    out.push_str("\t</array>\n");
    let _ = writeln!(
        out,
        "\t<key>EnvironmentVariables</key>\n\t<dict>\n\
         \t\t<key>PATH</key>\n\t\t<string>{}</string>\n\
         \t</dict>",
        xml_escape(job.path_env()?)
    );
    out.push_str("\t<key>StartCalendarInterval</key>\n\t<array>\n");
    for day in &job.schedule.days {
        let _ = writeln!(
            out,
            "\t\t<dict>\n\
             \t\t\t<key>Weekday</key>\n\t\t\t<integer>{}</integer>\n\
             \t\t\t<key>Hour</key>\n\t\t\t<integer>{}</integer>\n\
             \t\t\t<key>Minute</key>\n\t\t\t<integer>{}</integer>\n\
             \t\t</dict>",
            launchd_weekday(*day),
            job.schedule.time.hour(),
            job.schedule.time.minute(),
        );
    }
    out.push_str("\t</array>\n");
    let _ = writeln!(
        out,
        "\t<key>StandardOutPath</key>\n\t<string>{log}</string>\n\
         \t<key>StandardErrorPath</key>\n\t<string>{log}</string>"
    );
    out.push_str("</dict>\n</plist>\n");
    Ok(out)
}

/// The command line and `PATH` of a plist [`render_plist`] wrote, or `None`
/// when it has no `ProgramArguments`.
pub fn parse_plist(text: &str) -> Option<(Vec<String>, Option<String>)> {
    let after = |key: &str| {
        text.find(&format!("<key>{key}</key>"))
            .map(|at| &text[at + key.len() + 11..])
    };
    let args = after("ProgramArguments")?;
    let args = &args[..args.find("</array>")?];
    let command_line: Vec<String> = strings(args).collect();
    if command_line.is_empty() {
        return None;
    }
    let path = after("PATH").and_then(|rest| strings(rest).next());
    Some((command_line, path))
}

/// The `<string>` values in `text`, unescaped.
fn strings(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split("<string>")
        .skip(1)
        .filter_map(|piece| piece.split_once("</string>"))
        .map(|(value, _)| xml_unescape(value))
}

fn xml_unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// launchd numbers days from Sunday = 0.
fn launchd_weekday(day: Weekday) -> u32 {
    day.num_days_from_sunday()
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn domain(uid: u32) -> String {
    format!("gui/{uid}")
}

fn service_target(uid: u32) -> String {
    format!("gui/{uid}/{LABEL}")
}

fn is_loaded(runner: &mut dyn CommandRunner, uid: u32) -> anyhow::Result<bool> {
    succeeds(runner, "launchctl", &["print".into(), service_target(uid)])
}

fn bootout(runner: &mut dyn CommandRunner, uid: u32) -> anyhow::Result<()> {
    run_checked(
        runner,
        "launchctl",
        &["bootout".into(), service_target(uid)],
    )
}

pub(super) fn install(
    agents_dir: &Path,
    uid: u32,
    job: &ScheduledRun,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Installed> {
    let plist = render_plist(job)?;
    let path = plist_path(agents_dir);
    let existed = path.exists();
    std::fs::create_dir_all(agents_dir)
        .with_context(|| format!("could not create {}", agents_dir.display()))?;
    write_file(&path, &plist)?;

    // Reload so launchd reads the new plist: bootstrap refuses a loaded label.
    let loaded = is_loaded(runner, uid)?;
    if loaded {
        bootout(runner, uid)?;
    }
    let path_text = path.to_string_lossy().into_owned();
    run_checked(
        runner,
        "launchctl",
        &["bootstrap".into(), domain(uid), path_text],
    )?;
    Ok(Installed {
        replaced: existed || loaded,
        files: vec![path],
    })
}

pub(super) fn uninstall(
    agents_dir: &Path,
    uid: u32,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Uninstalled> {
    let path = plist_path(agents_dir);
    let loaded = is_loaded(runner, uid)?;
    if loaded {
        bootout(runner, uid)?;
    }
    let removed = remove_file(&path)?;
    Ok(if loaded || removed {
        Uninstalled::Removed { files: vec![path] }
    } else {
        Uninstalled::NotInstalled
    })
}

pub(super) fn status(
    agents_dir: &Path,
    uid: u32,
    runner: &mut dyn CommandRunner,
) -> anyhow::Result<Status> {
    let path = plist_path(agents_dir);
    Ok(Status {
        installed: path.exists(),
        active: is_loaded(runner, uid)?,
        files: vec![path],
    })
}
