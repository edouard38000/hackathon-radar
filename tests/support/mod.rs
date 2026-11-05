//! The whole-Run harness: fakes for everything a Run touches, so tests drive a
//! real Run without the network. Later tickets add their fakes here.

#![allow(dead_code)] // Each test binary uses a different subset.

use std::cell::RefCell;
use std::path::Path;

use chrono::NaiveDate;
use tempfile::TempDir;

use hackathon_radar::agent::{Agent, JudgeRequest, Judgement, TravelFunded};
use hackathon_radar::alert::{Alert, AlertChannel};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::clock::Clock;
use hackathon_radar::http::{HttpGet, HttpPost, PostRequest};
use hackathon_radar::profile::{Eligibility, Profile};
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
/// while `fail_with` is set. With `print_only`, it behaves like the print
/// fallback: Alerts are recorded but reach no one.
#[derive(Default)]
pub struct RecordingChannel {
    pub sent: Vec<Alert>,
    pub fail_with: Option<String>,
    pub print_only: bool,
}

impl AlertChannel for RecordingChannel {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()> {
        if let Some(error) = &self.fail_with {
            anyhow::bail!("{error}");
        }
        self.sent.push(alert.clone());
        Ok(())
    }

    fn reaches_the_person(&self) -> bool {
        !self.print_only
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

/// What the fake Agent does when asked.
pub enum FakeAnswer {
    /// Judges each Candidate with the verdict set for its name, or the default.
    Judge,
    /// Fails, as an Agent that errors, times out or runs out of steps does.
    Fail(String),
    /// Answers this text as it is, matching the schema or not.
    Raw(String),
}

/// An Agent that answers canned judgements and records what it was asked.
pub struct FakeAgent {
    pub answer: FakeAnswer,
    /// Verdicts by Candidate name; any other Candidate gets `default`.
    pub verdicts: Vec<(String, Judgement)>,
    pub default: Judgement,
    /// Candidate names the answer leaves out.
    pub leave_out: Vec<String>,
    /// The names of the Candidates sent, one list per request.
    pub asked: RefCell<Vec<Vec<String>>>,
    /// The Eligibility facts sent with each request.
    pub eligibility: RefCell<Vec<Eligibility>>,
}

impl FakeAgent {
    /// Judges every Candidate a Hackathon the person is eligible for, not Travel-funded.
    pub fn approving() -> Self {
        Self {
            answer: FakeAnswer::Judge,
            verdicts: Vec::new(),
            default: hackathon(),
            leave_out: Vec::new(),
            asked: RefCell::new(Vec::new()),
            eligibility: RefCell::new(Vec::new()),
        }
    }

    pub fn failing(error: &str) -> Self {
        Self {
            answer: FakeAnswer::Fail(error.to_owned()),
            ..Self::approving()
        }
    }

    pub fn answering(raw: &str) -> Self {
        Self {
            answer: FakeAnswer::Raw(raw.to_owned()),
            ..Self::approving()
        }
    }

    pub fn with_verdict(mut self, name: &str, judgement: Judgement) -> Self {
        self.verdicts.push((name.to_owned(), judgement));
        self
    }

    pub fn requests(&self) -> usize {
        self.asked.borrow().len()
    }
}

impl Agent for FakeAgent {
    fn name(&self) -> &str {
        "fake"
    }

    fn judge(&self, request: &JudgeRequest<'_>) -> anyhow::Result<String> {
        let names = request
            .candidates
            .iter()
            .map(|c| c.candidate.name.clone())
            .collect();
        self.asked.borrow_mut().push(names);
        self.eligibility
            .borrow_mut()
            .push(request.eligibility.clone());
        match &self.answer {
            FakeAnswer::Fail(error) => anyhow::bail!("{error}"),
            FakeAnswer::Raw(text) => Ok(text.clone()),
            FakeAnswer::Judge => {
                let judgements: Vec<serde_json::Value> = request
                    .candidates
                    .iter()
                    .filter(|sent| !self.leave_out.contains(&sent.candidate.name))
                    .map(|sent| {
                        let j = self
                            .verdicts
                            .iter()
                            .find(|(name, _)| *name == sent.candidate.name)
                            .map_or(&self.default, |(_, j)| j);
                        serde_json::json!({
                            "id": sent.id,
                            "is_hackathon": j.is_hackathon,
                            "eligible": j.eligible,
                            "eligibility_restriction": j.eligibility_restriction,
                            "travel_funded": j.travel_funded,
                            "travel_detail": j.travel_detail,
                            "reason": j.reason,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "judgements": judgements }).to_string())
            }
        }
    }
}

/// A Hackathon the person is eligible for, not Travel-funded.
pub fn hackathon() -> Judgement {
    Judgement {
        is_hackathon: true,
        eligible: true,
        eligibility_restriction: None,
        travel_funded: TravelFunded::None,
        travel_detail: None,
        reason: "Teams build software over a weekend.".into(),
    }
}

pub fn not_a_hackathon() -> Judgement {
    Judgement {
        is_hackathon: false,
        reason: "A pitch competition; nothing is built.".into(),
        ..hackathon()
    }
}

pub fn ineligible(restriction: &str) -> Judgement {
    Judgement {
        eligible: false,
        eligibility_restriction: Some(restriction.into()),
        reason: "Only open to others.".into(),
        ..hackathon()
    }
}

pub fn travel_funded(how: TravelFunded, detail: &str) -> Judgement {
    Judgement {
        travel_funded: how,
        travel_detail: Some(detail.into()),
        ..hackathon()
    }
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

/// Everything one or more Runs share: a Profile, a temporary database on disk,
/// a recording Alert channel and a fake Agent (which approves every Candidate
/// unless a test sets it up otherwise). Each Run gets its own Sources and clock.
pub struct Harness {
    pub profile: Profile,
    pub store: Store,
    pub channel: RecordingChannel,
    pub agent: FakeAgent,
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
            agent: FakeAgent::approving(),
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
                agent: &self.agent,
            },
            options,
        )
    }

    /// A Run with another Agent than the harness's fake, such as a real adapter
    /// driven by a fake process runner.
    pub fn run_with_agent(
        &mut self,
        agent: &dyn Agent,
        sources: &[Box<dyn Source>],
        today: NaiveDate,
        options: RunOptions,
    ) -> RunOutcome {
        run::run(
            RunDeps {
                profile: &self.profile,
                sources,
                clock: &FixedClock(today),
                store: &mut self.store,
                channel: &mut self.channel,
                agent,
            },
            options,
        )
        .expect("the Run failed")
    }

    pub fn unjudged_names(&self) -> Vec<String> {
        self.store
            .unjudged()
            .unwrap()
            .into_iter()
            .map(|u| u.candidate.name)
            .collect()
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
