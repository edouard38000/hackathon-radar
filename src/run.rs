//! A Run: one pass that collects Candidates, decides which Hackathons Fit, and
//! sends at most one Alert. Everything it touches is passed in, so tests can
//! drive a whole Run with fakes.

use crate::agent::Agent;
use crate::alert::{self, Alert, AlertChannel, AlertKind};
use crate::candidate::Candidate;
use crate::clock::Clock;
use crate::judging;
use crate::matching;
use crate::profile::Profile;
use crate::rules;
use crate::sources::Source;
use crate::store::Store;
use crate::travel;

pub struct RunDeps<'a> {
    pub profile: &'a Profile,
    pub sources: &'a [Box<dyn Source>],
    pub clock: &'a dyn Clock,
    pub store: &'a mut Store,
    pub channel: &'a mut dyn AlertChannel,
    pub agent: &'a dyn Agent,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Compose the Alert but don't send it.
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct RunOutcome {
    /// How many Candidates the Sources returned, before any rule.
    pub collected: usize,
    /// The Hackathons that Fit, as stored.
    pub fitting: Vec<Candidate>,
    /// Sources that couldn't be read this time; the Run carries on without them.
    pub failed_sources: Vec<SourceFailure>,
    /// The Alert this Run composed, if anything qualified.
    pub alert: Option<Alert>,
    /// Whether the Alert went to the Alert channel.
    pub sent: bool,
    /// How many Candidates the Agent judged on this Run.
    pub judged: usize,
    /// How many Candidates are still waiting for the Agent; the next Run sends them again.
    pub unjudged: usize,
    /// Why the Agent couldn't judge this Run, if it couldn't.
    pub agent_failure: Option<String>,
    /// How many destination cities still wait for a travel estimate.
    pub unestimated: usize,
    /// Why the Agent's travel estimates failed this Run, if they did; those
    /// cities are asked about again on the next Run.
    pub travel_failure: Option<String>,
    /// How many collected listings were Duplicate listings merged into another.
    pub duplicates: usize,
    /// How many Hackathons wait for the Agent to say whether two listings are the same one.
    pub waiting_on_duplicates: usize,
}

#[derive(Debug)]
pub struct SourceFailure {
    pub source: String,
    pub error: String,
}

pub fn run(deps: RunDeps<'_>, options: RunOptions) -> anyhow::Result<RunOutcome> {
    let today = deps.clock.today();

    let mut candidates = Vec::new();
    let mut failed_sources = Vec::new();
    for source in deps.sources {
        match source.fetch() {
            Ok(found) => candidates.extend(found),
            Err(error) => failed_sources.push(SourceFailure {
                source: source.name().to_owned(),
                error: format!("{error:#}"),
            }),
        }
    }
    let collected = candidates.len();

    // From here on, the Run works on Hackathons: one Candidate per merged group of listings.
    let merged = matching::merge(deps.agent, deps.profile, deps.store, candidates, today)?;
    let passed = rules::apply(merged.hackathons, deps.profile, today);
    let judged = judging::judge(deps.agent, deps.profile, deps.store, passed, today)?;
    let fitting = judged.fitting;
    deps.store.save_hackathons(&fitting, today)?;
    // An Agent that just failed to judge isn't asked for travel estimates too.
    let travelled = travel::plan(
        deps.agent,
        deps.profile,
        deps.store,
        &fitting,
        today,
        judged.failure.is_none(),
    )?;

    // Until something has been Announced, the Alert is the Welcome Alert; either
    // way it only covers what no Alert has announced yet.
    let kind = if deps.store.has_announced_anything()? {
        AlertKind::New
    } else {
        AlertKind::Welcome
    };
    let not_announced = deps.store.not_announced(&fitting)?;
    let composed = alert::compose_with_trips(&not_announced, kind, |h| {
        travelled.trip(h).map(|t| t.label.clone())
    });
    let mut sent = false;
    if let Some(composed) = &composed
        && !options.dry_run
    {
        deps.channel.send(&composed.alert)?;
        if deps.channel.reaches_the_person() {
            deps.store.mark_announced(&composed.announces, today)?;
        }
        sent = true;
    }

    Ok(RunOutcome {
        collected,
        fitting,
        failed_sources,
        alert: composed.map(|c| c.alert),
        sent,
        judged: judged.judged_now,
        unjudged: judged.unjudged,
        agent_failure: judged.failure.or(merged.failure),
        unestimated: travelled.unestimated,
        travel_failure: travelled.failure,
        duplicates: merged.duplicates,
        waiting_on_duplicates: merged.waiting,
    })
}
