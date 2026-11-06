//! Fares from Google Flights through SerpApi, on the person's own key (ADR 0002).
//!
//! One search per Fare (<https://serpapi.com/google-flights-api>):
//! `GET https://serpapi.com/search?engine=google_flights` with
//!
//! - `departure_id`: every Home airport, comma-separated, which the API
//!   documents as several departure airports in one search, so the cheapest
//!   Home airport costs one search instead of one per airport;
//! - `arrival_id`: the Hackathon's airport;
//! - `outbound_date` / `return_date`: start − 1 day and end + 1 day, `YYYY-MM-DD`;
//! - `type=1` (round trip), `travel_class=1` (economy), `adults=1`;
//! - `currency`: the Profile's currency; `hl=en`;
//! - `api_key`: the person's key, from the secret store ([`crate::secrets`]:
//!   `SERPAPI_API_KEY`, then the keychain). It is never stored in the Profile,
//!   never printed, and is scrubbed from every error.
//!
//! The reply's `best_flights` and `other_flights` hold the outbound options of
//! the round trip, each with the round-trip `price` and its `flights`, the
//! first of which leaves from `departure_airport.id`
//! (<https://serpapi.com/google-flights-results>). The cheapest of them is the
//! Fare. When neither list has a price, `price_insights.lowest_price` is used
//! (<https://serpapi.com/google-flights-price-insights>). A reply whose
//! `error` says Google Flights returned no results means there is no flight.

use std::fmt;

use anyhow::anyhow;
use serde::Deserialize;

use super::{FareLookup, FareQuery, FoundFare};
use crate::http::HttpGet;

/// The search endpoint.
pub const ENDPOINT: &str = "https://serpapi.com/search";

/// The environment variable that overrides the key in the keychain.
pub const KEY_VAR: &str = "SERPAPI_API_KEY";

/// What SerpApi's `error` says when Google Flights has nothing for the query.
const NO_RESULTS: &str = "hasn't returned any results";

/// Looks up Fares through SerpApi.
pub struct SerpApi<H> {
    http: H,
    key: String,
}

/// Never shows the key.
impl<H> fmt::Debug for SerpApi<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SerpApi").field("key", &"<hidden>").finish()
    }
}

impl<H: HttpGet> SerpApi<H> {
    pub fn new(http: H, key: impl Into<String>) -> Self {
        Self {
            http,
            key: key.into(),
        }
    }

    /// The search URL for `query`, key included.
    pub fn url(&self, query: &FareQuery) -> String {
        let outbound = query.outbound.format("%Y-%m-%d").to_string();
        let inbound = query.inbound.format("%Y-%m-%d").to_string();
        let params: [(&str, &str); 11] = [
            ("engine", "google_flights"),
            ("departure_id", &query.from.join(",")),
            ("arrival_id", &query.to),
            ("outbound_date", &outbound),
            ("return_date", &inbound),
            ("type", "1"),
            ("travel_class", "1"),
            ("adults", "1"),
            ("currency", &query.currency),
            ("hl", "en"),
            ("api_key", &self.key),
        ];
        let query: Vec<String> = params
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect();
        format!("{ENDPOINT}?{}", query.join("&"))
    }

    /// Removes the key from text that might carry the URL, such as an HTTP error.
    fn scrub(&self, text: &str) -> String {
        text.replace(&encode(&self.key), "<hidden>")
            .replace(&self.key, "<hidden>")
    }
}

impl<H: HttpGet> FareLookup for SerpApi<H> {
    fn lookup(&self, query: &FareQuery) -> anyhow::Result<Option<FoundFare>> {
        let route = format!("{}→{}", query.from.join("/"), query.to);
        let body = self.http.get_text(&self.url(query)).map_err(|error| {
            anyhow!(
                "SerpApi could not look up the Fare {route}: {}",
                self.scrub(&format!("{error:#}"))
            )
        })?;
        read_reply(&body).map_err(|error| {
            anyhow!(
                "SerpApi's reply for the Fare {route}: {}",
                self.scrub(&error.to_string())
            )
        })
    }
}

/// Reads the cheapest Fare from a Google Flights reply.
pub fn read_reply(body: &str) -> anyhow::Result<Option<FoundFare>> {
    let reply: Reply =
        serde_json::from_str(body).map_err(|e| anyhow!("not the documented JSON: {e}"))?;
    if let Some(error) = reply.error {
        if error.contains(NO_RESULTS) {
            return Ok(None);
        }
        anyhow::bail!("{error}");
    }
    if reply
        .search_metadata
        .and_then(|m| m.status)
        .is_some_and(|s| s != "Success")
    {
        anyhow::bail!("the search did not succeed");
    }
    let cheapest = reply
        .best_flights
        .iter()
        .chain(&reply.other_flights)
        .filter_map(|option| {
            let price = whole(option.price?)?;
            let from = option
                .flights
                .first()
                .and_then(|f| f.departure_airport.as_ref())
                .and_then(|a| a.id.clone());
            Some((price, from))
        })
        .min_by_key(|(price, _)| *price);
    Ok(match cheapest {
        Some((price, from_airport)) => Some(FoundFare {
            price,
            from_airport,
        }),
        None => reply
            .price_insights
            .and_then(|p| p.lowest_price)
            .and_then(whole)
            .map(|price| FoundFare {
                price,
                from_airport: None,
            }),
    })
}

/// A documented integer price, read as a number in case it ever has decimals.
fn whole(price: f64) -> Option<u32> {
    (price.is_finite() && price >= 0.5 && price < f64::from(u32::MAX)).then(|| price.round() as u32)
}

/// Percent-encodes a query value, keeping only unreserved characters.
fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// The documented reply, read leniently: only these fields matter.
#[derive(Deserialize)]
struct Reply {
    error: Option<String>,
    search_metadata: Option<Metadata>,
    #[serde(default)]
    best_flights: Vec<FlightOption>,
    #[serde(default)]
    other_flights: Vec<FlightOption>,
    price_insights: Option<PriceInsights>,
}

#[derive(Deserialize)]
struct Metadata {
    status: Option<String>,
}

#[derive(Deserialize)]
struct FlightOption {
    #[serde(default)]
    flights: Vec<Flight>,
    price: Option<f64>,
}

#[derive(Deserialize)]
struct Flight {
    departure_airport: Option<Airport>,
}

#[derive(Deserialize)]
struct Airport {
    id: Option<String>,
}

#[derive(Deserialize)]
struct PriceInsights {
    lowest_price: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("BRU,CRL"), "BRU%2CCRL");
        assert_eq!(encode("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(encode("2026-12-04"), "2026-12-04");
    }

    #[test]
    fn a_missing_price_list_falls_back_to_price_insights() {
        let found = read_reply(
            r#"{"search_metadata": {"status": "Success"}, "price_insights": {"lowest_price": 140}}"#,
        )
        .unwrap();
        assert_eq!(
            found,
            Some(FoundFare {
                price: 140,
                from_airport: None
            })
        );
        assert_eq!(
            read_reply(r#"{"search_metadata": {"status": "Success"}}"#).unwrap(),
            None
        );
    }
}
