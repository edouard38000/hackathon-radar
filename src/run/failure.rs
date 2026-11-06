//! Failed Runs (ticket 10): what makes a Run fail, and when a failing Run sends
//! the "needs attention" Alert.
//!
//! A Run fails when any of these happen:
//! - the Agent step failed: judging, or the question about Duplicate listings,
//!   errored, used all its steps or answered something that doesn't match the schema;
//! - every Source failed, so the Run collected nothing (a Run with no Sources
//!   at all doesn't count as that);
//! - the Alert channel failed to send the Run's Alert;
//! - an unexpected error stopped the Run, such as the database failing.
//!
//! Failed travel estimates don't fail a Run: those cities only go without a
//! trip label until a later Run estimates them. A dry run is never recorded and
//! never counts, either way.
//!
//! Nothing a failed Run collected is lost: its listings are stored and its
//! unjudged Candidates stay queued for the next Run ([`crate::judging`]).

use crate::alert::{Alert, attention};
use crate::store::Store;

use super::SourceFailure;

/// How many failed Runs in a row send the "needs attention" Alert.
pub const FAILED_RUNS_BEFORE_ATTENTION: usize = 2;

/// Why the Run failed before sending its Alert, in one line, if it did.
pub fn before_sending(
    agent_failure: Option<&str>,
    failed_sources: &[SourceFailure],
    sources: usize,
) -> Option<String> {
    let mut reasons = Vec::new();
    if let Some(error) = agent_failure {
        reasons.push(format!("the Agent failed: {error}"));
    }
    if let Some(first) = failed_sources.first()
        && failed_sources.len() == sources
    {
        reasons.push(format!(
            "every Source failed ({}: {})",
            first.source, first.error
        ));
    }
    joined(reasons)
}

/// Adds the Alert channel failing to why the Run failed.
pub fn with_alert_failure(failure: Option<String>, alert_failure: Option<&str>) -> Option<String> {
    let reasons = failure
        .into_iter()
        .chain(alert_failure.map(|e| format!("the Alert channel failed: {e}")));
    joined(reasons.collect())
}

/// The "needs attention" Alert, when this failing Run makes
/// [`FAILED_RUNS_BEFORE_ATTENTION`] or more failed Runs in a row and none of
/// them has got it to the person yet. A Run that succeeds starts the count again.
/// An Alert channel that reaches no one (printing, when the Profile has none)
/// doesn't get it to the person, so it's printed again on each failed Run.
pub fn attention_due(store: &Store, failure: &str) -> anyhow::Result<Option<Alert>> {
    let streak = store.failed_streak()?;
    let failed_runs = streak.runs + 1;
    Ok(
        (failed_runs >= FAILED_RUNS_BEFORE_ATTENTION && !streak.attention_sent)
            .then(|| attention::needs_attention(failed_runs, failure)),
    )
}

fn joined(reasons: Vec<String>) -> Option<String> {
    (!reasons.is_empty()).then(|| attention::one_line(&reasons.join("; ")))
}
