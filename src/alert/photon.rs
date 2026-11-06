//! The Photon Alert channel: an iMessage from the person's own Photon line.
//!
//! Photon only ships a TypeScript SDK, so the Alert goes through the small Node
//! sidecar in `sidecar/photon/` (ADR 0005), the one part of hackathon-radar
//! that isn't Rust. Each Alert starts the sidecar once: `node send.mjs`, with
//! the recipient and text as JSON on stdin and the project credentials in its
//! environment. It sends one message, prints one line of JSON and exits.

use std::path::PathBuf;
use std::time::Duration;
use std::{fmt, io};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::alert::{Alert, AlertChannel};
use crate::process::{Command, Output, ProcessRunner};
use crate::profile::PhotonSetting;

/// The Photon project ID's name: the environment variable that overrides the
/// one in the keychain ([`crate::secrets`]), and the variable the sidecar reads.
pub const PROJECT_ID_ENV_VAR: &str = "PHOTON_PROJECT_ID";
/// The Photon project secret's name, as for [`PROJECT_ID_ENV_VAR`].
pub const PROJECT_SECRET_ENV_VAR: &str = "PHOTON_PROJECT_SECRET";
/// Points at another copy of the sidecar directory than the bundled one.
pub const SIDECAR_DIR_ENV_VAR: &str = "HACKATHON_RADAR_PHOTON_SIDECAR_DIR";

/// The program that runs the sidecar.
pub const NODE: &str = "node";
/// The sidecar's entry point, inside its directory.
pub const SCRIPT: &str = "send.mjs";
/// The oldest Node the sidecar's dependencies support.
pub const MIN_NODE_VERSION: &str = "20.18.1";
/// How long one send may take before the sidecar is stopped. The sidecar gives
/// up on its own after 45 seconds, so this only catches a sidecar that hangs.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// Where a person signs up and finds their project's ID and secret.
pub const DASHBOARD: &str = "https://app.photon.codes";

/// A Photon project's ID and secret. `Debug` never shows the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    project_id: String,
    project_secret: String,
}

impl Credentials {
    pub fn new(project_id: impl Into<String>, project_secret: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            project_secret: project_secret.into(),
        }
    }

    /// The credentials from these values of the two secrets, as the secret
    /// store ([`crate::secrets`]) found them, or an error naming each one that
    /// is missing or empty.
    pub fn from_vars(id: Option<String>, secret: Option<String>) -> anyhow::Result<Self> {
        let id = id.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let secret = secret
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        match (id, secret) {
            (Some(id), Some(secret)) => Ok(Self::new(id, secret)),
            (id, secret) => {
                let missing: Vec<&str> = [
                    id.is_none().then_some(PROJECT_ID_ENV_VAR),
                    secret.is_none().then_some(PROJECT_SECRET_ENV_VAR),
                ]
                .into_iter()
                .flatten()
                .collect();
                anyhow::bail!(
                    "Photon Alerts need your Photon project credentials, but {} {} not set. \
                     Run `hackathon-radar init` to store them in your keychain, or set \
                     {PROJECT_ID_ENV_VAR} and {PROJECT_SECRET_ENV_VAR} in the environment; \
                     your project's ID and secret are in the Photon dashboard ({DASHBOARD}). \
                     They never go in the Profile.",
                    missing.join(" and "),
                    if missing.len() == 1 { "is" } else { "are" }
                )
            }
        }
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("project_id", &self.project_id)
            .field("project_secret", &"[redacted]")
            .finish()
    }
}

/// Where the sidecar lives: a directory holding [`SCRIPT`] and, once `npm ci`
/// has run there, its `node_modules`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    pub dir: PathBuf,
}

impl Sidecar {
    /// The copy in this source tree, which is where `cargo install --path .`
    /// and `cargo run` find it.
    pub fn bundled() -> Self {
        Self {
            dir: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/sidecar/photon")),
        }
    }

    /// [`SIDECAR_DIR_ENV_VAR`] when it is set, or else the bundled copy.
    pub fn locate() -> Self {
        match std::env::var_os(SIDECAR_DIR_ENV_VAR) {
            Some(dir) if !dir.is_empty() => Self { dir: dir.into() },
            _ => Self::bundled(),
        }
    }

    pub fn script(&self) -> PathBuf {
        self.dir.join(SCRIPT)
    }
}

/// What the sidecar reads on stdin.
#[derive(Serialize)]
struct SidecarRequest<'a> {
    to: &'a str,
    text: &'a str,
}

/// The one line of JSON the sidecar prints on stdout.
#[derive(Deserialize)]
struct SidecarResult {
    ok: bool,
    error: Option<String>,
    message: Option<String>,
}

pub struct PhotonChannel<R> {
    setting: PhotonSetting,
    /// The credentials, or why there are none.
    credentials: Result<Credentials, String>,
    sidecar: Sidecar,
    runner: R,
}

impl<R: ProcessRunner> PhotonChannel<R> {
    /// With `Err` credentials, every send fails with that error, which says how
    /// to set them (see [`Credentials::from_vars`]).
    pub fn new(
        setting: PhotonSetting,
        credentials: Result<Credentials, String>,
        sidecar: Sidecar,
        runner: R,
    ) -> Self {
        Self {
            setting,
            credentials,
            sidecar,
            runner,
        }
    }

    /// The sidecar run that sends this Alert.
    pub fn command(&self, alert: &Alert, credentials: &Credentials) -> Command {
        let request = SidecarRequest {
            to: &self.setting.to,
            text: &alert.text,
        };
        Command {
            program: NODE.to_owned(),
            args: vec![self.sidecar.script().to_string_lossy().into_owned()],
            stdin: serde_json::to_string(&request).expect("the request serialises"),
            working_dir: None,
            timeout: TIMEOUT,
            env: vec![
                (
                    PROJECT_ID_ENV_VAR.to_owned(),
                    credentials.project_id.clone(),
                ),
                (
                    PROJECT_SECRET_ENV_VAR.to_owned(),
                    credentials.project_secret.clone(),
                ),
            ],
        }
    }

    /// Turns the sidecar's output into the outcome of the send, with an error
    /// that says what to do for each failure the person can fix.
    fn outcome(&self, output: &Output, secret: &str) -> anyhow::Result<()> {
        let result = output
            .stdout
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| serde_json::from_str::<SidecarResult>(line).ok());
        let Some(result) = result else {
            return Err(self.no_result(output, secret));
        };
        if result.ok && output.success() {
            return Ok(());
        }
        let detail = scrub(result.message.as_deref().unwrap_or_default(), secret);
        let dir = self.sidecar.dir.display();
        match result.error.as_deref().unwrap_or_default() {
            "not_texted_yet" => anyhow::bail!(
                "Photon won't message {to} yet: a shared Photon line can only text a number \
                 that has texted it first. Send any message to your Photon line from {to}, \
                 then run again.",
                to = self.setting.to
            ),
            "bad_credentials" => anyhow::bail!(
                "Photon refused your project credentials ({detail}). Check your project's ID and \
                 secret in the Photon dashboard ({DASHBOARD}) and store them again with \
                 `hackathon-radar init` (or {PROJECT_ID_ENV_VAR} and {PROJECT_SECRET_ENV_VAR}, if \
                 you set them in the environment)."
            ),
            "missing_credentials" => anyhow::bail!(
                "the Photon sidecar got no project credentials ({detail}); run \
                 `hackathon-radar init` to store them"
            ),
            "sdk_missing" => anyhow::bail!(
                "the Photon sidecar's dependencies aren't installed. Run `npm ci` once in {dir} \
                 (it needs Node.js {MIN_NODE_VERSION} or later)."
            ),
            "node_too_old" => anyhow::bail!(
                "the Photon sidecar needs Node.js {MIN_NODE_VERSION} or later ({detail}); \
                 install a newer Node from https://nodejs.org"
            ),
            "rate_limited" => anyhow::bail!(
                "Photon is limiting how often your line sends ({detail}); the Alert goes out \
                 on the next Run"
            ),
            "timeout" => anyhow::bail!(
                "Photon didn't answer in time ({detail}); the Alert goes out on the next Run"
            ),
            code => anyhow::bail!(
                "Photon could not send the Alert ({}): {detail}",
                if code.is_empty() {
                    "no reason given"
                } else {
                    code
                }
            ),
        }
    }

    /// The error for a sidecar that printed no result, which means Node
    /// couldn't run it at all.
    fn no_result(&self, output: &Output, secret: &str) -> anyhow::Error {
        let stderr = scrub(&output.stderr, secret);
        if stderr.contains("Cannot find module") && stderr.contains(SCRIPT) {
            return anyhow::anyhow!(
                "the Photon sidecar isn't at {}. Set {SIDECAR_DIR_ENV_VAR} to the sidecar/photon \
                 directory of your hackathon-radar checkout.",
                self.sidecar.script().display()
            );
        }
        let exit = match output.code {
            Some(code) => format!("exit code {code}"),
            None => "stopped by a signal".to_owned(),
        };
        anyhow::anyhow!(
            "the Photon sidecar gave no result ({exit}): {}",
            tail(stderr.trim())
        )
    }
}

impl<R: ProcessRunner> AlertChannel for PhotonChannel<R> {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()> {
        let credentials = match &self.credentials {
            Ok(credentials) => credentials,
            Err(why) => anyhow::bail!("{why}"),
        };
        let command = self.command(alert, credentials);
        let output = match self.runner.run(&command) {
            Ok(output) => output,
            Err(error) if is_not_found(&error) => anyhow::bail!(
                "Photon Alerts need Node.js {MIN_NODE_VERSION} or later, and `{NODE}` isn't on \
                 your PATH. Install Node from https://nodejs.org; if Runs are scheduled, run \
                 `hackathon-radar schedule install` again afterwards so they find it."
            ),
            Err(error) => return Err(error).context("sending the Alert through Photon"),
        };
        self.outcome(&output, &credentials.project_secret)
    }
}

/// Whether the program couldn't start because it doesn't exist.
fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == io::ErrorKind::NotFound)
    })
}

/// The text with every copy of the secret taken out.
fn scrub(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_owned()
    } else {
        text.replace(secret, "[redacted]")
    }
}

/// The end of a long error output, which is where the reason usually is.
fn tail(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 500 {
        return text.to_owned();
    }
    format!("…{}", chars[chars.len() - 500..].iter().collect::<String>())
}
