//! The Profile's settings for getting from Home to a Hackathon: the Train limit,
//! the Flight budget, and the person's corrections to the Agent's travel
//! estimates (ADR 0002).
//!
//! They are read from the optional `[travel]` section:
//!
//! ```toml
//! [travel]
//! train_limit_hours = 4
//! travel_funded_anywhere = true
//! flight_budget = 200   # in the Profile's currency
//!
//! [[travel.overrides]]
//! city = "Munich"
//! train_hours = 5.5
//! airport = "MUC"
//! ```

use serde::Deserialize;

/// The Train limit when the Profile doesn't set one: four hours.
pub const DEFAULT_TRAIN_LIMIT_MINUTES: u32 = 4 * 60;

/// No train journey, and no Train limit, is longer than this many hours.
const MAX_HOURS: f64 = 48.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TravelSettings {
    /// The Train limit, in minutes: the longest train journey from Home that
    /// still counts as going by train.
    pub train_limit_minutes: u32,
    /// The Flight budget: the most the person will pay for a return flight, in
    /// whole units of the Profile's currency. Without one, Fares aren't looked
    /// up and flights are left to the other rules.
    pub flight_budget: Option<u32>,
    /// The person's corrections, which always win over the Agent's estimates.
    pub overrides: Vec<TravelOverride>,
    /// The Travel-funded rule: whether a Travel-funded Hackathon counts
    /// anywhere in the world, regardless of Countries and the Flight budget. Off unless the Profile
    /// sets `travel_funded_anywhere = true`, because it sends listings from
    /// every country to the Agent.
    pub travel_funded_anywhere: bool,
}

impl Default for TravelSettings {
    fn default() -> Self {
        Self {
            train_limit_minutes: DEFAULT_TRAIN_LIMIT_MINUTES,
            flight_budget: None,
            overrides: Vec::new(),
            travel_funded_anywhere: false,
        }
    }
}

impl TravelSettings {
    /// The override for a destination city, matched without regard to case.
    pub fn override_for(&self, city: &str) -> Option<&TravelOverride> {
        let city = city.trim().to_lowercase();
        self.overrides
            .iter()
            .find(|o| o.city.to_lowercase() == city)
    }
}

/// A correction for one destination city. Whatever it leaves unset still comes
/// from the Agent's estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TravelOverride {
    pub city: String,
    /// The fastest train journey from Home, in minutes.
    pub train_minutes: Option<u32>,
    pub flight: Option<FlightOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlightOverride {
    /// Fly to this airport (an upper-cased IATA code).
    Airport(String),
    /// There is no practical flight to the city.
    NoPracticalFlight,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct TravelFile {
    train_limit_hours: Option<f64>,
    flight_budget: Option<u32>,
    #[serde(default)]
    overrides: Vec<OverrideFile>,
    #[serde(default)]
    travel_funded_anywhere: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideFile {
    city: String,
    train_hours: Option<f64>,
    airport: Option<String>,
    #[serde(default)]
    no_practical_flight: bool,
}

impl TravelSettings {
    pub(super) fn from_file(file: TravelFile, problems: &mut Vec<String>) -> Self {
        let train_limit_minutes = match file.train_limit_hours {
            None => DEFAULT_TRAIN_LIMIT_MINUTES,
            Some(hours) => minutes(hours).unwrap_or_else(|| {
                problems.push(format!(
                    "travel.train_limit_hours is {hours}; give the longest train journey you'd take, \
                     in hours, such as 4"
                ));
                DEFAULT_TRAIN_LIMIT_MINUTES
            }),
        };

        let flight_budget = match file.flight_budget {
            Some(0) => {
                problems.push(
                    "travel.flight_budget is 0; give the most you'd pay for a return flight, such as 200"
                        .to_owned(),
                );
                None
            }
            budget => budget,
        };

        let mut overrides: Vec<TravelOverride> = Vec::new();
        for file in file.overrides {
            let city = file.city.trim().to_owned();
            if city.is_empty() {
                problems.push("travel.overrides has an entry with an empty city".to_owned());
                continue;
            }
            let at = format!("travel.overrides for {city:?}");
            if overrides
                .iter()
                .any(|o| o.city.to_lowercase() == city.to_lowercase())
            {
                problems.push(format!("{at} is given twice; keep one"));
                continue;
            }

            let train_minutes = file.train_hours.and_then(|hours| {
                let found = minutes(hours);
                if found.is_none() {
                    problems.push(format!(
                        "{at}: train_hours is {hours}; give the train journey in hours, such as 2.5"
                    ));
                }
                found
            });

            let airport = file
                .airport
                .as_deref()
                .map(|a| a.trim().to_ascii_uppercase());
            let flight = match (airport, file.no_practical_flight) {
                (Some(_), true) => {
                    problems.push(format!(
                        "{at} gives an airport and no_practical_flight; keep one"
                    ));
                    None
                }
                (Some(code), false) if is_iata(&code) => Some(FlightOverride::Airport(code)),
                (Some(code), false) => {
                    problems.push(format!(
                        "{at}: airport {code:?} is not a 3-letter IATA airport code"
                    ));
                    None
                }
                (None, true) => Some(FlightOverride::NoPracticalFlight),
                (None, false) => None,
            };

            if file.train_hours.is_none() && file.airport.is_none() && !file.no_practical_flight {
                problems.push(format!(
                    "{at} sets nothing; give train_hours, airport or no_practical_flight = true"
                ));
            }
            overrides.push(TravelOverride {
                city,
                train_minutes,
                flight,
            });
        }

        TravelSettings {
            train_limit_minutes,
            flight_budget,
            overrides,
            travel_funded_anywhere: file.travel_funded_anywhere,
        }
    }
}

/// A positive number of hours, up to [`MAX_HOURS`], in whole minutes.
fn minutes(hours: f64) -> Option<u32> {
    (hours.is_finite() && hours > 0.0 && hours <= MAX_HOURS)
        .then(|| ((hours * 60.0).round() as u32).max(1))
}

fn is_iata(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::profile::{Profile, ProfileError};

    const HOME: &str =
        "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n";

    fn parse(travel: &str) -> Result<Profile, ProfileError> {
        Profile::from_toml(&format!("{HOME}{travel}"), Path::new("profile.toml"))
    }

    #[test]
    fn the_train_limit_defaults_to_four_hours_with_no_overrides() {
        let profile = parse("").unwrap();
        assert_eq!(profile.travel, TravelSettings::default());
        assert_eq!(profile.travel.train_limit_minutes, 240);
        assert!(profile.travel.overrides.is_empty());
    }

    #[test]
    fn reads_the_train_limit_in_whole_or_fractional_hours() {
        let whole = parse("[travel]\ntrain_limit_hours = 3\n").unwrap();
        assert_eq!(whole.travel.train_limit_minutes, 180);
        let fractional = parse("[travel]\ntrain_limit_hours = 2.5\n").unwrap();
        assert_eq!(fractional.travel.train_limit_minutes, 150);
    }

    #[test]
    fn reads_overrides() {
        let profile = parse(
            r#"
            [travel]
            [[travel.overrides]]
            city = " Munich "
            train_hours = 5.5
            airport = "muc"

            [[travel.overrides]]
            city = "Luxembourg"
            no_practical_flight = true

            [[travel.overrides]]
            city = "Paris"
            train_hours = 1.4
            "#,
        )
        .unwrap();
        assert_eq!(
            profile.travel.overrides,
            [
                TravelOverride {
                    city: "Munich".into(),
                    train_minutes: Some(330),
                    flight: Some(FlightOverride::Airport("MUC".into())),
                },
                TravelOverride {
                    city: "Luxembourg".into(),
                    train_minutes: None,
                    flight: Some(FlightOverride::NoPracticalFlight),
                },
                TravelOverride {
                    city: "Paris".into(),
                    train_minutes: Some(84),
                    flight: None,
                },
            ]
        );
        assert_eq!(
            profile.travel.override_for("munich").map(|o| &o.city[..]),
            Some("Munich")
        );
        assert!(profile.travel.override_for("Berlin").is_none());
    }

    #[test]
    fn a_bad_train_limit_is_reported() {
        for bad in ["0", "-2", "49", "nan"] {
            let err = parse(&format!("[travel]\ntrain_limit_hours = {bad}\n")).unwrap_err();
            assert!(
                err.to_string().contains("travel.train_limit_hours is"),
                "{bad}: {err}"
            );
        }
        let text = parse("[travel]\ntrain_limit_hours = \"4h\"\n").unwrap_err();
        assert!(matches!(text, ProfileError::Syntax { .. }), "{text}");
    }

    #[test]
    fn bad_overrides_are_all_reported() {
        let err = parse(
            r#"
            [[travel.overrides]]
            city = " "
            train_hours = 2

            [[travel.overrides]]
            city = "Munich"

            [[travel.overrides]]
            city = "Lyon"
            airport = "LYSX"
            train_hours = 0

            [[travel.overrides]]
            city = "Geneva"
            airport = "GVA"
            no_practical_flight = true

            [[travel.overrides]]
            city = "lyon"
            train_hours = 4
            "#,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - travel.overrides has an entry with an empty city\n\
             \x20 - travel.overrides for \"Munich\" sets nothing; give train_hours, airport or no_practical_flight = true\n\
             \x20 - travel.overrides for \"Lyon\": train_hours is 0; give the train journey in hours, such as 2.5\n\
             \x20 - travel.overrides for \"Lyon\": airport \"LYSX\" is not a 3-letter IATA airport code\n\
             \x20 - travel.overrides for \"Geneva\" gives an airport and no_practical_flight; keep one\n\
             \x20 - travel.overrides for \"lyon\" is given twice; keep one"
        );
    }

    #[test]
    fn unknown_travel_keys_are_refused() {
        let err = parse("[travel]\ntrain_limit = 4\n").unwrap_err();
        assert!(err.to_string().contains("train_limit"), "{err}");
        let err = parse("[[travel.overrides]]\ncity = \"Paris\"\nflight = \"CDG\"\n").unwrap_err();
        assert!(err.to_string().contains("flight"), "{err}");
    }

    #[test]
    fn the_example_profiles_travel_block_is_valid_once_uncommented() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/profile.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        let uncommented = text
            .replace("# [travel]", "[travel]")
            .replace("# train_limit_hours = ", "train_limit_hours = ")
            .replace("# [[travel.overrides]]", "[[travel.overrides]]")
            .replace("# city = ", "city = ")
            .replace("# train_hours = ", "train_hours = ")
            .replace("# airport = ", "airport = ")
            .replace("# travel_max_turns = ", "travel_max_turns = ");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert_eq!(profile.agent.travel_max_turns, 15);
        assert_eq!(profile.home.country.as_deref(), Some("Belgium"));
        assert_eq!(profile.travel.train_limit_minutes, 240);
        assert_eq!(
            profile.travel.overrides,
            [TravelOverride {
                city: "Munich".into(),
                train_minutes: Some(330),
                flight: Some(FlightOverride::Airport("MUC".into())),
            }]
        );
    }

    #[test]
    fn home_may_name_its_country() {
        let profile = Profile::from_toml(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\ncountry = \" Belgium \"\nairports = [\"BRU\"]\n",
            Path::new("profile.toml"),
        )
        .unwrap();
        assert_eq!(profile.home.country.as_deref(), Some("Belgium"));
        assert_eq!(parse("").unwrap().home.country, None);

        let empty = Profile::from_toml(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\ncountry = \"\"\nairports = [\"BRU\"]\n",
            Path::new("profile.toml"),
        )
        .unwrap_err();
        assert!(
            empty.to_string().contains("home.country is empty"),
            "{empty}"
        );
    }
}
