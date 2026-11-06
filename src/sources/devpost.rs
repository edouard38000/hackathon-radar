//! Devpost, through the JSON its Hackathons page loads: in-person events that
//! are upcoming or open, a page at a time.
//!
//! Devpost gives each event's venue as its organiser typed it, often without a
//! city or country (`Santa Clara Convention Center`, `Austin, TX`), so the
//! country is read from the venue's text by [`places::from_venue`]: a US state
//! or its code, a known city (North American ones included), a trailing
//! country, or one of those named inside it (`University of Calgary`). A venue
//! that names none of them, such as a bare building name, leaves the country
//! unknown, and Countries then leaves the listing out.
//!
//! Known limitation: Devpost only gives a submission period, not the event's
//! days. For an in-person event that period usually spans the event, but it
//! can open weeks before it. Candidates from Devpost say so
//! ([`Candidate::dates`] is [`Dates::SubmissionPeriod`]), so later rules and
//! Fares don't take those dates as the days someone has to be there.
//!
//! [`Dates::SubmissionPeriod`]: crate::candidate::Dates::SubmissionPeriod

use chrono::NaiveDate;
use serde::Deserialize;

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::sources::{Source, places};

pub const NAME: &str = "devpost";
pub const API_URL: &str =
    "https://devpost.com/api/hackathons?challenge_type[]=in-person&status[]=upcoming&status[]=open";

/// The most result pages read on one Run.
pub const MAX_PAGES: usize = 20;

pub struct Devpost<H> {
    http: H,
}

impl<H: HttpGet> Devpost<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }
}

/// The URL of one page of results, counting from 1.
pub fn page_url(page: usize) -> String {
    if page <= 1 {
        API_URL.to_owned()
    } else {
        format!("{API_URL}&page={page}")
    }
}

impl<H: HttpGet> Source for Devpost<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        let mut candidates = Vec::new();
        let mut listed = 0;
        for page in 1..=MAX_PAGES {
            let response = parse_page(&self.http.get_text(&page_url(page))?)?;
            listed += response.listed;
            candidates.extend(response.candidates);
            if response.listed == 0 || listed >= response.total {
                break;
            }
        }
        Ok(candidates)
    }
}

/// One page of results.
#[derive(Debug)]
pub struct Page {
    /// The in-person Hackathons on the page that have a venue and readable dates.
    pub candidates: Vec<Candidate>,
    /// How many Hackathons the page listed, before any was left out.
    pub listed: usize,
    /// How many Hackathons all pages list together.
    pub total: usize,
}

pub fn parse_page(json: &str) -> anyhow::Result<Page> {
    let response: Response = serde_json::from_str(json)?;
    let listed = response.hackathons.len();
    let candidates = response
        .hackathons
        .into_iter()
        .filter_map(to_candidate)
        .collect();
    Ok(Page {
        candidates,
        listed,
        total: response.meta.map_or(listed, |m| m.total_count),
    })
}

fn to_candidate(h: ApiHackathon) -> Option<Candidate> {
    let venue = h.displayed_location.and_then(|l| l.location)?;
    let venue = venue.trim();
    if venue.is_empty() || venue.eq_ignore_ascii_case("online") {
        return None;
    }
    let (start, end) = parse_dates(&h.submission_period_dates)?;
    // `Santa Clara Convention Center + Online` is a hybrid event at that venue.
    let place = venue
        .trim_end_matches("+ Online")
        .trim_end_matches("+ online")
        .trim();
    let (city, country) = places::from_venue(place);
    Some(Candidate {
        source: NAME.to_owned(),
        source_key: h.id.to_string(),
        name: h.title.trim().to_owned(),
        start,
        end,
        city,
        location: Some(venue.to_owned()),
        country,
        link: Some(h.url),
    })
}

/// Reads Devpost's date ranges: `Nov 15, 2026`, `Nov 03 - 05, 2026`,
/// `Sep 22 - Dec 07, 2026` and `Dec 28, 2026 - Jan 03, 2027`.
pub fn parse_dates(text: &str) -> Option<(NaiveDate, NaiveDate)> {
    let text = text.trim();
    let day = |month: &str, day: &str, year: &str| {
        NaiveDate::parse_from_str(&format!("{month} {day} {year}"), "%b %d %Y").ok()
    };
    let Some((from, to)) = text.split_once(" - ") else {
        let (month_day, year) = text.split_once(',')?;
        let (month, d) = month_day.trim().split_once(' ')?;
        let date = day(month, d, year.trim())?;
        return Some((date, date));
    };
    let (to_part, to_year) = to.split_once(',')?;
    let to_year = to_year.trim();
    let to_words: Vec<&str> = to_part.split_whitespace().collect();
    let (from_part, from_year) = from.split_once(',').unwrap_or((from, to_year));
    let (from_month, from_day) = from_part.trim().split_once(' ')?;
    let start = day(from_month, from_day, from_year.trim())?;
    let end = match to_words[..] {
        [d] => day(from_month, d, to_year)?,
        [month, d] => day(month, d, to_year)?,
        _ => return None,
    };
    Some((start, end.max(start)))
}

#[derive(Deserialize)]
struct Response {
    hackathons: Vec<ApiHackathon>,
    meta: Option<Meta>,
}

#[derive(Deserialize)]
struct Meta {
    total_count: usize,
}

#[derive(Deserialize)]
struct ApiHackathon {
    id: u64,
    title: String,
    url: String,
    displayed_location: Option<DisplayedLocation>,
    submission_period_dates: String,
}

#[derive(Deserialize)]
struct DisplayedLocation {
    location: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn reads_every_date_range_shape() {
        assert_eq!(
            parse_dates("Nov 15, 2026"),
            Some((date(2026, 11, 15), date(2026, 11, 15)))
        );
        assert_eq!(
            parse_dates("Nov 03 - 05, 2026"),
            Some((date(2026, 11, 3), date(2026, 11, 5)))
        );
        assert_eq!(
            parse_dates("Sep 22 - Dec 07, 2026"),
            Some((date(2026, 9, 22), date(2026, 12, 7)))
        );
        assert_eq!(
            parse_dates("Dec 28, 2026 - Jan 03, 2027"),
            Some((date(2026, 12, 28), date(2027, 1, 3)))
        );
        assert_eq!(parse_dates("Soon"), None);
        assert_eq!(parse_dates(""), None);
    }

    #[test]
    fn page_urls_count_from_one() {
        assert_eq!(page_url(1), API_URL);
        assert_eq!(page_url(3), format!("{API_URL}&page=3"));
    }
}
