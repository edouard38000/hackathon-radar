//! Seam 1: whole Runs whose Alert channel and Fare lookup are built from the
//! Profile and the secret store, as `hackathon-radar run` builds them. Their
//! secrets come from the (fake) keychain, which a scheduled Run can read, and
//! never need the person's shell environment. Nothing touches the network.

mod support;

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use hackathon_radar::alert::photon::Sidecar;
use hackathon_radar::alert::{AlertChannel, ntfy};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::fares::FareLookup;
use hackathon_radar::fares::serpapi::ENDPOINT;
use hackathon_radar::http::HttpGet;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::Profile;
use hackathon_radar::run::{self, RunDeps, RunOptions, RunOutcome};
use hackathon_radar::secrets::Secrets;
use hackathon_radar::sources::Source;
use hackathon_radar::wiring;
use support::*;

const NTFY: &str = "[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar-wiring\"\n\
                    server = \"https://ntfy.example.org\"\n";
const PHOTON: &str = "[alert_channel]\nkind = \"photon\"\nto = \"+32470123456\"\n";

fn profile_with(extra: &str) -> Profile {
    travel_profile(&["Belgium", "Germany"], extra)
}

/// Records each command and answers like the Photon sidecar that sent its message.
#[derive(Default)]
struct RecordingRunner {
    commands: RefCell<Vec<Command>>,
}

impl ProcessRunner for &RecordingRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
        Ok(Output {
            code: Some(0),
            stdout: fixture("photon/sent.json"),
            stderr: String::new(),
        })
    }
}

/// For channels that must never start a program.
struct NoProcess;

impl ProcessRunner for NoProcess {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        panic!("nothing should run, but {} did", command.program)
    }
}

/// Answers every SerpApi search with the saved Munich reply, recording the URLs.
#[derive(Default)]
struct SerpApiFixture {
    requested: RefCell<Vec<String>>,
}

impl HttpGet for &SerpApiFixture {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        assert!(url.starts_with(ENDPOINT), "only SerpApi is reached: {url}");
        self.requested.borrow_mut().push(url.to_owned());
        Ok(fixture("serpapi/bru-crl-muc.json"))
    }
}

fn sidecar() -> Sidecar {
    Sidecar {
        dir: PathBuf::from("/opt/hackathon-radar/sidecar/photon"),
    }
}

fn run_with(
    harness: &mut Harness,
    channel: &mut dyn AlertChannel,
    fares: Option<&dyn FareLookup>,
    sources: &[Box<dyn Source>],
    options: RunOptions,
) -> RunOutcome {
    run::run(
        RunDeps {
            profile: &harness.profile,
            sources,
            clock: &FixedClock(date(2026, 11, 9)),
            store: &mut harness.store,
            channel,
            agent: &harness.agent,
            fares,
        },
        options,
    )
    .expect("the Run failed")
}

fn secrets(keychain: &FakeKeychain, dir: &Path, env: &[(&str, &str)]) -> Secrets {
    secrets_in(keychain, dir, env)
}

#[test]
fn a_run_sends_ntfy_alerts_with_the_token_from_the_keychain() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default().with("ntfy-token", "tk_from_keychain");
    let mut harness = Harness::new(profile_with(NTFY));
    let http = RecordingPost::ok();
    let store = secrets(&keychain, dir.path(), &[]);
    let mut channel =
        wiring::alert_channel(&harness.profile, &store, false, &http, NoProcess, sidecar())
            .unwrap();

    let outcome = run_with(
        &mut harness,
        channel.as_mut(),
        None,
        &[hackathon_be_from_fixture()],
        LIVE_RUN,
    );

    assert!(outcome.sent);
    let requests = http.requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://ntfy.example.org/radar-wiring");
    assert!(
        requests[0]
            .headers
            .contains(&("Authorization".into(), "Bearer tk_from_keychain".into())),
        "{:?}",
        requests[0].headers
    );
}

#[test]
fn the_ntfy_token_variable_still_overrides_the_keychain() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default().with("ntfy-token", "tk_from_keychain");
    let store = secrets(
        &keychain,
        dir.path(),
        &[(ntfy::TOKEN_ENV_VAR, "tk_from_env")],
    );
    let mut harness = Harness::new(profile_with(NTFY));
    let http = RecordingPost::ok();
    let mut channel =
        wiring::alert_channel(&harness.profile, &store, false, &http, NoProcess, sidecar())
            .unwrap();

    run_with(
        &mut harness,
        channel.as_mut(),
        None,
        &[hackathon_be_from_fixture()],
        LIVE_RUN,
    );

    let requests = http.requests.borrow();
    assert!(
        requests[0]
            .headers
            .contains(&("Authorization".into(), "Bearer tk_from_env".into()))
    );
}

#[test]
fn a_run_sends_photon_alerts_with_the_credentials_from_the_keychain() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default()
        .with("photon-project-id", "proj_keychain")
        .with("photon-project-secret", "secret_keychain");
    let mut harness = Harness::new(profile_with(PHOTON));
    let runner = RecordingRunner::default();
    let store = secrets(&keychain, dir.path(), &[]);
    let mut channel = wiring::alert_channel(
        &harness.profile,
        &store,
        false,
        RecordingPost::ok(),
        &runner,
        sidecar(),
    )
    .unwrap();

    let outcome = run_with(
        &mut harness,
        channel.as_mut(),
        None,
        &[hackathon_be_from_fixture()],
        LIVE_RUN,
    );

    assert!(outcome.sent);
    let commands = runner.commands.borrow();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "node");
    assert_eq!(
        commands[0].env,
        [
            ("PHOTON_PROJECT_ID".to_owned(), "proj_keychain".to_owned()),
            (
                "PHOTON_PROJECT_SECRET".to_owned(),
                "secret_keychain".to_owned()
            ),
        ]
    );
    // The secret never shows in the command's Debug output.
    assert!(!format!("{:?}", commands[0]).contains("secret_keychain"));
}

#[test]
fn photon_credentials_on_a_headless_machine_come_from_the_owner_only_file() {
    let dir = tempfile::tempdir().unwrap();
    let headless = FakeKeychain::unavailable();
    let store = secrets(&headless, dir.path(), &[]);
    for (name, value) in [
        (
            hackathon_radar::secrets::SecretName::PhotonProjectId,
            "proj_file",
        ),
        (
            hackathon_radar::secrets::SecretName::PhotonProjectSecret,
            "secret_file",
        ),
    ] {
        store
            .store(name, &hackathon_radar::secrets::Secret::new(value).unwrap())
            .unwrap();
    }
    let mut harness = Harness::new(profile_with(PHOTON));
    let runner = RecordingRunner::default();
    let mut channel = wiring::alert_channel(
        &harness.profile,
        &store,
        false,
        RecordingPost::ok(),
        &runner,
        sidecar(),
    )
    .unwrap();

    run_with(
        &mut harness,
        channel.as_mut(),
        None,
        &[hackathon_be_from_fixture()],
        LIVE_RUN,
    );

    assert_eq!(
        runner.commands.borrow()[0].env[1],
        ("PHOTON_PROJECT_SECRET".to_owned(), "secret_file".to_owned())
    );
}

#[test]
fn a_live_run_without_photon_credentials_stops_before_the_run_but_a_dry_run_goes_ahead() {
    let dir = tempfile::tempdir().unwrap();
    let store = secrets(&FakeKeychain::default(), dir.path(), &[]);
    let mut harness = Harness::new(profile_with(PHOTON));
    let runner = RecordingRunner::default();

    let error = wiring::alert_channel(
        &harness.profile,
        &store,
        false,
        RecordingPost::ok(),
        &runner,
        sidecar(),
    )
    .err()
    .expect("missing credentials stop a live Run")
    .to_string();
    assert!(error.contains("hackathon-radar init"), "{error}");
    assert!(
        error.contains("PHOTON_PROJECT_ID and PHOTON_PROJECT_SECRET are not set"),
        "{error}"
    );

    let mut channel = wiring::alert_channel(
        &harness.profile,
        &store,
        true,
        RecordingPost::ok(),
        &runner,
        sidecar(),
    )
    .unwrap();
    let outcome = run_with(
        &mut harness,
        channel.as_mut(),
        None,
        &[hackathon_be_from_fixture()],
        DRY_RUN,
    );
    assert!(outcome.alert.is_some());
    assert!(runner.commands.borrow().is_empty());
}

fn munich() -> Candidate {
    let start = date(2026, 12, 5);
    Candidate {
        source: "fake".into(),
        source_key: "Munich Hack".into(),
        name: "Munich Hack".into(),
        start,
        end: date(2026, 12, 6),
        city: Some("Munich".into()),
        location: None,
        country: Some("Germany".into()),
        link: None,
    }
}

#[test]
fn a_run_looks_up_fares_with_the_serpapi_key_from_the_keychain() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default().with("serpapi-api-key", "serp_keychain_key");
    let store = secrets(&keychain, dir.path(), &[]);
    let mut harness = Harness::new(profile_with("[travel]\nflight_budget = 200\n"));
    harness.agent = FakeAgent::approving().with_estimate("Munich", Some(6.4), Some("MUC"));
    let http = SerpApiFixture::default();
    let serpapi = wiring::fare_lookup(&store, &http)
        .unwrap()
        .expect("a key in the keychain turns SerpApi on");
    let sources: Vec<Box<dyn Source>> = vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(vec![munich()]),
    })];

    let outcome = run_with(
        &mut harness,
        &mut RecordingChannel::default(),
        Some(&serpapi),
        &sources,
        LIVE_RUN,
    );

    assert_eq!(outcome.fares_looked_up, 1);
    let requested = http.requested.borrow();
    assert_eq!(requested.len(), 1);
    assert!(
        requested[0].ends_with("&api_key=serp_keychain_key"),
        "{}",
        requested[0]
    );
    assert_eq!(harness.agent.fare_requests(), 0, "no Unverified fares");
    assert!(!format!("{serpapi:?}").contains("serp_keychain_key"));
}

#[test]
fn without_a_serpapi_key_anywhere_the_agent_estimates_fares() {
    let dir = tempfile::tempdir().unwrap();
    let store = secrets(&FakeKeychain::default(), dir.path(), &[]);
    let http = SerpApiFixture::default();
    assert!(wiring::fare_lookup(&store, &http).unwrap().is_none());

    let mut harness = Harness::new(profile_with("[travel]\nflight_budget = 200\n"));
    harness.agent = FakeAgent::approving()
        .with_estimate("Munich", Some(6.4), Some("MUC"))
        .with_fare_estimate("MUC", Some(120), Some("BRU"));
    let sources: Vec<Box<dyn Source>> = vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(vec![munich()]),
    })];

    run_with(
        &mut harness,
        &mut RecordingChannel::default(),
        None,
        &sources,
        LIVE_RUN,
    );

    assert_eq!(harness.agent.fare_requests(), 1);
    assert!(http.requested.borrow().is_empty());
}

#[test]
fn a_test_alert_is_one_short_alert_through_the_channel() {
    let dir = tempfile::tempdir().unwrap();
    let store = secrets(&FakeKeychain::default(), dir.path(), &[]);
    let http = RecordingPost::ok();
    let profile = profile_with(NTFY);
    let mut channel =
        wiring::alert_channel(&profile, &store, false, &http, NoProcess, sidecar()).unwrap();

    wiring::send_test_alert(channel.as_mut()).unwrap();

    let requests = http.requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, wiring::TEST_ALERT);

    // With no Alert channel, there is nobody to send it to.
    let mut printing = wiring::alert_channel(
        &profile_with(""),
        &store,
        false,
        &http,
        NoProcess,
        sidecar(),
    )
    .unwrap();
    let error = wiring::send_test_alert(printing.as_mut())
        .unwrap_err()
        .to_string();
    assert!(error.contains("no Alert channel"), "{error}");
}
