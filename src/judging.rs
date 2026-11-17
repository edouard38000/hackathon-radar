//! The Run step that hands Candidates to the Agent (ADR 0002).
//!
//! Candidates that passed the data-only rules and have no judgement yet are
//! queued in the store, then sent to the Agent. Its answer is checked against
//! the schema and stored. When the Agent fails, or its answer doesn't match the
//! schema, the Candidates stay queued and go to the Agent again on the next Run;
//! the Run itself carries on.
//!
//! Each Run sends one batch. A share of it is kept for Candidates outside
//! Countries (which only Fit when Travel-funded), so a long queue in Countries
//! can't hold them back Run after Run. Within each group the soonest go first,
//! and a group's unused share goes to the other. Candidates whose start date is
//! past leave the queue unjudged: by the time an Alert told the person, it
//! would be too late to go.

use std::collections::HashSet;

use chrono::NaiveDate;

use crate::agent::{self, Agent, JudgeRequest};
use crate::candidate::{Candidate, Dates};
use crate::profile::Profile;
use crate::rules;
use crate::store::Store;

/// The most Candidates sent to the Agent on one Run, so one request fits
/// within the Profile's step cap (ADR 0004). The rest wait for the next Run.
pub const MAX_CANDIDATES_PER_RUN: usize = 20;

/// The part of each batch kept for Candidates outside Countries while any
/// wait: a quarter. Most of the batch stays with those in Countries, which Fit
/// far more often; a quarter still gets through the soonest of those outside on
/// every Run.
pub const OUTSIDE_COUNTRIES_SHARE: usize = MAX_CANDIDATES_PER_RUN / 4;

/// A count of Candidates in the Profile's Countries and outside them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ByCountries {
    pub in_countries: usize,
    pub outside: usize,
}

impl ByCountries {
    pub fn total(self) -> usize {
        self.in_countries + self.outside
    }
}

/// How many Candidates the Agent judged on a Run and how many still wait, in
/// Countries and outside them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub judged: ByCountries,
    pub waiting: ByCountries,
}

impl Tally {
    /// The line `run` prints about judging.
    pub fn summary(&self) -> String {
        format!(
            "The Agent judged {} Candidates ({} in Countries, {} outside); \
             {} wait for the next Run ({} in Countries, {} outside).",
            self.judged.total(),
            self.judged.in_countries,
            self.judged.outside,
            self.waiting.total(),
            self.waiting.in_countries,
            self.waiting.outside,
        )
    }
}

#[derive(Debug)]
pub struct Judged {
    /// The Candidates the Agent judged to be Hackathons the person is eligible for.
    pub fitting: Vec<Candidate>,
    /// How many Candidates the Agent judged on this Run, and how many still
    /// wait for a judgement.
    pub tally: Tally,
    /// Why the Agent couldn't judge this Run, if it couldn't.
    pub failure: Option<String>,
    /// How many of the Agent's Theme answers were normalised rather than
    /// taken as given ([`agent::ReadAnswer`]).
    pub themes_normalised: usize,
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

    let mut in_countries = Vec::new();
    let mut outside = Vec::new();
    let mut started = Vec::new();
    for c in &considered {
        if store.judgement(&c.source, &c.source_key)?.is_some() {
            continue;
        }
        if has_started(c, today) {
            started.push(c);
        } else if rules::in_countries(c, profile) {
            in_countries.push(c.clone());
        } else {
            outside.push(c.clone());
        }
    }
    // Too late to be any use: never sent to the Agent, and off the queue if queued.
    store.remove_unjudged(started)?;
    for group in [&mut in_countries, &mut outside] {
        group.sort_by(|a, b| (a.start, &a.name).cmp(&(b.start, &b.name)));
        store.queue_unjudged(group, today)?;
    }

    let (from_countries, from_outside) = batch_sizes(in_countries.len(), outside.len());
    let batch: Vec<Candidate> = in_countries[..from_countries]
        .iter()
        .chain(&outside[..from_outside])
        .cloned()
        .collect();
    let mut judged = ByCountries::default();
    let mut failure = None;
    let mut themes_normalised = 0;
    if !batch.is_empty() {
        let ids = agent::numbered_ids(batch.len());
        let request = JudgeRequest::new(&batch, &ids, profile);
        let answer = agent
            .judge(&request)
            .map_err(|e| format!("{e:#}"))
            .and_then(|answer| request.read_answer(&answer).map_err(|e| e.to_string()));
        match answer {
            Ok(read) => {
                themes_normalised = read.themes_normalised;
                let answered = read.judgements;
                store.save_judgements(agent.name(), &answered, today)?;
                store.remove_unjudged(answered.iter().map(|(c, _)| *c))?;
                for (c, _) in &answered {
                    if rules::in_countries(c, profile) {
                        judged.in_countries += 1;
                    } else {
                        judged.outside += 1;
                    }
                }
            }
            Err(error) => {
                store.note_failed_judging(&batch, &error)?;
                failure = Some(error);
            }
        }
    }

    let mut fitting = Vec::new();
    for c in considered {
        if store
            .judgement(&c.source, &c.source_key)?
            .is_some_and(|j| rules::fits(&c, &j, profile, today))
        {
            fitting.push(c);
        }
    }
    Ok(Judged {
        fitting,
        tally: Tally {
            judged,
            waiting: ByCountries {
                in_countries: in_countries.len() - judged.in_countries,
                outside: outside.len() - judged.outside,
            },
        },
        failure,
        themes_normalised,
    })
}

/// How many of the waiting Candidates in Countries and outside them go in this
/// Run's batch: those outside get up to [`OUTSIDE_COUNTRIES_SHARE`], those in
/// Countries the rest, and either group takes what the other doesn't use.
fn batch_sizes(in_countries: usize, outside: usize) -> (usize, usize) {
    let unused_in_countries = MAX_CANDIDATES_PER_RUN.saturating_sub(in_countries);
    let from_outside = outside.min(OUTSIDE_COUNTRIES_SHARE.max(unused_in_countries));
    let from_countries = in_countries.min(MAX_CANDIDATES_PER_RUN - from_outside);
    (from_countries, from_outside)
}

/// Whether the Hackathon's start date is past. One that starts today is still
/// judged, as a Run's Alert can reach the person in time for a local one. A
/// submission period (Devpost) opens before the event, so its start doesn't
/// say the event has started.
fn has_started(candidate: &Candidate, today: NaiveDate) -> bool {
    candidate.dates() == Dates::EventDays && candidate.start < today
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_of_the_batch_is_kept_for_candidates_outside_countries() {
        assert_eq!(OUTSIDE_COUNTRIES_SHARE, 5);
        assert_eq!(batch_sizes(146, 120), (15, 5));
        assert_eq!(batch_sizes(20, 20), (15, 5));
    }

    #[test]
    fn a_group_s_unused_share_goes_to_the_other() {
        assert_eq!(batch_sizes(25, 0), (20, 0));
        assert_eq!(batch_sizes(25, 2), (18, 2));
        assert_eq!(batch_sizes(10, 30), (10, 10));
        assert_eq!(batch_sizes(0, 30), (0, 20));
        assert_eq!(batch_sizes(3, 4), (3, 4));
        assert_eq!(batch_sizes(0, 0), (0, 0));
    }

    #[test]
    fn the_summary_splits_judged_and_waiting_by_countries() {
        let tally = Tally {
            judged: ByCountries {
                in_countries: 15,
                outside: 5,
            },
            waiting: ByCountries {
                in_countries: 30,
                outside: 116,
            },
        };
        assert_eq!(
            tally.summary(),
            "The Agent judged 20 Candidates (15 in Countries, 5 outside); \
             146 wait for the next Run (30 in Countries, 116 outside)."
        );
    }
}
