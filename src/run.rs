//! A Run: one pass that collects Candidates, decides which Hackathons Fit, and
//! sends at most one Alert. Everything it touches is passed in, so tests can
//! drive a whole Run with fakes.

use crate::agent::Agent;
use crate::alert::{self, Alert, AlertChannel, AlertKind, attention};
use crate::candidate::Candidate;
use crate::clock::Clock;
use crate::deadlines;
use crate::fares::{self, FareLookup};
use crate::judging;
use crate::matching;
use crate::profile::Profile;
use crate::rules;
use crate::sources::Source;
use crate::store::{RunCounts, RunRecord, Store};
use crate::travel;

pub mod failure;

pub struct RunDeps<'a> {
    pub profile: &'a Profile,
    pub sources: &'a [Box<dyn Source>],
    pub clock: &'a dyn Clock,
    pub store: &'a mut Store,
    pub channel: &'a mut dyn AlertChannel,
    pub agent: &'a dyn Agent,
    /// Looks up Fares on the person's SerpApi key; `None` without a key, when
    /// the Agent estimates Unverified fares instead.
    pub fares: Option<&'a dyn FareLookup>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Compose the Alert but don't send it. A dry run isn't recorded in the
    /// Run history and never counts as a failed Run.
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
    /// The Alert this Run composed, if anything qualified, with the "needs
    /// attention" part first when it carries one.
    pub alert: Option<Alert>,
    /// Whether the Alert went to the Alert channel.
    pub sent: bool,
    /// How many Hackathons the Alert Announced.
    pub announced: usize,
    /// Why the Alert channel couldn't send the Alert, if it couldn't; its
    /// Hackathons wait, not Announced, for the next Run.
    pub alert_failure: Option<String>,
    /// Whether the Alert carries the "needs attention" part ([`failure`]).
    pub needs_attention: bool,
    /// Why this Run failed, in one line, if it did ([`failure`]). A dry run
    /// says so too, though it's never recorded.
    pub failure: Option<String>,
    /// How many Candidates the Agent judged on this Run.
    pub judged: usize,
    /// How many Candidates are still waiting for the Agent; the next Run sends them again.
    pub unjudged: usize,
    /// The judged and waiting Candidates, in Countries and outside them.
    pub judging: judging::Tally,
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
    /// How many known Hackathons the Agent rechecked for deadline, status and travel funding.
    pub rechecked: usize,
    /// Why the Agent's recheck failed this Run, if it did; those Hackathons are due again.
    pub recheck_failure: Option<String>,
    /// How many Fares were looked up, or estimated by the Agent, on this Run.
    pub fares_looked_up: usize,
    /// How many flights wait for a Fare; they are held back from Alerts until they have one.
    pub waiting_on_fares: usize,
    /// How many flights are over the Flight budget, so don't Fit.
    pub over_budget: usize,
    /// Why Fare lookups failed this Run, if they did; they are tried again on the next Run.
    pub fare_failure: Option<String>,
}

#[derive(Debug)]
pub struct SourceFailure {
    pub source: String,
    pub error: String,
}

/// Runs once and, unless it's a dry run, records it in the Run history.
pub fn run(mut deps: RunDeps<'_>, options: RunOptions) -> anyhow::Result<RunOutcome> {
    let started_at = deps.clock.now();
    let result = run_steps(&mut deps, options, started_at);
    if let Err(error) = &result
        && !options.dry_run
    {
        // A Run stopped by an unexpected error still counts as failed. When
        // the database is what failed, this can't be recorded either.
        let _ = deps.store.record_run(&RunRecord {
            day: deps.clock.today(),
            started_at,
            finished_at: deps.clock.now(),
            failure: Some(attention::one_line(&format!("{error:#}"))),
            counts: RunCounts::default(),
            attention_sent: false,
        });
    }
    result
}

fn run_steps(
    deps: &mut RunDeps<'_>,
    options: RunOptions,
    started_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<RunOutcome> {
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
    // An Agent that just failed to judge isn't asked for travel or Fare estimates too.
    let travelled = travel::plan(
        deps.agent,
        deps.profile,
        deps.store,
        &judged.fitting,
        today,
        judged.failure.is_none(),
    )?;
    // The Flight budget: flights over it don't Fit, so they aren't stored.
    let priced = fares::price(
        deps.agent,
        deps.profile,
        deps.store,
        &judged.fitting,
        &travelled,
        deps.fares,
        today,
        judged.failure.is_none(),
    )?;
    let fitting: Vec<Candidate> = judged
        .fitting
        .into_iter()
        .filter(|h| priced.fits(h))
        .collect();
    deps.store.save_hackathons(&fitting, today)?;

    // Deadlines, status and travel funding of known Hackathons; passed deadlines
    // close even when the Agent failed.
    let rechecked = deadlines::recheck(
        deps.agent,
        deps.profile,
        deps.store,
        today,
        judged.failure.is_none(),
    )?;
    let fitting = deadlines::still_fitting(deps.store, deps.profile, fitting, today)?;

    // Until something has been Announced, the Alert is the Welcome Alert; either
    // way it only covers what no Alert has announced yet, plus Deadline
    // reminders and newly Travel-funded Hackathons decided from the store.
    // Closed Hackathons are left out, so they stay unannounced until they reopen.
    let kind = if deps.store.has_announced_anything()? {
        AlertKind::New
    } else {
        AlertKind::Welcome
    };
    let not_announced = deps.store.not_announced(&fitting)?;
    let mut entries = deadlines::entries(deps.store, deps.profile, &not_announced, today)?;
    // Each flight's Fare; a flight over the budget or still waiting for its Fare is left out.
    priced.fill(&mut entries);
    let composed = alert::compose_entries(entries, kind);

    // Whatever failed above, the Alert is composed from what's stored and sent;
    // a failing Run may add the "needs attention" part to it.
    let agent_failure = judged.failure.or(merged.failure);
    let failure = failure::before_sending(
        agent_failure.as_deref(),
        &failed_sources,
        deps.sources.len(),
    );
    let attention = match &failure {
        Some(why) if !options.dry_run => failure::attention_due(deps.store, why)?,
        _ => None,
    };
    let alert = attention::merged(attention.as_ref(), composed.as_ref().map(|c| &c.alert));
    let (mut sent, mut announced, mut alert_failure, mut attention_sent) = (false, 0, None, false);
    if let Some(alert) = &alert
        && !options.dry_run
    {
        match deps.channel.send(alert) {
            Ok(()) => {
                sent = true;
                if deps.channel.reaches_the_person() {
                    if let Some(composed) = &composed {
                        announced = deps.store.mark_announced(&composed.announces, today)?;
                        deps.store.mark_reminded(&composed.reminds, today)?;
                        deps.store
                            .mark_funding_announced(&composed.announces_funding, today)?;
                    }
                    attention_sent = attention.is_some();
                }
            }
            Err(error) => alert_failure = Some(attention::one_line(&format!("{error:#}"))),
        }
    }
    let failure = failure::with_alert_failure(failure, alert_failure.as_deref());

    let outcome = RunOutcome {
        collected,
        fitting,
        failed_sources,
        alert,
        sent,
        announced,
        alert_failure,
        needs_attention: attention.is_some(),
        failure,
        judged: judged.tally.judged.total(),
        unjudged: judged.tally.waiting.total(),
        judging: judged.tally,
        agent_failure,
        unestimated: travelled.unestimated,
        travel_failure: travelled.failure,
        duplicates: merged.duplicates,
        waiting_on_duplicates: merged.waiting,
        rechecked: rechecked.rechecked,
        recheck_failure: rechecked.failure,
        fares_looked_up: priced.looked_up,
        waiting_on_fares: priced.waiting(),
        over_budget: priced.over_budget(),
        fare_failure: priced.failure,
    };
    if !options.dry_run {
        deps.store.record_run(&RunRecord {
            day: today,
            started_at,
            finished_at: deps.clock.now(),
            failure: outcome.failure.clone(),
            counts: RunCounts {
                collected: outcome.collected,
                judged: outcome.judged,
                unjudged: outcome.unjudged,
                fit: outcome.fitting.len(),
                announced: outcome.announced,
                sent: outcome.sent,
                failed_sources: outcome.failed_sources.len(),
            },
            attention_sent,
        })?;
    }
    Ok(outcome)
}
