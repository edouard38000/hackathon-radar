//! hackathonhub.eu, which lists Hackathons across Europe. Its list pages are
//! rendered by JavaScript, so the Source reads its events sitemap and each
//! event page's structured data (schema.org JSON-LD) instead.
//!
//! The sitemap lists over a thousand events, so a Run only fetches the pages it
//! hasn't stored yet, up to a cap, and returns the listings it already stored
//! for the rest. Pages for events from past years are skipped.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use serde_json::Value;

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::sources::{Source, places};

pub const NAME: &str = "hackathonhub.eu";
pub const SITEMAP_URL: &str = "https://hackathonhub.eu/sitemap-events.xml";

/// The default for how many new event pages one Run fetches.
pub const DEFAULT_PAGES_PER_RUN: usize = 40;

pub struct HackathonHub<H> {
    http: H,
    /// Listings stored by earlier Runs, by event page URL.
    known: HashMap<String, Candidate>,
    pages_per_run: usize,
    today: NaiveDate,
}

impl<H: HttpGet> HackathonHub<H> {
    /// `known` holds this Source's listings stored by earlier Runs; their pages
    /// aren't fetched again.
    pub fn new(http: H, known: Vec<Candidate>, pages_per_run: usize, today: NaiveDate) -> Self {
        Self {
            http,
            known: known
                .into_iter()
                .filter(|c| c.source == NAME)
                .map(|c| (c.source_key.clone(), c))
                .collect(),
            pages_per_run,
            today,
        }
    }
}

impl<H: HttpGet> Source for HackathonHub<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        let entries = parse_sitemap(&self.http.get_text(SITEMAP_URL)?);
        let mut candidates = Vec::new();
        let mut new_pages = Vec::new();
        for entry in entries {
            match self.known.get(&entry.url) {
                Some(known) => candidates.push(known.clone()),
                None if !is_from_a_past_year(&entry.url, self.today) => new_pages.push(entry),
                None => {}
            }
        }
        // Upcoming events have the higher priority in the sitemap.
        new_pages.sort_by(|a, b| b.priority.total_cmp(&a.priority));
        for entry in new_pages.into_iter().take(self.pages_per_run) {
            // One page that can't be read shouldn't hide the others; it is tried again next Run.
            if let Ok(page) = self.http.get_text(&entry.url)
                && let Some(candidate) = parse_event_page(&entry.url, &page)
            {
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    }
}

/// One `<url>` of the sitemap.
#[derive(Debug, Clone, PartialEq)]
pub struct SitemapEntry {
    pub url: String,
    pub priority: f64,
}

/// Reads the event page URLs of a sitemap, in order.
pub fn parse_sitemap(xml: &str) -> Vec<SitemapEntry> {
    xml.split("<url>")
        .skip(1)
        .filter_map(|block| {
            let url = tag(block, "loc")?;
            let priority = tag(block, "priority")
                .and_then(|p| p.parse().ok())
                .unwrap_or(0.5);
            Some(SitemapEntry {
                url: unescape_xml(&url),
                priority,
            })
        })
        .collect()
}

fn tag(block: &str, name: &str) -> Option<String> {
    let start = block.find(&format!("<{name}>"))? + name.len() + 2;
    let end = start + block[start..].find(&format!("</{name}>"))?;
    let value = block[start..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Event slugs end with their year, such as `…-berlin-2026` or `…-turku-2026-3`;
/// an event from before this year is over.
fn is_from_a_past_year(url: &str, today: NaiveDate) -> bool {
    let slug = url.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let mut parts = slug.rsplit('-');
    let last = parts.next().unwrap_or("");
    let year = if last.len() == 4 {
        last
    } else {
        parts.next().unwrap_or("")
    };
    year.len() == 4
        && year.bytes().all(|b| b.is_ascii_digit())
        && year.parse::<i32>().is_ok_and(|y| y < today.year())
}

/// Reads an event page's schema.org Event. `None` when the page has none, or
/// it lacks a name or start date, or the event is online only.
pub fn parse_event_page(url: &str, html: &str) -> Option<Candidate> {
    let event = json_ld_blocks(html)
        .into_iter()
        .find_map(|v| find_event(&v))?;
    let text = |v: &Value| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    if event["eventAttendanceMode"]
        .as_str()
        .is_some_and(|m| m.ends_with("OnlineEventAttendanceMode"))
    {
        return None;
    }
    let name = text(&event["name"])?;
    let start = date(&event["startDate"])?;
    let end = date(&event["endDate"]).unwrap_or(start).max(start);
    let address = &event["location"]["address"];
    // An event held in two places lists both, such as `Sofia, Birkirkara` in
    // `BG, MT`; the first counts.
    let first = |text: String| text.split(',').next().unwrap_or("").trim().to_owned();
    let locality = text(&address["addressLocality"]).map(first);
    let country = text(&address["addressCountry"])
        .or_else(|| text(&address["addressCountry"]["name"]))
        .map(first)
        .map(|c| places::country_name(&c).map_or(c, str::to_owned));
    let city = locality
        .as_deref()
        .map(|l| places::known_city(l).map_or_else(|| l.to_owned(), |(name, _)| name.to_owned()));
    let location = text(&event["location"]["name"])
        .filter(|n| Some(n) != locality.as_ref())
        .or_else(|| locality.clone());
    // The organiser's own page is the official link; the listing page is the fallback.
    let link = text(&event["organizer"]["url"])
        .or_else(|| text(&event["offers"]["url"]))
        .or_else(|| text(&event["url"]))
        .or_else(|| Some(url.to_owned()));
    Some(Candidate {
        source: NAME.to_owned(),
        source_key: url.to_owned(),
        name,
        start,
        end,
        city,
        location,
        country,
        link,
    })
}

/// The JSON of each `<script type="application/ld+json">` on the page.
fn json_ld_blocks(html: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("application/ld+json") {
        rest = &rest[at..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let body = &rest[open_end + 1..];
        let Some(close) = body.find("</script>") else {
            break;
        };
        if let Ok(value) = serde_json::from_str(body[..close].trim()) {
            blocks.push(value);
        }
        rest = &body[close..];
    }
    blocks
}

/// The first object whose `@type` is Event (or a kind of Event), looking
/// through `@graph` and arrays.
fn find_event(value: &Value) -> Option<Value> {
    match value {
        Value::Array(items) => items.iter().find_map(find_event),
        Value::Object(map) => {
            let is_event = match map.get("@type") {
                Some(Value::String(t)) => t.ends_with("Event"),
                Some(Value::Array(ts)) => ts
                    .iter()
                    .any(|t| t.as_str().is_some_and(|t| t.ends_with("Event"))),
                _ => false,
            };
            if is_event {
                return Some(value.clone());
            }
            map.get("@graph").and_then(find_event)
        }
        _ => None,
    }
}

/// The date part of `2026-11-28T09:30:00+00:00` or `2026-11-28`.
fn date(value: &Value) -> Option<NaiveDate> {
    let text = value.as_str()?.trim();
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_from_past_years_are_skipped() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let past = |slug: &str| {
            is_from_a_past_year(&format!("https://hackathonhub.eu/events/{slug}"), today)
        };
        assert!(past("rule-roulette-game-jam-edinburgh-2020-2"));
        assert!(past("some-hack-berlin-2025"));
        assert!(!past("big-berlin-hack-berlin-2026"));
        assert!(!past("since-ai-hackathon-2026-turku-2026-3"));
        assert!(!past("a-hack-2027"));
        assert!(!past("no-year-here"));
    }
}
