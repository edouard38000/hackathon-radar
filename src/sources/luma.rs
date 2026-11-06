//! Luma calendars, read as ICS feeds. A community's Luma calendar mixes
//! Hackathons with meetups, talks and dinners, so only events whose name says
//! they are a hack, a jam or a build event are kept; the Agent judges the rest.
//!
//! Luma hides most addresses until you register, so the city usually comes
//! from the event's coordinates.

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::ics;
use crate::sources::{Source, calendar, places};

pub const NAME: &str = "luma";

/// The Europe Source pack's Luma calendars: {Tech: Europe} and the European Defense Tech Hub.
pub const EUROPE_CALENDARS: &[&str] = &[
    "https://api.lu.ma/ics/get?entity=calendar&id=cal-qyEpCltsspbMoJR",
    "https://api.lu.ma/ics/get?entity=calendar&id=cal-QCQbg4FodJDriFO",
];

/// The North America Source pack's Luma calendars: Luma's city pages for San
/// Francisco (luma.com/sf), New York (luma.com/nyc) and Boston
/// (luma.com/boston), each published as an ICS feed of its upcoming events.
pub const NORTH_AMERICA_CALENDARS: &[&str] = &[
    "https://api.lu.ma/ics/get?entity=discover&id=discplace-BDj7GNbGlsF7Cka",
    "https://api.lu.ma/ics/get?entity=discover&id=discplace-Izx1rQVSh8njYpP",
    "https://api.lu.ma/ics/get?entity=discover&id=discplace-VWeZ1zUvnawYHMj",
];

/// Words in an event's name that mark it as a possible Hackathon (folded, see [`places::fold`]).
const HACKATHON_WORDS: &[&str] = &[
    "hack",
    "buildathon",
    "datathon",
    "codefest",
    "jam",
    "makeathon",
];

pub struct LumaCalendar<H> {
    http: H,
    url: String,
}

impl<H: HttpGet> LumaCalendar<H> {
    pub fn new(http: H, url: impl Into<String>) -> Self {
        Self {
            http,
            url: url.into(),
        }
    }
}

impl<H: HttpGet> Source for LumaCalendar<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        Ok(parse(&self.http.get_text(&self.url)?))
    }
}

/// The calendar's events that may be Hackathons, as Candidates.
pub fn parse(feed: &str) -> Vec<Candidate> {
    ics::parse_events(feed)
        .iter()
        .filter_map(|event| calendar::to_candidate(NAME, event))
        .filter(|c| may_be_a_hackathon(&c.name))
        .collect()
}

fn may_be_a_hackathon(name: &str) -> bool {
    let name = places::fold(name);
    HACKATHON_WORDS.iter().any(|word| name.contains(word))
}
