//! Fares and the Flight budget (ADR 0002).
//!
//! For each flight Trip, the engine looks up the cheapest return Fare from
//! Home's airports for the Hackathon's dates (start − 1 day, end + 1 day)
//! through the person's own SerpApi key ([`serpapi`]). Without a key, the
//! Agent estimates it in a small read-only request ([`crate::agent::fares`])
//! and the engine stores the answer as an Unverified fare.
//!
//! SerpApi's free plan allows 250 searches a month, so Fares are looked up
//! sparingly: only for flight Trips to Hackathons that Fit, aren't exempt
//! from the budget by the Travel-funded rule, haven't started and start within
//! the Profile's horizon (10 weeks by default); each Fare is kept for a refresh interval (14 days)
//! before it is looked up again; and a Run makes at most a set number of
//! lookups (10), soonest Hackathon first. At the defaults, two Runs a week use
//! at most about 87 searches a month.
//!
//! The Flight budget then decides flights within Countries: a flight Trip Fits
//! only when its Fare, verified or not, is at or under the budget. Under the
//! Travel-funded rule (`travel.travel_funded_anywhere`), a Travel-funded
//! Hackathon skips the budget, as it skips Countries.
//!
//! A flight starting beyond the horizon isn't held back for a Fare: its
//! application deadline could pass first. It Fits, can be announced, and its
//! entry says the fare is checked closer to the date; the budget decides it
//! once it is inside the horizon. One already Announced that then turns out
//! over budget simply isn't announced again; no retraction is sent. Inside the
//! horizon, a flight with no Fare yet (the per-Run cap was reached, or the
//! lookup failed) is held back from Alerts, not dropped, until it has one,
//! usually a Run or two later. The Fare fills
//! the Alert entry's Fare slot ([`Priced::fill`]). A listing whose dates are a
//! submission period (Devpost) has no reliable travel dates, so it gets no
//! Fare and the budget leaves it unchecked. Without a Flight budget in the
//! Profile, none of this applies.

pub mod serpapi;

use std::collections::{HashMap, HashSet};

use chrono::{Days, NaiveDate};

use crate::agent::fares::FareRequest;
use crate::agent::{self, Agent, TravelFunded};
use crate::alert::Entry;
use crate::candidate::{Candidate, Dates};
use crate::money::Currency;
use crate::profile::Profile;
use crate::store::{ListingKey, Store, key_of};
use crate::travel::{Travelled, Trip, TripKind};

/// One Fare to look up: a return flight from any of Home's airports to the
/// airport nearest a Hackathon, economy, one adult.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FareQuery {
    /// Home's airports, in the Profile's order.
    pub from: Vec<String>,
    /// The destination airport.
    pub to: String,
    /// The day before the Hackathon starts.
    pub outbound: NaiveDate,
    /// The day after it ends.
    pub inbound: NaiveDate,
    /// The Profile's currency, such as `EUR`.
    pub currency: String,
}

impl FareQuery {
    /// The Fare for flying to `airport` for `hackathon`'s dates.
    pub fn for_hackathon(profile: &Profile, hackathon: &Candidate, airport: &str) -> Self {
        Self {
            from: profile.home.airports.clone(),
            to: airport.to_owned(),
            outbound: hackathon.start - Days::new(1),
            inbound: hackathon.end + Days::new(1),
            currency: profile.currency.code().to_owned(),
        }
    }
}

/// What a lookup found: the cheapest return price, in whole units of the
/// query's currency, and the Home airport it leaves from, when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundFare {
    pub price: u32,
    pub from_airport: Option<String>,
}

/// Looks up real Fares. [`serpapi::SerpApi`] is the one implementation; tests use fakes.
pub trait FareLookup {
    /// The cheapest return Fare for `query`, or `None` when no flight was found.
    fn lookup(&self, query: &FareQuery) -> anyhow::Result<Option<FoundFare>>;
}

/// A Fare as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fare {
    /// `None` when the lookup found no flight.
    pub price: Option<u32>,
    pub from_airport: Option<String>,
    /// Looked up from flight prices; `false` for an Unverified fare.
    pub verified: bool,
    pub checked_on: NaiveDate,
}

/// Where one Hackathon stands with the Flight budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FareCheck {
    /// Travel-funded under the Travel-funded rule, so the Flight budget doesn't apply.
    TravelPaid(TravelFunded),
    /// It has a Fare; `within_budget` says whether it Fits.
    Priced { fare: Fare, within_budget: bool },
    /// Inside the horizon with no Fare yet: held back from Alerts until it has one.
    Waiting,
    /// Starts beyond the horizon: Fits for now, with no Fare and no budget check yet.
    Later,
}

/// What this Run worked out about Fares.
#[derive(Debug, Default)]
pub struct Priced {
    /// By listing, for flight Trips only.
    pub checks: HashMap<ListingKey, FareCheck>,
    /// The Profile's currency, for showing prices.
    currency: Currency,
    /// Home's first airport, the one the Trip label names.
    home_airport: String,
    /// How many Fares were looked up (or estimated by the Agent) on this Run.
    pub looked_up: usize,
    /// Why Fare lookups failed this Run, if they did; they are tried again on the next Run.
    pub failure: Option<String>,
}

impl Priced {
    pub fn check(&self, hackathon: &Candidate) -> Option<&FareCheck> {
        self.checks.get(&key_of(hackathon))
    }

    /// Whether the Flight budget lets this Hackathon Fit.
    pub fn fits(&self, hackathon: &Candidate) -> bool {
        !matches!(
            self.check(hackathon),
            Some(FareCheck::Priced {
                within_budget: false,
                ..
            })
        )
    }

    /// Whether it waits for a Fare before any Alert lists it.
    pub fn held_back(&self, hackathon: &Candidate) -> bool {
        matches!(self.check(hackathon), Some(FareCheck::Waiting))
    }

    /// How many flight Trips wait for a Fare.
    pub fn waiting(&self) -> usize {
        self.checks
            .values()
            .filter(|c| matches!(c, FareCheck::Waiting))
            .count()
    }

    /// How many flight Trips are over the Flight budget.
    pub fn over_budget(&self) -> usize {
        self.checks
            .values()
            .filter(|c| {
                matches!(
                    c,
                    FareCheck::Priced {
                        within_budget: false,
                        ..
                    }
                )
            })
            .count()
    }

    /// The Fare as an Alert entry shows it after the Trip: `~€185`,
    /// `~€185 (unverified)`, `~€96 from CRL` when the cheapest flight leaves
    /// from another Home airport than the one the Trip label names, or
    /// [`LATER`] beyond the horizon.
    pub fn fare_text(&self, hackathon: &Candidate) -> Option<String> {
        if let Some(FareCheck::Later) = self.check(hackathon) {
            return Some(LATER.to_owned());
        }
        let Some(FareCheck::Priced {
            fare:
                Fare {
                    price: Some(price),
                    from_airport,
                    verified,
                    ..
                },
            ..
        }) = self.check(hackathon)
        else {
            return None;
        };
        let mut text = format!("~{}", self.currency.format(u64::from(*price)));
        if let Some(from) = from_airport.as_deref().filter(|f| *f != self.home_airport) {
            text.push_str(&format!(" from {from}"));
        }
        if !verified {
            text.push_str(" (unverified)");
        }
        Some(text)
    }

    /// Fills each entry's Fare slot, and drops the entries of flights that
    /// are over the Flight budget or still wait for a Fare.
    pub fn fill(&self, entries: &mut Vec<Entry>) {
        entries.retain(|e| self.fits(&e.hackathon) && !self.held_back(&e.hackathon));
        for entry in entries {
            entry.fare = self.fare_text(&entry.hackathon);
        }
    }
}

/// The Fare slot of a flight beyond the horizon.
pub const LATER: &str = "fare checked closer to the date";

/// Settles the Fare of each flight Trip in `fitting`, first looking up the
/// Fares that are missing or due for a refresh, within the per-Run cap: through
/// `lookup` when the person has a SerpApi key, otherwise by asking the Agent
/// (unless `ask_agent` is false because it already failed this Run). The Fares
/// are stored.
#[allow(clippy::too_many_arguments)]
pub fn price(
    agent: &dyn Agent,
    profile: &Profile,
    store: &mut Store,
    fitting: &[Candidate],
    travelled: &Travelled,
    lookup: Option<&dyn FareLookup>,
    today: NaiveDate,
    ask_agent: bool,
) -> anyhow::Result<Priced> {
    let mut priced = Priced::empty(profile);
    let Some(budget) = profile.travel.flight_budget else {
        return Ok(priced);
    };
    let settings = &profile.fares;

    // The flight Trips that need a Fare, and which of those to look up now.
    let mut flights: Vec<(&Candidate, FareQuery)> = Vec::new();
    let mut wanted: Vec<(FareQuery, String)> = Vec::new();
    let mut seen = HashSet::new();
    for hackathon in fitting {
        let query = match settle(profile, store, hackathon, travelled.trip(hackathon), today)? {
            Settled::NoCheck => continue,
            Settled::Checked(check) => {
                priced.checks.insert(key_of(hackathon), check);
                continue;
            }
            Settled::Query(query) => query,
        };
        let cached = store.fare(&query)?;
        let due = match &cached {
            None => true,
            // With a key, an Unverified fare is replaced by a real one.
            Some(fare) if lookup.is_some() && !fare.verified => true,
            Some(fare) => fare.checked_on + Days::new(u64::from(settings.refresh_days)) <= today,
        };
        let started = hackathon.start <= today;
        if due && !started && seen.insert(query.clone()) {
            let city = hackathon.city.clone().unwrap_or_default();
            wanted.push((query.clone(), city));
        }
        flights.push((hackathon, query));
    }

    // Soonest first, so the Hackathons closest to their dates get a Fare first.
    wanted.sort_by(|a, b| (a.0.outbound, &a.0.to).cmp(&(b.0.outbound, &b.0.to)));
    wanted.truncate(settings.lookups_per_run);
    if !wanted.is_empty() {
        match lookup {
            Some(lookup) => {
                for (query, _) in &wanted {
                    match lookup.lookup(query) {
                        Ok(found) => {
                            store.save_fare(query, found.as_ref(), true, "serpapi", today)?;
                            priced.looked_up += 1;
                        }
                        // Most failures (a bad key, no network) would fail every
                        // lookup, so the rest wait for the next Run.
                        Err(error) => {
                            priced.failure = Some(format!("{error:#}"));
                            break;
                        }
                    }
                }
            }
            None if ask_agent => {
                let ids = agent::numbered_ids(wanted.len());
                let request = FareRequest::new(&wanted, &ids, profile);
                let answer = agent
                    .judge(&request.agent_request())
                    .map_err(|e| format!("{e:#}"))
                    .and_then(|answer| request.read_fares(&answer).map_err(|e| e.to_string()));
                match answer {
                    Ok(estimates) => {
                        for (query, found) in &estimates {
                            store.save_fare(query, found.as_ref(), false, agent.name(), today)?;
                        }
                        priced.looked_up = estimates.len();
                    }
                    Err(error) => priced.failure = Some(error),
                }
            }
            None => {}
        }
    }

    for (hackathon, query) in flights {
        let check = stored_check(store, &query, budget)?;
        priced.checks.insert(key_of(hackathon), check);
    }
    Ok(priced)
}

impl Priced {
    fn empty(profile: &Profile) -> Self {
        Priced {
            currency: profile.currency.clone(),
            home_airport: profile.home.airports[0].clone(),
            ..Priced::default()
        }
    }

    /// Where each of `hackathons` stands with the Flight budget, from the
    /// stored Trips and Fares alone: nothing is looked up or stored. Reports
    /// and exports use it, so they decide the budget as the last Run did.
    pub fn from_store(
        profile: &Profile,
        store: &Store,
        hackathons: &[Candidate],
        today: NaiveDate,
    ) -> anyhow::Result<Self> {
        let mut priced = Priced::empty(profile);
        let Some(budget) = profile.travel.flight_budget else {
            return Ok(priced);
        };
        for hackathon in hackathons {
            let trip = store.trip(&hackathon.source, &hackathon.source_key)?;
            let check = match settle(profile, store, hackathon, trip.as_ref(), today)? {
                Settled::NoCheck => continue,
                Settled::Checked(check) => check,
                Settled::Query(query) => stored_check(store, &query, budget)?,
            };
            priced.checks.insert(key_of(hackathon), check);
        }
        Ok(priced)
    }
}

/// What the Flight budget needs to know about one Hackathon before any lookup.
enum Settled {
    /// Not a flight with an airport, or dates that give no flight dates: the budget doesn't decide it.
    NoCheck,
    /// Decided without a Fare.
    Checked(FareCheck),
    /// Decided by the Fare for this query.
    Query(FareQuery),
}

fn settle(
    profile: &Profile,
    store: &Store,
    hackathon: &Candidate,
    trip: Option<&Trip>,
    today: NaiveDate,
) -> anyhow::Result<Settled> {
    let Some(Trip {
        kind: TripKind::Flight,
        airport: Some(airport),
        ..
    }) = trip
    else {
        return Ok(Settled::NoCheck);
    };
    let funded = store
        .judgement(&hackathon.source, &hackathon.source_key)?
        .map_or(TravelFunded::None, |j| j.travel_funded);
    // The Travel-funded rule lets a Travel-funded Hackathon skip the budget.
    if funded != TravelFunded::None && profile.travel.travel_funded_anywhere {
        return Ok(Settled::Checked(FareCheck::TravelPaid(funded)));
    }
    // A submission period (Devpost) isn't the days someone has to be there,
    // so there are no reliable flight dates: no Fare is looked up, and the
    // Flight budget can't decide it. It is left unchecked, as without a
    // budget, rather than held back for a Fare that will never come.
    if hackathon.dates() == Dates::SubmissionPeriod {
        return Ok(Settled::NoCheck);
    }
    // Beyond the horizon: announced without a Fare, so its application
    // deadline can't pass while it waits; the budget decides it later.
    let horizon = today + Days::new(u64::from(profile.fares.horizon_weeks) * 7);
    if hackathon.start > horizon {
        return Ok(Settled::Checked(FareCheck::Later));
    }
    Ok(Settled::Query(FareQuery::for_hackathon(
        profile, hackathon, airport,
    )))
}

/// The stored Fare's verdict, or waiting when there is no price yet.
fn stored_check(store: &Store, query: &FareQuery, budget: u32) -> anyhow::Result<FareCheck> {
    Ok(match store.fare(query)? {
        Some(fare) if fare.price.is_some() => FareCheck::Priced {
            within_budget: fare.price.is_some_and(|p| p <= budget),
            fare,
        },
        _ => FareCheck::Waiting,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_spans_the_day_before_to_the_day_after() {
        let profile = Profile::from_toml(
            "countries = [\"Germany\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\", \"CRL\"]\n",
            std::path::Path::new("test.toml"),
        )
        .unwrap();
        let hackathon = Candidate {
            source: "s".into(),
            source_key: "k".into(),
            name: "Munich Hack".into(),
            start: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
            city: Some("Munich".into()),
            location: None,
            country: Some("Germany".into()),
            link: None,
        };
        let query = FareQuery::for_hackathon(&profile, &hackathon, "MUC");
        assert_eq!(query.from, ["BRU", "CRL"]);
        assert_eq!(
            query.outbound,
            NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()
        );
        assert_eq!(query.inbound, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(query.currency, "EUR");
    }
}
