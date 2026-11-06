//! How often Fares are looked up (ADR 0002), read from the optional `[fares]`
//! section. SerpApi's free plan allows 250 searches a month, so a Run only
//! looks up the Fares it needs, and keeps them for a while:
//!
//! ```toml
//! [fares]
//! horizon_weeks = 10    # look up Fares for Hackathons starting within 10 weeks
//! refresh_days = 14     # a Fare is looked up again after 14 days
//! lookups_per_run = 10  # at most 10 lookups on one Run
//! ```
//!
//! The Flight budget itself is `travel.flight_budget`, next to the Train limit.

use serde::Deserialize;

pub const DEFAULT_HORIZON_WEEKS: u32 = 10;
pub const DEFAULT_REFRESH_DAYS: u32 = 14;
pub const DEFAULT_LOOKUPS_PER_RUN: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FareSettings {
    /// Fares are looked up only for Hackathons starting within this many
    /// weeks; later flights Fit without a Fare until they are inside it.
    pub horizon_weeks: u32,
    /// A Fare older than this many days is looked up again.
    pub refresh_days: u32,
    /// The most Fare lookups one Run makes; the rest wait for the next Run.
    pub lookups_per_run: usize,
}

impl Default for FareSettings {
    fn default() -> Self {
        Self {
            horizon_weeks: DEFAULT_HORIZON_WEEKS,
            refresh_days: DEFAULT_REFRESH_DAYS,
            lookups_per_run: DEFAULT_LOOKUPS_PER_RUN,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct FaresFile {
    horizon_weeks: Option<u32>,
    refresh_days: Option<u32>,
    lookups_per_run: Option<usize>,
}

impl FareSettings {
    pub(super) fn from_file(file: FaresFile, problems: &mut Vec<String>) -> Self {
        let defaults = Self::default();
        let mut positive = |key: &str, value: Option<u32>, default: u32, example: &str| match value
        {
            None => default,
            Some(0) => {
                problems.push(format!(
                    "fares.{key} is 0; give at least 1, such as {example}"
                ));
                default
            }
            Some(n) => n,
        };
        let horizon_weeks = positive(
            "horizon_weeks",
            file.horizon_weeks,
            defaults.horizon_weeks,
            "10",
        );
        let refresh_days = positive(
            "refresh_days",
            file.refresh_days,
            defaults.refresh_days,
            "14",
        );
        let lookups_per_run = match file.lookups_per_run {
            None => defaults.lookups_per_run,
            Some(0) => {
                problems.push(
                    "fares.lookups_per_run is 0; give at least 1, such as 10, or flights never get a Fare"
                        .to_owned(),
                );
                defaults.lookups_per_run
            }
            Some(n) => n,
        };
        Self {
            horizon_weeks,
            refresh_days,
            lookups_per_run,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::profile::Profile;

    const HOME: &str =
        "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n";

    fn parse(extra: &str) -> Result<Profile, String> {
        Profile::from_toml(&format!("{HOME}{extra}"), Path::new("profile.toml"))
            .map_err(|e| e.to_string())
    }

    #[test]
    fn defaults_fit_the_serpapi_free_plan() {
        let profile = parse("").unwrap();
        assert_eq!(profile.fares, FareSettings::default());
        assert_eq!(profile.fares.horizon_weeks, 10);
        assert_eq!(profile.fares.refresh_days, 14);
        assert_eq!(profile.fares.lookups_per_run, 10);
        assert_eq!(profile.travel.flight_budget, None);
        assert_eq!(profile.currency.code(), "EUR");
        assert_eq!(profile.agent.fare_max_turns, 10);
    }

    #[test]
    fn reads_the_fare_settings_and_the_flight_budget() {
        let profile = Profile::from_toml(
            &format!(
                "currency = \" usd \"\n{HOME}[travel]\nflight_budget = 250\n\
                 [fares]\nhorizon_weeks = 6\nrefresh_days = 7\nlookups_per_run = 3\n\
                 [agent]\nfare_max_turns = 4\n"
            ),
            Path::new("profile.toml"),
        )
        .unwrap();
        assert_eq!(profile.currency.code(), "USD");
        assert_eq!(profile.travel.flight_budget, Some(250));
        assert_eq!(
            profile.fares,
            FareSettings {
                horizon_weeks: 6,
                refresh_days: 7,
                lookups_per_run: 3,
            }
        );
        assert_eq!(profile.agent.fare_max_turns, 4);
    }

    #[test]
    fn the_example_profiles_fare_settings_are_valid_once_uncommented() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/profile.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        let uncommented = text
            .replace("# currency = ", "currency = ")
            .replace("# [travel]", "[travel]")
            .replace("# flight_budget = ", "flight_budget = ")
            .replace("# [fares]", "[fares]")
            .replace("# horizon_weeks = ", "horizon_weeks = ")
            .replace("# refresh_days = ", "refresh_days = ")
            .replace("# lookups_per_run = ", "lookups_per_run = ")
            .replace("# fare_max_turns = ", "fare_max_turns = ");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert_eq!(profile.currency.code(), "EUR");
        assert_eq!(profile.travel.flight_budget, Some(200));
        assert_eq!(profile.fares, FareSettings::default());
        assert_eq!(profile.agent.fare_max_turns, 10);
    }

    #[test]
    fn zeroes_and_bad_currencies_are_refused() {
        let err = Profile::from_toml(
            &format!(
                "currency = \"euro\"\n{HOME}[travel]\nflight_budget = 0\n\
                 [fares]\nhorizon_weeks = 0\nrefresh_days = 0\nlookups_per_run = 0\n\
                 [agent]\nfare_max_turns = 0\n"
            ),
            Path::new("profile.toml"),
        )
        .unwrap_err()
        .to_string();
        for problem in [
            "currency: \"euro\" is not a currency code",
            "agent.fare_max_turns is 0",
            "travel.flight_budget is 0",
            "fares.horizon_weeks is 0",
            "fares.refresh_days is 0",
            "fares.lookups_per_run is 0",
        ] {
            assert!(err.contains(problem), "{problem:?} missing from {err}");
        }
    }
}
