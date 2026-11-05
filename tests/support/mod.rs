//! The whole-Run harness: fakes for everything a Run touches, so tests drive a
//! real Run without the network. Later tickets add their fakes here.

#![allow(dead_code)] // Each test binary uses a different subset.

use std::cell::RefCell;
use std::path::Path;

use chrono::NaiveDate;
use tempfile::TempDir;

use hackathon_radar::alert::{Alert, AlertChannel};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::clock::Clock;
use hackathon_radar::http::{HttpGet, HttpPost, PostRequest};
use hackathon_radar::profile::Profile;
use hackathon_radar::run::{self, RunDeps, RunOptions, RunOutcome};
use hackathon_radar::sources::Source;
use hackathon_radar::sources::hackathon_be::{self, HackathonBe};
use hackathon_radar::store::Store;

pub fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

pub struct FixedClock(pub NaiveDate);

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.0
    }
}

/// Serves saved pages by URL and records every request. An unknown URL is an
/// error, so a test can't reach the network by accident.
pub struct FixtureHttp {
    pages: Vec<(String, String)>,
    pub requested: RefCell<Vec<String>>,
}

impl FixtureHttp {
    pub fn new(pages: &[(&str, String)]) -> Self {
        Self {
            pages: pages
                .iter()
                .map(|(url, body)| (url.to_string(), body.clone()))
                .collect(),
            requested: RefCell::new(Vec::new()),
        }
    }
}

impl HttpGet for FixtureHttp {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        self.requested.borrow_mut().push(url.to_owned());
        self.pages
            .iter()
            .find(|(u, _)| u == url)
            .map(|(_, body)| body.clone())
            .ok_or_else(|| anyhow::anyhow!("no fixture for {url}"))
    }
}

/// The real hackathon.be Source, reading its saved calendar.
pub fn hackathon_be_from_fixture() -> Box<dyn Source> {
    let http = FixtureHttp::new(&[(
        hackathon_be::CALENDAR_URL,
        fixture("hackathon-be/calendar.ics"),
    )]);
    Box::new(HackathonBe::new(http))
}

/// A Source that returns canned Candidates, or fails.
pub struct FakeSource {
    pub name: String,
    pub result: Result<Vec<Candidate>, String>,
}

impl Source for FakeSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        self.result.clone().map_err(anyhow::Error::msg)
    }
}

/// Records each Alert instead of sending it, or fails like an unreachable channel
/// while `fail_with` is set.
#[derive(Default)]
pub struct RecordingChannel {
    pub sent: Vec<Alert>,
    pub fail_with: Option<String>,
}

impl AlertChannel for RecordingChannel {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()> {
        if let Some(error) = &self.fail_with {
            anyhow::bail!("{error}");
        }
        self.sent.push(alert.clone());
        Ok(())
    }
}

/// Records each POST and answers with `reply`, so an Alert channel's request
/// can be checked without the network.
pub struct RecordingPost {
    pub requests: RefCell<Vec<PostRequest>>,
    pub reply: Result<(), String>,
}

impl RecordingPost {
    pub fn ok() -> Self {
        Self {
            requests: RefCell::new(Vec::new()),
            reply: Ok(()),
        }
    }
}

impl HttpPost for RecordingPost {
    fn post(&self, request: &PostRequest) -> anyhow::Result<()> {
        self.requests.borrow_mut().push(request.clone());
        self.reply.clone().map_err(anyhow::Error::msg)
    }
}

/// A request written the way the recorded fixtures are: the method and URL,
/// one header per line, a blank line, then the body.
pub fn render_request(request: &PostRequest) -> String {
    let mut text = format!("POST {}\n", request.url);
    for (name, value) in &request.headers {
        text.push_str(&format!("{name}: {value}\n"));
    }
    text.push('\n');
    text.push_str(&request.body);
    text
}

pub fn profile(countries: &[&str]) -> Profile {
    let countries = countries
        .iter()
        .map(|c| format!("{c:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        "countries = [{countries}]\n\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\", \"CRL\"]\n"
    );
    Profile::from_toml(&text, Path::new("test-profile.toml")).unwrap()
}

/// Everything one or more Runs share: a Profile, a temporary database on disk and
/// a recording Alert channel. Each Run gets its own Sources and clock.
pub struct Harness {
    pub profile: Profile,
    pub store: Store,
    pub channel: RecordingChannel,
    _dir: TempDir,
}

impl Harness {
    pub fn new(profile: Profile) -> Self {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("hackathon-radar.sqlite")).unwrap();
        Self {
            profile,
            store,
            channel: RecordingChannel::default(),
            _dir: dir,
        }
    }

    pub fn run(
        &mut self,
        sources: &[Box<dyn Source>],
        today: NaiveDate,
        options: RunOptions,
    ) -> RunOutcome {
        self.try_run(sources, today, options)
            .expect("the Run failed")
    }

    pub fn try_run(
        &mut self,
        sources: &[Box<dyn Source>],
        today: NaiveDate,
        options: RunOptions,
    ) -> anyhow::Result<RunOutcome> {
        run::run(
            RunDeps {
                profile: &self.profile,
                sources,
                clock: &FixedClock(today),
                store: &mut self.store,
                channel: &mut self.channel,
            },
            options,
        )
    }

    pub fn stored_names(&self) -> Vec<String> {
        self.store
            .hackathons()
            .unwrap()
            .into_iter()
            .map(|h| h.name)
            .collect()
    }

    /// The stored Hackathons an Alert has announced, soonest first.
    pub fn announced_names(&self) -> Vec<String> {
        self.store
            .hackathons()
            .unwrap()
            .into_iter()
            .filter(|h| h.announced_on.is_some())
            .map(|h| h.name)
            .collect()
    }
}

pub const DRY_RUN: RunOptions = RunOptions { dry_run: true };
pub const LIVE_RUN: RunOptions = RunOptions { dry_run: false };
