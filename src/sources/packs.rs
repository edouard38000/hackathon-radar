//! Which Sources a Run reads (ADR 0009): the global Sources for everyone, the
//! Source pack Home's country picks, any packs the Profile adds, and the
//! person's own calendars.
//!
//! - Global: Devpost, MLH (every region), ETHGlobal, the CASSINI Hackathons
//!   and dev.events.
//! - Europe: hackathon.be, hackathonhub.eu and the European Luma calendars.
//! - North America: Luma's San Francisco, New York and Boston calendars.
//!
//! Web search, the last global Source in ADR 0009, goes through the Agent and
//! isn't one of these: a Run already makes the two Agent requests ADR 0004
//! allows (judging and travel estimates).

use std::fmt;

use chrono::NaiveDate;
use serde::Deserialize;

use crate::candidate::Candidate;
use crate::countries;
use crate::http::HttpGet;
use crate::profile::Profile;
use crate::sources::calendar::IcsCalendar;
use crate::sources::cassini::Cassini;
use crate::sources::dev_events::DevEvents;
use crate::sources::devpost::Devpost;
use crate::sources::ethglobal::EthGlobal;
use crate::sources::hackathon_be::HackathonBe;
use crate::sources::hackathonhub::HackathonHub;
use crate::sources::luma::{self, LumaCalendar};
use crate::sources::mlh::Mlh;
use crate::sources::{Source, places};

/// A Source pack: the Sources for one part of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pack {
    Europe,
    NorthAmerica,
}

impl Pack {
    pub const ALL: [Pack; 2] = [Pack::Europe, Pack::NorthAmerica];

    /// The name the Profile uses for it.
    pub fn name(self) -> &'static str {
        match self {
            Pack::Europe => "europe",
            Pack::NorthAmerica => "north-america",
        }
    }

    /// The pack for a country (by any name [`places::country_name`] knows), if one covers it.
    pub fn for_country(country: &str) -> Option<Pack> {
        let country = places::country_name(country)?;
        if countries::EUROPE.contains(&country) {
            Some(Pack::Europe)
        } else if countries::NORTH_AMERICA.contains(&country) {
            Some(Pack::NorthAmerica)
        } else {
            None
        }
    }

    /// The Sources the pack adds.
    pub fn sources(self) -> Vec<SourceSpec> {
        match self {
            Pack::Europe => {
                let mut sources = vec![SourceSpec::HackathonBe, SourceSpec::HackathonHub];
                sources.extend(luma::EUROPE_CALENDARS.iter().map(|u| SourceSpec::Luma(u)));
                sources
            }
            Pack::NorthAmerica => luma::NORTH_AMERICA_CALENDARS
                .iter()
                .map(|u| SourceSpec::Luma(u))
                .collect(),
        }
    }
}

impl fmt::Display for Pack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One Source a Run reads, before it is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSpec {
    Devpost,
    /// MLH, in every region.
    Mlh,
    EthGlobal,
    Cassini,
    DevEvents,
    HackathonBe,
    HackathonHub,
    /// A Luma calendar's ICS feed.
    Luma(&'static str),
    /// One of the person's own ICS calendars.
    Ics(String),
}

/// The Sources everyone gets, wherever Home is.
pub fn global() -> Vec<SourceSpec> {
    vec![
        SourceSpec::Devpost,
        SourceSpec::Mlh,
        SourceSpec::EthGlobal,
        SourceSpec::Cassini,
        SourceSpec::DevEvents,
    ]
}

/// The Profile's Source packs: the one Home's country picks first, then those
/// it adds, each once.
pub fn packs(profile: &Profile) -> Vec<Pack> {
    let mut packs: Vec<Pack> = profile
        .home
        .country_name()
        .and_then(|c| Pack::for_country(&c))
        .into_iter()
        .collect();
    for pack in &profile.sources.packs {
        if !packs.contains(pack) {
            packs.push(*pack);
        }
    }
    packs
}

/// Every Source a Run with this Profile reads: the global ones, its packs'
/// and its own calendars.
pub fn plan(profile: &Profile) -> Vec<SourceSpec> {
    let mut plan = global();
    for pack in packs(profile) {
        plan.extend(pack.sources());
    }
    plan.extend(profile.sources.ics.iter().cloned().map(SourceSpec::Ics));
    plan
}

/// What building the Sources needs besides the plan.
pub struct Fetching<H> {
    /// The polite client the built-in Sources share.
    pub http: H,
    /// The client for the person's own calendars.
    pub own_calendars: H,
    pub today: NaiveDate,
    /// hackathonhub.eu listings stored by earlier Runs.
    pub hackathonhub_known: Vec<Candidate>,
    pub hackathonhub_pages_per_run: usize,
}

/// Builds the planned Sources.
pub fn build<H>(plan: &[SourceSpec], fetching: Fetching<H>) -> Vec<Box<dyn Source>>
where
    H: HttpGet + Clone + 'static,
{
    let Fetching {
        http,
        own_calendars,
        today,
        hackathonhub_known,
        hackathonhub_pages_per_run,
    } = fetching;
    let mut hub_known = Some(hackathonhub_known);
    plan.iter()
        .map(|spec| -> Box<dyn Source> {
            match spec {
                SourceSpec::Devpost => Box::new(Devpost::new(http.clone())),
                SourceSpec::Mlh => Box::new(Mlh::new(http.clone(), today)),
                SourceSpec::EthGlobal => Box::new(EthGlobal::new(http.clone())),
                SourceSpec::Cassini => Box::new(Cassini::new(http.clone())),
                SourceSpec::DevEvents => Box::new(DevEvents::new(http.clone())),
                SourceSpec::HackathonBe => Box::new(HackathonBe::new(http.clone())),
                SourceSpec::HackathonHub => Box::new(HackathonHub::new(
                    http.clone(),
                    hub_known.take().unwrap_or_default(),
                    hackathonhub_pages_per_run,
                    today,
                )),
                SourceSpec::Luma(url) => Box::new(LumaCalendar::new(http.clone(), *url)),
                SourceSpec::Ics(url) => {
                    Box::new(IcsCalendar::new(own_calendars.clone(), url.clone()))
                }
            }
        })
        .collect()
}
