//! A Run: one pass that collects Candidates, decides which Hackathons Fit, and
//! sends at most one Alert. Everything it touches is passed in, so tests can
//! drive a whole Run with fakes.

use crate::alert::{self, Alert, AlertChannel};
use crate::candidate::Candidate;
use crate::clock::Clock;
use crate::profile::Profile;
use crate::rules;
use crate::sources::Source;
use crate::store::Store;

pub struct RunDeps<'a> {
    pub profile: &'a Profile,
    pub sources: &'a [Box<dyn Source>],
    pub clock: &'a dyn Clock,
    pub store: &'a mut Store,
    pub channel: &'a mut dyn AlertChannel,
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

    let fitting = rules::apply(candidates, deps.profile, today);
    deps.store.save_hackathons(&fitting, today)?;

    let alert = alert::compose(&fitting);
    let mut sent = false;
    if let Some(alert) = &alert
        && !options.dry_run
    {
        deps.channel.send(alert)?;
        sent = true;
    }

    Ok(RunOutcome {
        collected,
        fitting,
        failed_sources,
        alert,
        sent,
    })
}
