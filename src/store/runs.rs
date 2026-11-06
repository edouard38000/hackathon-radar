//! Run history (ticket 10): one row per Run that wasn't a dry run.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::params;

use super::Store;

/// One Run as recorded in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    /// The day the Run was for, from its clock.
    pub day: NaiveDate,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    /// Why the Run failed, in one line; `None` when it succeeded.
    pub failure: Option<String>,
    pub counts: RunCounts,
    /// Whether the Run's Alert carried the "needs attention" part to the person.
    pub attention_sent: bool,
}

impl RunRecord {
    pub fn succeeded(&self) -> bool {
        self.failure.is_none()
    }
}

/// What one Run did, as counted in the history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunCounts {
    /// Candidates the Sources returned, before any rule.
    pub collected: usize,
    /// Candidates the Agent judged on this Run.
    pub judged: usize,
    /// Candidates still waiting for the Agent after this Run.
    pub unjudged: usize,
    /// Hackathons that Fit on this Run.
    pub fit: usize,
    /// Hackathons this Run's Alert Announced.
    pub announced: usize,
    /// Whether an Alert went to the Alert channel.
    pub sent: bool,
    /// Sources that couldn't be read.
    pub failed_sources: usize,
}

/// The failed Runs since the last one that succeeded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FailedStreak {
    /// How many Runs in a row have failed.
    pub runs: usize,
    /// Whether one of them already sent the "needs attention" Alert to the person.
    pub attention_sent: bool,
}

impl Store {
    /// Adds a Run to the history.
    pub fn record_run(&mut self, run: &RunRecord) -> anyhow::Result<()> {
        let c = &run.counts;
        let n = |count: usize| i64::try_from(count);
        self.conn.execute(
            "INSERT INTO runs
                (day, started_at, finished_at, succeeded, failure, collected, judged, unjudged,
                 fit, announced, sent, failed_sources, attention_sent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                run.day,
                run.started_at,
                run.finished_at,
                run.succeeded(),
                run.failure,
                n(c.collected)?,
                n(c.judged)?,
                n(c.unjudged)?,
                n(c.fit)?,
                n(c.announced)?,
                c.sent,
                n(c.failed_sources)?,
                run.attention_sent,
            ],
        )?;
        Ok(())
    }

    /// Every recorded Run, oldest first.
    pub fn runs(&self) -> anyhow::Result<Vec<RunRecord>> {
        let mut query = self.conn.prepare(
            "SELECT day, started_at, finished_at, failure, collected, judged, unjudged, fit,
                    announced, sent, failed_sources, attention_sent
             FROM runs
             ORDER BY id",
        )?;
        let rows = query.query_map([], |row| {
            Ok(RunRecord {
                day: row.get(0)?,
                started_at: row.get(1)?,
                finished_at: row.get(2)?,
                failure: row.get(3)?,
                counts: RunCounts {
                    collected: count(row, 4)?,
                    judged: count(row, 5)?,
                    unjudged: count(row, 6)?,
                    fit: count(row, 7)?,
                    announced: count(row, 8)?,
                    sent: row.get(9)?,
                    failed_sources: count(row, 10)?,
                },
                attention_sent: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The failed Runs recorded since the last Run that succeeded.
    pub fn failed_streak(&self) -> anyhow::Result<FailedStreak> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(attention_sent), 0)
             FROM runs
             WHERE succeeded = 0
               AND id > COALESCE((SELECT MAX(id) FROM runs WHERE succeeded = 1), 0)",
            [],
            |row| {
                Ok(FailedStreak {
                    runs: count(row, 0)?,
                    attention_sent: row.get(1)?,
                })
            },
        )?)
    }
}

/// Reads a count stored as an SQLite integer.
fn count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<usize> {
    let stored: u32 = row.get(index)?;
    Ok(stored as usize)
}
