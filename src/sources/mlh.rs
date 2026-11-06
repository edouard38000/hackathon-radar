//! Major League Hacking (MLH), through its season's events page. The page is
//! an Inertia.js app that ships its data as JSON in the page itself
//! (`<script data-page="app" type="application/json">`), so each event comes
//! out structured: name, start and end times, venue and format.
//!
//! MLH groups events by region (`EMEA`, `APAC`, `AMER`) and filters the page by
//! it. MLH is a global Source: it reads every region, one region's pages after
//! the other, and Countries leaves out what is too far away. Reading by region
//! keeps each listing short and its pages in date order.

use chrono::{DateTime, Datelike, NaiveDate};
use serde::Deserialize;

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::sources::{Source, places};

pub const NAME: &str = "mlh";
pub const BASE_URL: &str = "https://www.mlh.com";

/// The most result pages read per region on one Run (24 events a page).
pub const MAX_PAGES: usize = 15;

/// MLH's regions: Europe, the Middle East and Africa; Asia-Pacific; the Americas.
pub const REGIONS: &[&str] = &["EMEA", "APAC", "AMER"];

pub struct Mlh<H> {
    http: H,
    regions: &'static [&'static str],
    today: NaiveDate,
}

impl<H: HttpGet> Mlh<H> {
    /// Reads the current season's upcoming events in every MLH region.
    pub fn new(http: H, today: NaiveDate) -> Self {
        Self::in_regions(http, REGIONS, today)
    }

    /// Reads the current season's upcoming events in these MLH regions only.
    pub fn in_regions(http: H, regions: &'static [&'static str], today: NaiveDate) -> Self {
        Self {
            http,
            regions,
            today,
        }
    }
}

/// MLH's seasons run from summer to summer and are named after the year they
/// end in: the 2027 season starts in July 2026.
pub fn season(today: NaiveDate) -> i32 {
    if today.month() >= 7 {
        today.year() + 1
    } else {
        today.year()
    }
}

/// The URL of one page of a season's upcoming events in a region, counting from 1.
pub fn page_url(season: i32, region: &str, page: usize) -> String {
    let url = format!("{BASE_URL}/seasons/{season}/events?region={region}");
    if page <= 1 {
        url
    } else {
        format!("{url}&upcoming_page={page}")
    }
}

impl<H: HttpGet> Source for Mlh<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        let season = season(self.today);
        let mut candidates = Vec::new();
        for region in self.regions {
            for page in 1..=MAX_PAGES {
                let parsed = parse_page(&self.http.get_text(&page_url(season, region, page))?)?;
                candidates.extend(parsed.candidates);
                if !parsed.has_next_page {
                    break;
                }
            }
        }
        Ok(candidates)
    }
}

/// One page of upcoming events.
#[derive(Debug)]
pub struct Page {
    /// The in-person and hybrid events on the page.
    pub candidates: Vec<Candidate>,
    pub has_next_page: bool,
}

const DATA_START: &str = r#"<script data-page="app" type="application/json">"#;

pub fn parse_page(html: &str) -> anyhow::Result<Page> {
    let (_, after) = html
        .split_once(DATA_START)
        .ok_or_else(|| anyhow::anyhow!("the MLH events page has no page data"))?;
    let (json, _) = after
        .split_once("</script>")
        .ok_or_else(|| anyhow::anyhow!("the MLH events page data is cut short"))?;
    let data: PageData = serde_json::from_str(json)?;
    let has_next_page = data
        .scroll_props
        .and_then(|s| s.upcoming_events)
        .is_some_and(|u| u.next_page.is_some());
    Ok(Page {
        candidates: data
            .props
            .upcoming_events
            .into_iter()
            .filter_map(to_candidate)
            .collect(),
        has_next_page,
    })
}

fn to_candidate(event: Event) -> Option<Candidate> {
    // `digital` events are online only; `physical` and `hybrid_physical` have a venue.
    if !matches!(
        event.format_type.as_deref(),
        Some("physical" | "hybrid_physical")
    ) {
        return None;
    }
    let starts = DateTime::parse_from_rfc3339(&event.starts_at).ok()?;
    let ends = DateTime::parse_from_rfc3339(&event.ends_at).ok()?;
    let (start, end) = event
        .date_range
        .as_deref()
        .and_then(|range| read_date_range(range, starts.date_naive()))
        .unwrap_or((starts.date_naive(), ends.date_naive()));
    let venue = event.venue_address.unwrap_or_default();
    let country = venue
        .country
        .as_deref()
        .and_then(places::country_name)
        .map(str::to_owned);
    let region = venue
        .state
        .as_deref()
        .and_then(places::subdivision)
        .map(|(_, code)| code);
    let city = venue
        .city
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|c| {
            country
                .as_deref()
                .and_then(|country| places::known_city_in(c, country, region))
                .map_or_else(|| c.to_owned(), str::to_owned)
        });
    let location = event
        .location
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    let link = event
        .website_url
        .filter(|u| u.starts_with("http"))
        .unwrap_or_else(|| format!("{BASE_URL}{}", event.url));
    Some(Candidate {
        source: NAME.to_owned(),
        source_key: event.id,
        name: event.name.trim().to_owned(),
        start,
        end: end.max(start),
        city,
        location: location.map(str::to_owned),
        country,
        link: Some(link),
    })
}

/// Reads MLH's own `NOV 06 - 08`, `NOV 30 - DEC 02` or `NOV 06`, which give the
/// event's local days, where its start and end times are in UTC and can fall on
/// the day before or after. The year comes from the start time; `None` when the
/// range doesn't read or doesn't start within a day of it.
fn read_date_range(range: &str, utc_start: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let words: Vec<&str> = range.split_whitespace().filter(|w| *w != "-").collect();
    let month = |m: &str| {
        let m = m.to_ascii_lowercase();
        [
            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
        ]
        .iter()
        .position(|name| m.starts_with(name))
        .map(|i| i as u32 + 1)
    };
    let (start_month, start_day, end_month, end_day) = match words[..] {
        [m, d] => (month(m)?, d.parse().ok()?, month(m)?, d.parse().ok()?),
        [m, d, e] => (month(m)?, d.parse().ok()?, month(m)?, e.parse().ok()?),
        [m, d, em, e] => (month(m)?, d.parse().ok()?, month(em)?, e.parse().ok()?),
        _ => return None,
    };
    // The local start is within a day of the UTC start, possibly across New Year.
    let start = [utc_start.year() - 1, utc_start.year(), utc_start.year() + 1]
        .into_iter()
        .filter_map(|y| NaiveDate::from_ymd_opt(y, start_month, start_day))
        .find(|d| (*d - utc_start).num_days().abs() <= 1)?;
    let end_year = if end_month < start_month {
        start.year() + 1
    } else {
        start.year()
    };
    let end = NaiveDate::from_ymd_opt(end_year, end_month, end_day)?;
    Some((start, end.max(start)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageData {
    props: Props,
    scroll_props: Option<ScrollProps>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Props {
    #[serde(default)]
    upcoming_events: Vec<Event>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrollProps {
    upcoming_events: Option<Scroll>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scroll {
    next_page: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    id: String,
    name: String,
    starts_at: String,
    ends_at: String,
    date_range: Option<String>,
    url: String,
    location: Option<String>,
    format_type: Option<String>,
    website_url: Option<String>,
    venue_address: Option<Venue>,
}

#[derive(Deserialize, Default)]
struct Venue {
    state: Option<String>,
    city: Option<String>,
    country: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn seasons_are_named_after_the_year_they_end_in() {
        assert_eq!(season(date(2026, 11, 6)), 2027);
        assert_eq!(season(date(2027, 3, 1)), 2027);
        assert_eq!(season(date(2027, 7, 1)), 2028);
    }

    #[test]
    fn page_urls_filter_by_region_and_count_from_one() {
        assert_eq!(
            page_url(2027, "AMER", 1),
            "https://www.mlh.com/seasons/2027/events?region=AMER"
        );
        assert_eq!(
            page_url(2027, "AMER", 2),
            "https://www.mlh.com/seasons/2027/events?region=AMER&upcoming_page=2"
        );
    }

    #[test]
    fn mlhs_date_ranges_give_the_local_days() {
        // Starts on Friday evening in California, which is already Saturday in UTC.
        assert_eq!(
            read_date_range("NOV 06 - 08", date(2026, 11, 7)),
            Some((date(2026, 11, 6), date(2026, 11, 8)))
        );
        assert_eq!(
            read_date_range("NOV 30 - DEC 02", date(2026, 11, 30)),
            Some((date(2026, 11, 30), date(2026, 12, 2)))
        );
        assert_eq!(
            read_date_range("DEC 31 - JAN 02", date(2027, 1, 1)),
            Some((date(2026, 12, 31), date(2027, 1, 2)))
        );
        assert_eq!(
            read_date_range("NOV 06", date(2026, 11, 6)),
            Some((date(2026, 11, 6), date(2026, 11, 6)))
        );
        assert_eq!(read_date_range("NOV 20 - 22", date(2026, 11, 6)), None);
        assert_eq!(read_date_range("SOON", date(2026, 11, 6)), None);
    }
}
