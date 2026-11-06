//! The Profile's settings for application deadlines: when a Deadline reminder
//! goes out, and how the Agent rechecks known Hackathons on each Run.
//!
//! They are read from the optional `[deadlines]` section:
//!
//! ```toml
//! [deadlines]
//! reminder_days = 3
//! rechecks_per_run = 10
//! recheck_every_days = 3
//! recheck_horizon_days = 120
//! ```
//!
//! Rechecks go to the Agent as one request per Run, held to the same step cap
//! as judging, `agent.max_turns` (ADR 0004). A recheck needs at least one step
//! (reading the Hackathon's page), so `rechecks_per_run` may not exceed
//! `agent.max_turns`; with the defaults, 10 rechecks of a page or two each fit
//! well within 50 steps.

use serde::Deserialize;

/// A Deadline reminder goes out this many days before the deadline.
pub const DEFAULT_REMINDER_DAYS: u32 = 3;
/// The most known Hackathons the Agent rechecks on one Run.
pub const DEFAULT_RECHECKS_PER_RUN: usize = 10;
/// A Hackathon checked fewer than this many days ago is not rechecked.
pub const DEFAULT_RECHECK_EVERY_DAYS: u32 = 3;
/// Only Hackathons starting within this many days are rechecked.
pub const DEFAULT_RECHECK_HORIZON_DAYS: u32 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeadlineSettings {
    /// How many days before an open Hackathon's deadline its Deadline reminder goes out.
    #[serde(default = "default_reminder_days")]
    pub reminder_days: u32,
    /// The most known Hackathons rechecked on one Run; 0 turns rechecks off.
    #[serde(default = "default_rechecks_per_run")]
    pub rechecks_per_run: usize,
    /// A Hackathon judged or rechecked fewer than this many days ago waits.
    #[serde(default = "default_recheck_every_days")]
    pub recheck_every_days: u32,
    /// Only Hackathons starting within this many days are rechecked.
    #[serde(default = "default_recheck_horizon_days")]
    pub recheck_horizon_days: u32,
}

fn default_reminder_days() -> u32 {
    DEFAULT_REMINDER_DAYS
}

fn default_rechecks_per_run() -> usize {
    DEFAULT_RECHECKS_PER_RUN
}

fn default_recheck_every_days() -> u32 {
    DEFAULT_RECHECK_EVERY_DAYS
}

fn default_recheck_horizon_days() -> u32 {
    DEFAULT_RECHECK_HORIZON_DAYS
}

impl Default for DeadlineSettings {
    fn default() -> Self {
        Self {
            reminder_days: DEFAULT_REMINDER_DAYS,
            rechecks_per_run: DEFAULT_RECHECKS_PER_RUN,
            recheck_every_days: DEFAULT_RECHECK_EVERY_DAYS,
            recheck_horizon_days: DEFAULT_RECHECK_HORIZON_DAYS,
        }
    }
}

impl DeadlineSettings {
    /// Adds a problem for each setting that can't work, given the step cap.
    pub(super) fn check(&self, max_turns: u32, problems: &mut Vec<String>) {
        if self.recheck_every_days == 0 {
            problems.push(
                "deadlines.recheck_every_days is 0; give how many days a Hackathon waits \
                 between rechecks, such as 3"
                    .to_owned(),
            );
        }
        if self.recheck_horizon_days == 0 {
            problems.push(
                "deadlines.recheck_horizon_days is 0; give how many days ahead Hackathons \
                 are rechecked, such as 120"
                    .to_owned(),
            );
        }
        if max_turns > 0 && self.rechecks_per_run > max_turns as usize {
            problems.push(format!(
                "deadlines.rechecks_per_run is {}, more than agent.max_turns ({max_turns}); \
                 each recheck takes at least one Agent step, so lower rechecks_per_run or \
                 raise agent.max_turns",
                self.rechecks_per_run
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::profile::Profile;

    const BASE: &str =
        "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n";

    fn parse(extra: &str) -> Result<Profile, String> {
        Profile::from_toml(&format!("{BASE}{extra}"), Path::new("p.toml"))
            .map_err(|e| e.to_string())
    }

    #[test]
    fn defaults_apply_without_the_section() {
        let profile = parse("").unwrap();
        assert_eq!(profile.deadlines, DeadlineSettings::default());
        assert_eq!(profile.deadlines.reminder_days, 3);
        assert_eq!(profile.deadlines.rechecks_per_run, 10);
        assert_eq!(profile.deadlines.recheck_every_days, 3);
        assert_eq!(profile.deadlines.recheck_horizon_days, 120);
    }

    #[test]
    fn reads_each_setting() {
        let profile = parse(
            "[deadlines]\nreminder_days = 5\nrechecks_per_run = 4\nrecheck_every_days = 7\n\
             recheck_horizon_days = 60\n",
        )
        .unwrap();
        assert_eq!(
            profile.deadlines,
            DeadlineSettings {
                reminder_days: 5,
                rechecks_per_run: 4,
                recheck_every_days: 7,
                recheck_horizon_days: 60,
            }
        );
    }

    #[test]
    fn the_example_profiles_deadlines_block_gives_the_defaults_once_uncommented() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/profile.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut uncommented = text.replace("# [deadlines]", "[deadlines]");
        for key in [
            "reminder_days",
            "rechecks_per_run",
            "recheck_every_days",
            "recheck_horizon_days",
        ] {
            uncommented = uncommented.replace(&format!("# {key} = "), &format!("{key} = "));
        }
        assert_ne!(uncommented, text);
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert_eq!(profile.deadlines, DeadlineSettings::default());
    }

    #[test]
    fn the_travel_funded_rule_is_off_unless_set() {
        assert!(!parse("").unwrap().travel.travel_funded_anywhere);
        let on = parse("[travel]\ntravel_funded_anywhere = true\n").unwrap();
        assert!(on.travel.travel_funded_anywhere);
    }

    #[test]
    fn refuses_settings_that_cannot_work() {
        let err =
            parse("[deadlines]\nrecheck_every_days = 0\nrecheck_horizon_days = 0\n").unwrap_err();
        assert!(err.contains("deadlines.recheck_every_days is 0"), "{err}");
        assert!(err.contains("deadlines.recheck_horizon_days is 0"), "{err}");

        let err = parse("[agent]\nmax_turns = 8\n").unwrap_err();
        assert!(
            err.contains("deadlines.rechecks_per_run is 10, more than agent.max_turns (8)"),
            "{err}"
        );
        assert!(parse("[agent]\nmax_turns = 8\n[deadlines]\nrechecks_per_run = 0\n").is_ok());

        let err = parse("[deadlines]\nreminder = 3\n").unwrap_err();
        assert!(err.contains("reminder"), "{err}");
    }
}
