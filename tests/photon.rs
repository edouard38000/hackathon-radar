//! Seam 2: the Photon Alert channel against the sidecar's contract, with a fake
//! process runner instead of Node. The sidecar outputs in
//! `tests/fixtures/photon/` are the ones the sidecar's own tests produce from a
//! fake Photon SDK (`sidecar/photon/test/`), so both sides check one recording.
//! Plus seam 1: whole Runs that send through the Photon channel.

mod support;

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use hackathon_radar::alert::photon::{self, Credentials, PhotonChannel, Sidecar};
use hackathon_radar::alert::{Alert, AlertChannel};
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::PhotonSetting;
use support::*;

const SECRET: &str = "secret_fixture_not_real";
const TO: &str = "+32470123456";

/// What the fake runner does when asked to run the sidecar.
enum Reply {
    /// The sidecar ran and printed this.
    Printed(Output),
    /// The program couldn't start because it isn't installed, as the real
    /// runner reports it.
    NotInstalled,
}

/// Records each command and answers with a canned reply instead of running it.
struct FakeRunner {
    reply: Reply,
    commands: RefCell<Vec<Command>>,
}

impl FakeRunner {
    fn printing(stdout: &str, code: i32) -> Self {
        Self::stdout_stderr(stdout, "", code)
    }

    fn stdout_stderr(stdout: &str, stderr: &str, code: i32) -> Self {
        Self {
            reply: Reply::Printed(Output {
                code: Some(code),
                stdout: stdout.to_owned(),
                stderr: stderr.to_owned(),
            }),
            commands: RefCell::new(Vec::new()),
        }
    }

    /// Answers with a recorded sidecar result; failures exit 1, as the sidecar does.
    fn answering(fixture_name: &str) -> Self {
        let stdout = fixture(&format!("photon/{fixture_name}"));
        let code = if stdout.contains(r#""ok":true"#) {
            0
        } else {
            1
        };
        Self::printing(&stdout, code)
    }

    fn not_installed() -> Self {
        Self {
            reply: Reply::NotInstalled,
            commands: RefCell::new(Vec::new()),
        }
    }

    fn runs(&self) -> usize {
        self.commands.borrow().len()
    }
}

impl ProcessRunner for &FakeRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
        match &self.reply {
            Reply::Printed(output) => Ok(output.clone()),
            Reply::NotInstalled => {
                Err(io::Error::from(io::ErrorKind::NotFound)).with_context(|| {
                    format!(
                        "could not start `{}`; is it installed and on your PATH?",
                        command.program
                    )
                })
            }
        }
    }
}

fn sidecar() -> Sidecar {
    Sidecar {
        dir: PathBuf::from("/opt/hackathon-radar/sidecar/photon"),
    }
}

fn channel(runner: &FakeRunner) -> PhotonChannel<&FakeRunner> {
    PhotonChannel::new(
        PhotonSetting { to: TO.into() },
        Ok(Credentials::new("proj_fixture", SECRET)),
        sidecar(),
        runner,
    )
}

fn alert() -> Alert {
    Alert {
        text: "hackathon-radar: 1 Hackathon newly fits you\n\nMystery Hack\n1 Dec 2026".into(),
    }
}

/// The error a send fails with, checked to never hold the secret.
fn send_error(runner: &FakeRunner) -> String {
    let error = channel(runner).send(&alert()).unwrap_err();
    let message = format!("{error:#}");
    assert!(!message.contains(SECRET), "{message}");
    message
}

#[test]
fn runs_the_sidecar_with_node_and_the_alert_on_stdin() {
    let runner = FakeRunner::answering("sent.json");

    channel(&runner).send(&alert()).unwrap();

    let commands = runner.commands.borrow();
    let [command] = &commands[..] else {
        panic!("one Alert, one sidecar run: {commands:?}");
    };
    assert_eq!(command.program, "node");
    assert_eq!(
        command.args,
        ["/opt/hackathon-radar/sidecar/photon/send.mjs"]
    );
    assert_eq!(command.stdin, fixture("photon/request.json").trim_end());
    let names: Vec<&str> = command.env.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["PHOTON_PROJECT_ID", "PHOTON_PROJECT_SECRET"]);
    assert_eq!(command.working_dir, None);
    assert_eq!(command.timeout, Duration::from_secs(60));
    // The credentials go to the sidecar only, never into what a log could show.
    assert!(!format!("{command:?}").contains(SECRET));
}

#[test]
fn passes_the_credentials_it_was_given() {
    let runner = FakeRunner::answering("sent.json");
    channel(&runner).send(&alert()).unwrap();
    assert_eq!(
        runner.commands.borrow()[0].env,
        [
            ("PHOTON_PROJECT_ID".to_owned(), "proj_fixture".to_owned()),
            ("PHOTON_PROJECT_SECRET".to_owned(), SECRET.to_owned()),
        ]
    );
}

#[test]
fn the_sdk_s_console_output_before_the_result_is_ignored() {
    let stdout = format!(
        "[spectrum.lifecycle] INFO Spectrum started\n{}",
        fixture("photon/sent.json")
    );
    let runner = FakeRunner::printing(&stdout, 0);
    channel(&runner).send(&alert()).unwrap();
}

#[test]
fn a_number_that_has_not_texted_the_line_is_told_to() {
    let message = send_error(&FakeRunner::answering("not-texted-yet.json"));
    assert_eq!(
        message,
        "Photon won't message +32470123456 yet: a shared Photon line can only text a number \
         that has texted it first. Send any message to your Photon line from +32470123456, then \
         run again."
    );
}

#[test]
fn refused_credentials_say_how_to_store_them_again() {
    let message = send_error(&FakeRunner::answering("bad-credentials.json"));
    assert_eq!(
        message,
        "Photon refused your project credentials (Invalid credentials). Check your project's ID \
         and secret in the Photon dashboard (https://app.photon.codes) and store them again with \
         `hackathon-radar init` (or PHOTON_PROJECT_ID and PHOTON_PROJECT_SECRET, if you set them \
         in the environment)."
    );
}

#[test]
fn missing_sidecar_dependencies_say_to_run_npm_ci() {
    let message = send_error(&FakeRunner::answering("sdk-missing.json"));
    assert_eq!(
        message,
        "the Photon sidecar's dependencies aren't installed. Run `npm ci` once in \
         /opt/hackathon-radar/sidecar/photon (it needs Node.js 20.18.1 or later)."
    );
}

#[test]
fn rate_limits_wait_for_the_next_run() {
    let message = send_error(&FakeRunner::answering("rate-limited.json"));
    assert!(
        message.contains("Daily message limit exceeded"),
        "{message}"
    );
    assert!(message.contains("next Run"), "{message}");
}

#[test]
fn missing_node_says_to_install_it() {
    let runner = FakeRunner::not_installed();
    let message = send_error(&runner);
    assert!(
        message.starts_with(
            "Photon Alerts need Node.js 20.18.1 or later, and `node` isn't on your PATH."
        ),
        "{message}"
    );
    assert!(message.contains("schedule install"), "{message}");
}

#[test]
fn a_sidecar_that_is_not_there_says_where_it_was_looked_for() {
    let stderr = "node:internal/modules/cjs/loader:1228\n  throw err;\n  ^\n\n\
                  Error: Cannot find module '/opt/hackathon-radar/sidecar/photon/send.mjs'\n";
    let message = send_error(&FakeRunner::stdout_stderr("", stderr, 1));
    assert_eq!(
        message,
        "the Photon sidecar isn't at /opt/hackathon-radar/sidecar/photon/send.mjs. Set \
         HACKATHON_RADAR_PHOTON_SIDECAR_DIR to the sidecar/photon directory of your \
         hackathon-radar checkout."
    );
}

#[test]
fn a_crash_shows_the_end_of_stderr_without_the_secret() {
    let stderr = format!("TypeError: boom while using {SECRET}\n    at send (lib.mjs:1:1)\n");
    let message = send_error(&FakeRunner::stdout_stderr("", &stderr, 1));
    assert!(
        message.starts_with("the Photon sidecar gave no result (exit code 1): TypeError: boom"),
        "{message}"
    );
    assert!(message.contains("[redacted]"), "{message}");
}

#[test]
fn a_result_that_claims_success_with_a_failing_exit_is_a_failure() {
    let runner = FakeRunner::printing(&fixture("photon/sent.json"), 1);
    assert!(channel(&runner).send(&alert()).is_err());
}

#[test]
fn without_credentials_nothing_runs_and_the_error_says_what_to_set() {
    let runner = FakeRunner::answering("sent.json");
    let why = Credentials::from_vars(Some("proj_fixture".into()), Some("  ".into()))
        .unwrap_err()
        .to_string();
    let mut channel = PhotonChannel::new(
        PhotonSetting { to: TO.into() },
        Err(why),
        sidecar(),
        &runner,
    );

    let message = format!("{:#}", channel.send(&alert()).unwrap_err());

    assert_eq!(runner.runs(), 0);
    assert_eq!(
        message,
        "Photon Alerts need your Photon project credentials, but PHOTON_PROJECT_SECRET is not \
         set. Run `hackathon-radar init` to store them in your keychain, or set \
         PHOTON_PROJECT_ID and PHOTON_PROJECT_SECRET in the environment; your project's ID and \
         secret are in the Photon dashboard (https://app.photon.codes). They never go in the \
         Profile."
    );
}

#[test]
fn credentials_name_every_missing_variable_and_never_debug_print_the_secret() {
    let both = Credentials::from_vars(None, Some(String::new())).unwrap_err();
    assert!(
        both.to_string()
            .contains("but PHOTON_PROJECT_ID and PHOTON_PROJECT_SECRET are not set"),
        "{both}"
    );
    let credentials =
        Credentials::from_vars(Some(" proj_fixture ".into()), Some(SECRET.into())).unwrap();
    assert_eq!(credentials, Credentials::new("proj_fixture", SECRET));
    let debug = format!("{credentials:?}");
    assert!(
        debug.contains("proj_fixture") && !debug.contains(SECRET),
        "{debug}"
    );
}

#[test]
fn the_bundled_sidecar_is_in_this_repository() {
    let bundled = Sidecar::bundled();
    assert!(bundled.script().is_file(), "{}", bundled.script().display());
    assert!(bundled.dir.join("package-lock.json").is_file());
    assert_eq!(photon::SCRIPT, "send.mjs");
}

// Seam 1: whole Runs.

#[test]
fn a_run_sends_the_welcome_alert_by_imessage_and_announces_it() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let runner = FakeRunner::answering("sent.json");
    let mut channel = channel(&runner);

    let outcome = harness
        .try_run_with_channel(
            &mut channel,
            &[hackathon_be_from_fixture()],
            date(2026, 11, 9),
            LIVE_RUN,
        )
        .unwrap();

    assert!(outcome.sent);
    let commands = runner.commands.borrow();
    assert_eq!(commands.len(), 1);
    let request: serde_json::Value = serde_json::from_str(&commands[0].stdin).unwrap();
    assert_eq!(request["to"], TO);
    assert_eq!(request["text"], outcome.alert.unwrap().text);
    assert!(
        request["text"]
            .as_str()
            .unwrap()
            .starts_with("hackathon-radar: welcome! 5 Hackathons already fit you")
    );
    assert_eq!(harness.announced_names().len(), 5);
}

#[test]
fn a_run_whose_imessage_is_refused_announces_nothing_and_the_next_run_sends_it() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let refused = FakeRunner::answering("not-texted-yet.json");

    let failed = harness
        .try_run_with_channel(
            &mut channel(&refused),
            &[hackathon_be_from_fixture()],
            date(2026, 11, 9),
            LIVE_RUN,
        )
        .unwrap();

    let error = failed.alert_failure.expect("the Alert channel failed");
    assert!(error.contains("texted it first"), "{error}");
    assert!(!failed.sent);
    assert!(harness.announced_names().is_empty());
    assert_eq!(harness.stored_names().len(), 5, "what Fits is still stored");

    // Once the person has texted their line, the next Run sends the Welcome Alert.
    let sent = FakeRunner::answering("sent.json");
    let outcome = harness
        .try_run_with_channel(
            &mut channel(&sent),
            &[hackathon_be_from_fixture()],
            date(2026, 11, 12),
            LIVE_RUN,
        )
        .unwrap();
    assert!(outcome.sent);
    assert!(
        outcome
            .alert
            .unwrap()
            .text
            .starts_with("hackathon-radar: welcome!")
    );
    assert_eq!(harness.announced_names().len(), 5);
}

#[test]
fn a_dry_run_never_starts_the_sidecar() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let runner = FakeRunner::answering("sent.json");

    let outcome = harness
        .try_run_with_channel(
            &mut channel(&runner),
            &[hackathon_be_from_fixture()],
            date(2026, 11, 9),
            DRY_RUN,
        )
        .unwrap();

    assert!(outcome.alert.is_some());
    assert_eq!(runner.runs(), 0);
}
