//! Getting there: how a person gets from Home to each Hackathon that Fits them,
//! locally, by train within the Train limit, or by flight to its nearest
//! served airport (ADR 0002).
//!
//! The Agent estimates the train time and the nearest airport once per
//! destination city ([`crate::agent::travel`]); the answers are cached per Home
//! city and destination city in the store, so later Runs never ask about a
//! cached city again. The Profile's overrides always win over the cache. A
//! malformed answer is rejected and its cities are asked about on the next Run.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::agent::travel::{Destination, Estimate, TravelRequest};
use crate::agent::{self, Agent};
use crate::candidate::Candidate;
use crate::profile::{FlightOverride, Profile, TravelOverride};
use crate::store::Store;

/// The most destination cities sent to the Agent on one Run; the rest wait for the next Run.
pub const MAX_DESTINATIONS_PER_RUN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripKind {
    /// In Home's city, or in Home's country within the Train limit.
    Local,
    /// Within the Train limit.
    Train,
    /// Over the Train limit, or with no practical train.
    Flight,
}

impl TripKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TripKind::Local => "local",
            TripKind::Train => "train",
            TripKind::Flight => "flight",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(TripKind::Local),
            "train" => Some(TripKind::Train),
            "flight" => Some(TripKind::Flight),
            _ => None,
        }
    }
}

/// How the person gets to one Hackathon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trip {
    pub kind: TripKind,
    /// The train journey from Home, in minutes, when known.
    pub train_minutes: Option<u32>,
    /// For a flight, the destination airport; `None` when there is no practical flight.
    pub airport: Option<String>,
    /// As shown in the Alert entry, such as `train ~2h` or `fly BRU→MUC`.
    pub label: String,
}

/// What this Run worked out about getting to the Hackathons that Fit.
#[derive(Debug, Default)]
pub struct Travelled {
    /// Trips by listing (source, source key). A Hackathon with no city, or
    /// whose city is still waiting for an estimate, has none.
    pub trips: HashMap<(String, String), Trip>,
    /// How many destination cities the Agent estimated on this Run.
    pub estimated_now: usize,
    /// How many destination cities still wait for an estimate.
    pub unestimated: usize,
    /// Why the Agent's travel estimates failed this Run, if they did.
    pub failure: Option<String>,
}

impl Travelled {
    pub fn trip(&self, hackathon: &Candidate) -> Option<&Trip> {
        self.trips
            .get(&(hackathon.source.clone(), hackathon.source_key.clone()))
    }
}

/// Works out the Trip to each of `fitting`, first asking the Agent about the
/// cities that have neither a cached estimate nor an override that settles
/// the Trip. With `ask_agent` false (the Agent already failed this Run), no
/// one is asked and those cities wait for the next Run. The Trips are stored.
pub fn plan(
    agent: &dyn Agent,
    profile: &Profile,
    store: &mut Store,
    fitting: &[Candidate],
    today: NaiveDate,
    ask_agent: bool,
) -> anyhow::Result<Travelled> {
    let home = &profile.home.city;
    let mut waiting: Vec<Destination> = Vec::new();
    let mut seen = HashSet::new();
    for hackathon in fitting {
        let Some(destination) = destination(hackathon) else {
            continue;
        };
        if same_place(&destination.city, home)
            || !seen.insert((key(&destination.city), key(&destination.country)))
        {
            continue;
        }
        let over = profile.travel.override_for(&destination.city);
        if store.travel_estimate(home, &destination)?.is_none()
            && trip(profile, &destination, over, None).is_none()
        {
            waiting.push(destination);
        }
    }

    let mut estimated_now = 0;
    let mut failure = None;
    let batch = &waiting[..waiting.len().min(MAX_DESTINATIONS_PER_RUN)];
    if ask_agent && !batch.is_empty() {
        let ids = agent::numbered_ids(batch.len());
        let request = TravelRequest::new(batch, &ids, profile);
        let answer = agent
            .judge(&request.agent_request())
            .map_err(|e| format!("{e:#}"))
            .and_then(|answer| request.read_estimates(&answer).map_err(|e| e.to_string()));
        match answer {
            Ok(estimates) => {
                store.save_travel_estimates(home, agent.name(), &estimates, today)?;
                estimated_now = estimates.len();
            }
            Err(error) => failure = Some(error),
        }
    }

    let mut trips = HashMap::new();
    let mut stored = Vec::new();
    for hackathon in fitting {
        let found = match destination(hackathon) {
            Some(destination) => {
                let cached = store.travel_estimate(home, &destination)?;
                let over = profile.travel.override_for(&destination.city);
                trip(profile, &destination, over, cached.as_ref())
            }
            None => None,
        };
        if let Some(found) = &found {
            trips.insert(
                (hackathon.source.clone(), hackathon.source_key.clone()),
                found.clone(),
            );
        }
        stored.push((hackathon, found));
    }
    store.save_trips(&stored)?;

    Ok(Travelled {
        trips,
        estimated_now,
        unestimated: waiting.len() - estimated_now,
        failure,
    })
}

/// Normalises a city or country name for comparing and caching.
pub fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn same_place(a: &str, b: &str) -> bool {
    key(a) == key(b)
}

fn destination(hackathon: &Candidate) -> Option<Destination> {
    let city = hackathon.city.as_deref().map(str::trim)?;
    let country = hackathon.country.as_deref().map(str::trim)?;
    (!city.is_empty()).then(|| Destination {
        city: city.to_owned(),
        country: country.to_owned(),
    })
}

/// The Trip to `destination`, from the override where it says something and
/// the cached estimate otherwise, or `None` when neither settles it yet.
pub fn trip(
    profile: &Profile,
    destination: &Destination,
    over: Option<&TravelOverride>,
    cached: Option<&Estimate>,
) -> Option<Trip> {
    let home = &profile.home;
    if same_place(&destination.city, &home.city) {
        return Some(Trip {
            kind: TripKind::Local,
            train_minutes: None,
            airport: None,
            label: "local".to_owned(),
        });
    }

    // Outer `None`: not known yet. Inner `None`: no practical train or flight.
    let train: Option<Option<u32>> = over
        .and_then(|o| o.train_minutes)
        .map(Some)
        .or_else(|| cached.map(|c| c.train_minutes));
    let flight: Option<Option<String>> = over
        .and_then(|o| o.flight.as_ref())
        .map(|f| match f {
            FlightOverride::Airport(code) => Some(code.clone()),
            FlightOverride::NoPracticalFlight => None,
        })
        .or_else(|| cached.map(|c| c.airport.clone()));

    let limit = profile.travel.train_limit_minutes;
    if let Some(Some(minutes)) = train
        && minutes <= limit
    {
        let in_home_country = home
            .country
            .as_deref()
            .is_some_and(|c| same_place(c, &destination.country));
        let (kind, label) = if in_home_country {
            (
                TripKind::Local,
                format!("local, train {}", duration(minutes)),
            )
        } else {
            (TripKind::Train, format!("train {}", duration(minutes)))
        };
        return Some(Trip {
            kind,
            train_minutes: Some(minutes),
            airport: None,
            label,
        });
    }

    let (train, airport) = (train?, flight?);
    let label = match (&airport, train) {
        (Some(code), _) => format!("fly {}→{code}", home.airports[0]),
        (None, Some(minutes)) => format!("no practical flight (train {})", duration(minutes)),
        (None, None) => "no practical train or flight".to_owned(),
    };
    Some(Trip {
        kind: TripKind::Flight,
        train_minutes: train,
        airport,
        label,
    })
}

/// `~45min`, `~2h` or `~1h30`, to the nearest quarter hour.
fn duration(minutes: u32) -> String {
    let rounded = ((minutes + 7) / 15 * 15).max(15);
    match (rounded / 60, rounded % 60) {
        (0, m) => format!("~{m}min"),
        (h, 0) => format!("~{h}h"),
        (h, m) => format!("~{h}h{m:02}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn profile(extra: &str) -> Profile {
        Profile::from_toml(
            &format!(
                "countries = [\"Belgium\", \"France\", \"Germany\"]\n\
                 [home]\ncity = \"Brussels\"\ncountry = \"Belgium\"\nairports = [\"BRU\", \"CRL\"]\n{extra}"
            ),
            Path::new("test.toml"),
        )
        .unwrap()
    }

    fn to(city: &str, country: &str) -> Destination {
        Destination {
            city: city.into(),
            country: country.into(),
        }
    }

    fn estimate(train_minutes: Option<u32>, airport: Option<&str>) -> Estimate {
        Estimate {
            train_minutes,
            airport: airport.map(str::to_owned),
        }
    }

    fn label(profile: &Profile, d: &Destination, cached: Option<&Estimate>) -> Option<String> {
        let over = profile.travel.override_for(&d.city);
        trip(profile, d, over, cached).map(|t| t.label)
    }

    #[test]
    fn formats_durations_to_the_quarter_hour() {
        assert_eq!(duration(5), "~15min");
        assert_eq!(duration(37), "~30min");
        assert_eq!(duration(38), "~45min");
        assert_eq!(duration(120), "~2h");
        assert_eq!(duration(84), "~1h30");
        assert_eq!(duration(201), "~3h15");
    }

    #[test]
    fn labels_each_kind_of_trip() {
        let p = profile("");
        let home = to(" brussels ", "Belgium");
        assert_eq!(label(&p, &home, None).as_deref(), Some("local"));
        let ghent = estimate(Some(35), None);
        assert_eq!(
            label(&p, &to("Ghent", "Belgium"), Some(&ghent)).as_deref(),
            Some("local, train ~30min")
        );
        let paris = estimate(Some(84), Some("CDG"));
        assert_eq!(
            label(&p, &to("Paris", "France"), Some(&paris)).as_deref(),
            Some("train ~1h30")
        );
        let munich = estimate(Some(384), Some("MUC"));
        let trip = trip(&p, &to("Munich", "Germany"), None, Some(&munich)).unwrap();
        assert_eq!(trip.kind, TripKind::Flight);
        assert_eq!(trip.airport.as_deref(), Some("MUC"));
        assert_eq!(trip.label, "fly BRU→MUC");
        let far = estimate(Some(600), None);
        assert_eq!(
            label(&p, &to("Toulouse", "France"), Some(&far)).as_deref(),
            Some("no practical flight (train ~10h)")
        );
        let island = estimate(None, None);
        assert_eq!(
            label(&p, &to("Ajaccio", "France"), Some(&island)).as_deref(),
            Some("no practical train or flight")
        );
        assert_eq!(label(&p, &to("Lyon", "France"), None), None);
    }

    #[test]
    fn the_train_limit_comes_from_the_profile_and_is_inclusive() {
        let munich = estimate(Some(384), Some("MUC"));
        let d = to("Munich", "Germany");
        let wide = profile("[travel]\ntrain_limit_hours = 6.4\n");
        assert_eq!(
            label(&wide, &d, Some(&munich)).as_deref(),
            Some("train ~6h30")
        );
        let narrow = profile("[travel]\ntrain_limit_hours = 6.3\n");
        assert_eq!(
            label(&narrow, &d, Some(&munich)).as_deref(),
            Some("fly BRU→MUC")
        );
    }

    #[test]
    fn without_home_country_only_home_city_is_local() {
        let p = Profile::from_toml(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"CRL\"]\n",
            Path::new("test.toml"),
        )
        .unwrap();
        let ghent = estimate(Some(35), None);
        assert_eq!(
            label(&p, &to("Ghent", "Belgium"), Some(&ghent)).as_deref(),
            Some("train ~30min")
        );
    }

    #[test]
    fn overrides_win_field_by_field() {
        let p = profile(
            "[[travel.overrides]]\ncity = \"Munich\"\ntrain_hours = 3.5\n\
             [[travel.overrides]]\ncity = \"Berlin\"\nairport = \"ber\"\n\
             [[travel.overrides]]\ncity = \"Lyon\"\nno_practical_flight = true\n",
        );
        let munich = estimate(Some(384), Some("MUC"));
        assert_eq!(
            label(&p, &to("munich", "Germany"), Some(&munich)).as_deref(),
            Some("train ~3h30")
        );
        let berlin = estimate(Some(450), Some("SXF"));
        assert_eq!(
            label(&p, &to("Berlin", "Germany"), Some(&berlin)).as_deref(),
            Some("fly BRU→BER")
        );
        let lyon = estimate(Some(330), Some("LYS"));
        assert_eq!(
            label(&p, &to("Lyon", "France"), Some(&lyon)).as_deref(),
            Some("no practical flight (train ~5h30)")
        );
    }

    #[test]
    fn an_override_alone_settles_a_trip_only_when_it_says_enough() {
        let p = profile(
            "[[travel.overrides]]\ncity = \"Lille\"\ntrain_hours = 1.5\n\
             [[travel.overrides]]\ncity = \"Berlin\"\nairport = \"BER\"\n\
             [[travel.overrides]]\ncity = \"Munich\"\ntrain_hours = 6.5\nairport = \"MUC\"\n",
        );
        assert_eq!(
            label(&p, &to("Lille", "France"), None).as_deref(),
            Some("train ~1h30")
        );
        // Whether Berlin is within the Train limit is still unknown.
        assert_eq!(label(&p, &to("Berlin", "Germany"), None), None);
        assert_eq!(
            label(&p, &to("Munich", "Germany"), None).as_deref(),
            Some("fly BRU→MUC")
        );
    }
}
