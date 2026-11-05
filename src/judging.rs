//! The Run step that hands Candidates to the Agent (ADR 0002).
//!
//! Candidates that passed the data-only rules and have no judgement yet are
//! queued in the store, then sent to the Agent. Its answer is checked against
//! the schema and stored. When the Agent fails, or its answer doesn't match the
//! schema, the Candidates stay queued and go to the Agent again on the next Run;
//! the Run itself carries on.

use std::collections::HashSet;

use chrono::NaiveDate;

use crate::agent::{self, Agent, JudgeRequest};
use crate::candidate::Candidate;
use crate::profile::Profile;
use crate::rules;
use crate::store::Store;

/// The most Candidates sent to the Agent on one Run, soonest first, so one
/// request fits within the Profile's step cap. The rest wait for the next Run.
pub const MAX_CANDIDATES_PER_RUN: usize = 20;

#[derive(Debug)]
pub struct Judged {
    /// The Candidates the Agent judged to be Hackathons the person is eligible for.
    pub fitting: Vec<Candidate>,
    /// How many Candidates the Agent judged on this Run.
    pub judged_now: usize,
    /// How many Candidates are still waiting for a judgement.
    pub unjudged: usize,
    /// Why the Agent couldn't judge this Run, if it couldn't.
    pub failure: Option<String>,
}

/// Judges `passed` (the Candidates that passed the data-only rules), along with
/// any still queued from earlier Runs, and returns those that Fit.
pub fn judge(
    agent: &dyn Agent,
    profile: &Profile,
    store: &mut Store,
    passed: Vec<Candidate>,
    today: NaiveDate,
) -> anyhow::Result<Judged> {
    let considered = with_queued(passed, profile, store, today)?;

    let mut waiting = Vec::new();
    for c in &considered {
        if store.judgement(&c.source, &c.source_key)?.is_none() {
            waiting.push(c.clone());
        }
    }
    waiting.sort_by(|a, b| (a.start, &a.name).cmp(&(b.start, &b.name)));
    store.queue_unjudged(&waiting, today)?;

    let batch = &waiting[..waiting.len().min(MAX_CANDIDATES_PER_RUN)];
    let mut judged_now = 0;
    let mut failure = None;
    if !batch.is_empty() {
        let ids = agent::numbered_ids(batch.len());
        let request = JudgeRequest::new(batch, &ids, profile);
        let answer = agent
            .judge(&request)
            .map_err(|e| format!("{e:#}"))
            .and_then(|answer| request.read_judgements(&answer).map_err(|e| e.to_string()));
        match answer {
            Ok(judged) => {
                store.save_judgements(agent.name(), &judged, today)?;
                store.remove_unjudged(judged.iter().map(|(c, _)| *c))?;
                judged_now = judged.len();
            }
            Err(error) => {
                store.note_failed_judging(batch, &error)?;
                failure = Some(error);
            }
        }
    }

    let mut fitting = Vec::new();
    for c in considered {
        if store
            .judgement(&c.source, &c.source_key)?
            .is_some_and(|j| j.fits())
        {
            fitting.push(c);
        }
    }
    Ok(Judged {
        fitting,
        judged_now,
        unjudged: waiting.len() - judged_now,
        failure,
    })
}

/// Adds the Candidates queued by earlier Runs that this Run's Sources didn't
/// return, if they still pass the data-only rules; those that don't leave the queue.
fn with_queued(
    mut passed: Vec<Candidate>,
    profile: &Profile,
    store: &mut Store,
    today: NaiveDate,
) -> anyhow::Result<Vec<Candidate>> {
    let found: HashSet<(String, String)> = passed
        .iter()
        .map(|c| (c.source.clone(), c.source_key.clone()))
        .collect();
    let queued: Vec<Candidate> = store
        .unjudged()?
        .into_iter()
        .map(|u| u.candidate)
        .filter(|c| !found.contains(&(c.source.clone(), c.source_key.clone())))
        .collect();
    let still_passing = rules::apply(queued.clone(), profile, today);
    let dropped: Vec<&Candidate> = queued
        .iter()
        .filter(|c| !still_passing.contains(c))
        .collect();
    store.remove_unjudged(dropped)?;
    passed.extend(still_passing);
    Ok(passed)
}
