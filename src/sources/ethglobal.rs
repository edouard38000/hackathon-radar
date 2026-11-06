//! ETHGlobal, through its events page. The page is a Next.js app whose data
//! comes with it as a React Server Components payload
//! (`self.__next_f.push([1, "…"])` scripts); each event is a JSON object in it
//! with its type (`hackathon`, `summit`, …), medium (`physical`, `virtual`),
//! start and end times, status and city. Only in-person and hybrid
//! hackathons that haven't finished are kept.

use chrono::DateTime;
use serde::Deserialize;
use serde_json::Value;

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::sources::{Source, places};

pub const NAME: &str = "ethglobal";
pub const EVENTS_URL: &str = "https://ethglobal.com/events";

pub struct EthGlobal<H> {
    http: H,
}

impl<H: HttpGet> EthGlobal<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }
}

impl<H: HttpGet> Source for EthGlobal<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        parse(&self.http.get_text(EVENTS_URL)?)
            .ok_or_else(|| anyhow::anyhow!("the ETHGlobal events page has no event data"))
    }
}

/// The page's upcoming in-person and hybrid hackathons, or `None` when the
/// page carries no event data at all (its format changed).
pub fn parse(html: &str) -> Option<Vec<Candidate>> {
    let payload = rsc_payload(html);
    let mut seen = Vec::new();
    let mut candidates = Vec::new();
    let mut any_event = false;
    // Every event object starts with its id and name; read each one whole.
    let mut rest = payload.as_str();
    let mut offset = 0;
    while let Some(at) = rest.find(r#"{"id":"#) {
        let start = offset + at;
        let mut stream = serde_json::Deserializer::from_str(&payload[start..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next()
            && let Ok(event) = serde_json::from_value::<Event>(value)
        {
            any_event = true;
            if !seen.contains(&event.id) {
                seen.push(event.id);
                candidates.extend(to_candidate(event));
            }
        }
        offset = start + 1;
        rest = &payload[offset..];
    }
    any_event.then_some(candidates)
}

/// The text the page's `self.__next_f.push([1, "…"])` scripts carry, joined.
fn rsc_payload(html: &str) -> String {
    const PUSH: &str = "self.__next_f.push(";
    let mut payload = String::new();
    let mut rest = html;
    while let Some(at) = rest.find(PUSH) {
        let after = &rest[at + PUSH.len()..];
        let Some(end) = after.find(")</script>") else {
            break;
        };
        if let Ok(Value::Array(parts)) = serde_json::from_str::<Value>(&after[..end])
            && let Some(Value::String(text)) = parts.get(1)
        {
            payload.push_str(text);
        }
        rest = &after[end..];
    }
    payload
}

fn to_candidate(event: Event) -> Option<Candidate> {
    if event.kind != "hackathon"
        || !matches!(event.medium.as_deref(), Some("physical" | "hybrid"))
        || matches!(event.status.as_deref(), Some("finished" | "cancelled"))
    {
        return None;
    }
    let start = DateTime::parse_from_rfc3339(event.start_time.as_deref()?)
        .ok()?
        .date_naive();
    let end = DateTime::parse_from_rfc3339(event.end_time.as_deref()?)
        .ok()?
        .date_naive();
    let city = event.city?;
    let country = city
        .country_code
        .as_deref()
        .and_then(places::country_name)
        .or_else(|| {
            city.country
                .as_ref()
                .and_then(|c| places::country_name(&c.name))
        })
        .map(str::to_owned);
    let city_name = country
        .as_deref()
        .and_then(|country| places::known_city_in(&city.name, country, None))
        .map_or_else(|| city.name.trim().to_owned(), str::to_owned);
    let link = event
        .website
        .filter(|w| w.starts_with("http"))
        .unwrap_or_else(|| format!("https://ethglobal.com/events/{}", event.slug));
    Some(Candidate {
        source: NAME.to_owned(),
        source_key: event.slug,
        name: event.name.trim().to_owned(),
        start,
        end: end.max(start),
        location: Some(city.name.trim().to_owned()),
        city: Some(city_name),
        country,
        link: Some(link),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    id: u64,
    name: String,
    slug: String,
    #[serde(rename = "type")]
    kind: String,
    medium: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    status: Option<String>,
    website: Option<String>,
    city: Option<City>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct City {
    name: String,
    country: Option<Country>,
    country_code: Option<String>,
}

#[derive(Deserialize)]
struct Country {
    name: String,
}
