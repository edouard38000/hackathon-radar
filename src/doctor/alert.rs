//! The Alert channel: ntfy's topic and server, or Photon's Node, sidecar and
//! reminder; its credentials are in the Secrets section. With
//! `--send-test-alert`, one test Alert goes out the way `test-alert` sends it.

use std::time::Duration;

use super::{Check, FindProgram, Section, error_line, fetch_error};
use crate::alert::photon::{MIN_NODE_VERSION, NODE, SIDECAR_DIR_ENV_VAR, Sidecar};
use crate::http::{HttpGet, HttpPost};
use crate::init::Runner;
use crate::process::{Command, ProcessRunner};
use crate::profile::{AlertChannelSetting, NtfySetting, Profile};
use crate::secrets::Secrets;
use crate::wiring;

const S: Section = Section::AlertChannel;

/// How long `node --version` may take.
const NODE_VERSION_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct Context<'a> {
    pub shell_path: &'a str,
    pub find_program: FindProgram<'a>,
    pub runner: &'a dyn ProcessRunner,
    pub http: &'a dyn HttpGet,
    pub http_post: &'a dyn HttpPost,
    pub secrets: &'a Secrets,
    pub sidecar: &'a Sidecar,
    pub offline: bool,
    pub send_test_alert: bool,
}

pub(super) fn check(profile: &Profile, cx: &Context<'_>, checks: &mut Vec<Check>) {
    match &profile.alert_channel {
        None => checks.push(Check::warn(
            S,
            "alert.channel",
            "Alert channel",
            "No Alert channel is set, so Alerts are only printed and never reach you.",
            "Run `hackathon-radar init --alert-channel photon` (or ntfy) to set one up.",
        )),
        Some(AlertChannelSetting::Ntfy(setting)) => ntfy(setting, cx, checks),
        Some(AlertChannelSetting::Photon(_)) => photon(cx, checks),
    }
    if cx.send_test_alert {
        checks.push(test_alert(profile, cx));
    }
}

fn ntfy(setting: &NtfySetting, cx: &Context<'_>, checks: &mut Vec<Check>) {
    let server = &setting.server;
    // The topic stays out of the output: anyone who knows it can read the Alerts.
    checks.push(if setting.topic.trim().is_empty() {
        Check::fail(
            S,
            "alert.ntfy.topic",
            "ntfy topic",
            "No ntfy topic is set.",
            "Run `hackathon-radar init --alert-channel ntfy --ntfy-topic <TOPIC>`.",
        )
    } else {
        Check::pass(
            S,
            "alert.ntfy.topic",
            "ntfy topic",
            format!("Alerts go to your topic on {server} (the topic is left out here)."),
        )
    });
    if cx.offline {
        checks.push(Check::skip(
            S,
            "alert.ntfy.server",
            "ntfy server",
            "Skipped with --offline.",
        ));
        return;
    }
    let root = format!("{server}/");
    checks.push(match cx.http.get_text(&root) {
        Ok(_) => Check::pass(
            S,
            "alert.ntfy.server",
            "ntfy server",
            format!("{server} answered."),
        ),
        Err(error) => Check::fail(
            S,
            "alert.ntfy.server",
            "ntfy server",
            format!(
                "{server} didn't answer, so Alerts can't go out: {}.",
                fetch_error(&error, &root, false).replace(&setting.topic, "[topic]")
            ),
            "Check alert_channel.server in your Profile and your connection, then run doctor \
             again.",
        ),
    });
}

fn photon(cx: &Context<'_>, checks: &mut Vec<Check>) {
    checks.push(node(cx));

    let dir = cx.sidecar.dir.display();
    let script = cx.sidecar.script();
    if script.is_file() {
        checks.push(Check::pass(
            S,
            "alert.photon.sidecar",
            "Photon sidecar",
            format!("Found at {dir}."),
        ));
        let modules = cx.sidecar.dir.join("node_modules");
        checks.push(if modules.is_dir() {
            Check::pass(
                S,
                "alert.photon.dependencies",
                "Photon sidecar dependencies",
                format!("Installed in {}.", modules.display()),
            )
        } else {
            Check::fail(
                S,
                "alert.photon.dependencies",
                "Photon sidecar dependencies",
                format!("They aren't installed: there is no {}.", modules.display()),
                format!("Run `npm ci` in {dir}."),
            )
        });
    } else {
        checks.push(Check::fail(
            S,
            "alert.photon.sidecar",
            "Photon sidecar",
            format!(
                "The Photon sidecar isn't at {dir}: there is no {}.",
                script.display()
            ),
            format!(
                "Set {SIDECAR_DIR_ENV_VAR} to the sidecar/photon directory of your \
                 hackathon-radar checkout."
            ),
        ));
    }

    checks.push(
        Check::pass(
            S,
            "alert.photon.line",
            "Photon line",
            "Photon can only message your number after you have texted your Photon line from \
             it once.",
        )
        .fix("Text your Photon line once from your phone, if you haven't yet."),
    );
}

/// Node is on PATH and at least [`MIN_NODE_VERSION`].
fn node(cx: &Context<'_>) -> Check {
    const ID: &str = "alert.photon.node";
    const NAME: &str = "Node.js";
    let install = format!(
        "Install Node.js {MIN_NODE_VERSION} or later from https://nodejs.org; if Runs are \
         scheduled, run `hackathon-radar schedule install` again afterwards."
    );
    let Some(path) = (cx.find_program)(NODE, cx.shell_path) else {
        return Check::fail(
            S,
            ID,
            NAME,
            format!(
                "Photon Alerts need Node.js {MIN_NODE_VERSION} or later, and `node` isn't on your PATH."
            ),
            install,
        );
    };
    let output = cx.runner.run(&Command {
        program: path.to_string_lossy().into_owned(),
        args: vec!["--version".into()],
        stdin: String::new(),
        working_dir: None,
        timeout: NODE_VERSION_TIMEOUT,
        env: Vec::new(),
    });
    let version = match &output {
        Ok(output) if output.success() => parse_version(&output.stdout),
        _ => None,
    };
    let minimum = parse_version(MIN_NODE_VERSION).expect("the minimum is a version");
    match version {
        Some(version) if version >= minimum => Check::pass(
            S,
            ID,
            NAME,
            format!("Node.js {} at {}.", show(version), path.display()),
        ),
        Some(version) => Check::fail(
            S,
            ID,
            NAME,
            format!(
                "Node.js {} at {} is older than {MIN_NODE_VERSION}, which the Photon sidecar needs.",
                show(version),
                path.display()
            ),
            install,
        ),
        None => Check::warn(
            S,
            ID,
            NAME,
            format!(
                "Couldn't tell which Node.js {} is: `node --version` gave no version.",
                path.display()
            ),
            format!("Check that `node --version` prints {MIN_NODE_VERSION} or later."),
        ),
    }
}

type Version = (u32, u32, u32);

/// `v20.18.1` (or `20.18`) as numbers.
fn parse_version(text: &str) -> Option<Version> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.split('.').map(|p| p.trim().parse::<u32>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

fn show((major, minor, patch): Version) -> String {
    format!("{major}.{minor}.{patch}")
}

/// Sends one test Alert through the existing test-alert path.
fn test_alert(profile: &Profile, cx: &Context<'_>) -> Check {
    const ID: &str = "alert.test";
    const NAME: &str = "Test Alert";
    let name = wiring::channel_name(profile);
    let sent = wiring::alert_channel(
        profile,
        cx.secrets,
        false,
        cx.http_post,
        Runner(cx.runner),
        cx.sidecar.clone(),
    )
    .and_then(|mut channel| wiring::send_test_alert(channel.as_mut()));
    match sent {
        Ok(()) => Check::pass(
            S,
            ID,
            NAME,
            format!("Sent a test Alert through {name}. Check your phone."),
        ),
        Err(error) => Check::fail(
            S,
            ID,
            NAME,
            format!(
                "The test Alert through {name} wasn't sent: {}",
                error_line(&error, ntfy_topic(profile))
            ),
            "Fix the cause, then run `hackathon-radar test-alert` (or doctor --send-test-alert).",
        ),
    }
}

fn ntfy_topic(profile: &Profile) -> Option<&str> {
    match &profile.alert_channel {
        Some(AlertChannelSetting::Ntfy(setting)) => Some(&setting.topic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_node_versions() {
        assert_eq!(parse_version("v20.18.1\n"), Some((20, 18, 1)));
        assert_eq!(parse_version("22.3"), Some((22, 3, 0)));
        assert_eq!(parse_version("node"), None);
        assert!(parse_version("v20.18.0").unwrap() < parse_version(MIN_NODE_VERSION).unwrap());
        assert!(parse_version("v20.19.0").unwrap() > parse_version(MIN_NODE_VERSION).unwrap());
    }
}
