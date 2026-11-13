//! The last Run: when it ran, whether it succeeded and, if it failed, why;
//! and whether it is so old that scheduled Runs seem not to be firing.

use std::path::Path;

use chrono::{DateTime, Duration, Utc, Weekday};

use super::{Check, Section, error_line};
use crate::profile::Schedule;
use crate::store::RunRecord;

const S: Section = Section::LastRun;

pub(super) struct Context<'a> {
    pub now: DateTime<Utc>,
    pub schedule: Schedule,
    /// Whether a schedule is installed, when that could be told.
    pub installed: Option<bool>,
    pub db: &'a Path,
    pub log: &'a Path,
}

pub(super) fn check(
    last: anyhow::Result<Option<RunRecord>>,
    cx: &Context<'_>,
    checks: &mut Vec<Check>,
) {
    let db = cx.db.display();
    let run = match last {
        Ok(Some(run)) => run,
        Ok(None) => {
            checks.push(Check::warn(
                S,
                "last-run",
                "Last Run",
                format!("No Run is recorded in {db} yet."),
                "Run `hackathon-radar run` once, or wait for the schedule.",
            ));
            return;
        }
        Err(error) => {
            checks.push(Check::warn(
                S,
                "last-run",
                "Last Run",
                format!(
                    "Couldn't read the Run history in {db}: {}.",
                    error_line(&error, None)
                ),
                "Check that the database is readable, or pass --db <PATH>.",
            ));
            return;
        }
    };

    let when = format!(
        "{} ({})",
        run.finished_at.format("%Y-%m-%d %H:%M UTC"),
        ago(cx.now - run.finished_at)
    );
    checks.push(match &run.failure {
        None => Check::pass(
            S,
            "last-run",
            "Last Run",
            format!("The last Run finished {when} and succeeded."),
        ),
        Some(failure) => Check::warn(
            S,
            "last-run",
            "Last Run",
            format!("The last Run, {when}, failed: {failure}"),
            format!(
                "Fix what the checks above report, then run `hackathon-radar run`; scheduled \
                 Runs log to {}.",
                cx.log.display()
            ),
        ),
    });

    // Without a schedule, nothing is late.
    if cx.installed == Some(false) {
        return;
    }
    let gap = longest_gap_days(&cx.schedule);
    let limit = Duration::days(2 * gap);
    let age = cx.now - run.finished_at;
    checks.push(if age > limit {
        Check::warn(
            S,
            "last-run.age",
            "Run frequency",
            format!(
                "The last Run was {}, more than twice the longest gap between scheduled Runs \
                 ({gap} days), so scheduled Runs may not be firing: the Mac may have been off \
                 at Run time, or the schedule isn't loaded.",
                ago(age)
            ),
            format!(
                "Check `hackathon-radar schedule status` and the log at {}; Runs missed while \
                 the machine was shut down are skipped.",
                cx.log.display()
            ),
        )
    } else {
        Check::pass(
            S,
            "last-run.age",
            "Run frequency",
            format!(
                "The last Run is recent for a schedule that runs {}.",
                cx.schedule
            ),
        )
    });
}

/// The most days between two scheduled Runs, going round the week.
fn longest_gap_days(schedule: &Schedule) -> i64 {
    let mut days: Vec<i64> = schedule
        .days
        .iter()
        .map(|d: &Weekday| i64::from(d.num_days_from_monday()))
        .collect();
    days.sort_unstable();
    days.dedup();
    match days.as_slice() {
        [] | [_] => 7,
        _ => days
            .iter()
            .zip(days.iter().cycle().skip(1))
            .map(|(a, b)| (b - a).rem_euclid(7))
            .max()
            .unwrap_or(7),
    }
}

/// "3 days ago", "5 hours ago", "just now".
fn ago(age: Duration) -> String {
    if age < Duration::zero() {
        return "in the future: check this machine's clock".to_owned();
    }
    let (n, unit) = if age.num_days() >= 2 {
        (age.num_days(), "days")
    } else if age.num_hours() >= 1 {
        (
            age.num_hours(),
            if age.num_hours() == 1 {
                "hour"
            } else {
                "hours"
            },
        )
    } else if age.num_minutes() >= 1 {
        (
            age.num_minutes(),
            if age.num_minutes() == 1 {
                "minute"
            } else {
                "minutes"
            },
        )
    } else {
        return "just now".to_owned();
    };
    format!("{n} {unit} ago")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn on(days: &[Weekday]) -> Schedule {
        Schedule {
            days: days.to_vec(),
            time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        }
    }

    #[test]
    fn the_longest_gap_goes_round_the_week() {
        assert_eq!(longest_gap_days(&on(&[Weekday::Mon, Weekday::Thu])), 4);
        assert_eq!(longest_gap_days(&on(&[Weekday::Sun])), 7);
        assert_eq!(
            longest_gap_days(&on(&[
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun
            ])),
            1
        );
    }

    #[test]
    fn ages_read_naturally() {
        assert_eq!(ago(Duration::seconds(5)), "just now");
        assert_eq!(ago(Duration::hours(1)), "1 hour ago");
        assert_eq!(ago(Duration::hours(30)), "30 hours ago");
        assert_eq!(ago(Duration::days(9)), "9 days ago");
    }
}
