//! Seam 2: the SerpApi Fare lookup against its contract. The request is
//! checked parameter by parameter against SerpApi's Google Flights docs
//! (https://serpapi.com/google-flights-api), and fixtures modelled on its
//! documented replies become Fares. Nothing touches the network.

mod support;

use std::cell::RefCell;

use hackathon_radar::fares::serpapi::{ENDPOINT, SerpApi, read_reply};
use hackathon_radar::fares::{FareLookup, FareQuery, FoundFare};
use hackathon_radar::http::HttpGet;
use support::*;

const KEY: &str = "test-key-0123456789abcdef";

fn munich() -> FareQuery {
    FareQuery {
        from: vec!["BRU".into(), "CRL".into()],
        to: "MUC".into(),
        outbound: date(2026, 12, 4),
        inbound: date(2026, 12, 7),
        currency: "EUR".into(),
    }
}

const MUNICH_URL: &str = "https://serpapi.com/search?engine=google_flights\
    &departure_id=BRU%2CCRL&arrival_id=MUC\
    &outbound_date=2026-12-04&return_date=2026-12-07\
    &type=1&travel_class=1&adults=1&currency=EUR&hl=en\
    &api_key=test-key-0123456789abcdef";

#[test]
fn the_request_carries_exactly_the_documented_parameters() {
    let http = FixtureHttp::new(&[(MUNICH_URL, fixture("serpapi/bru-crl-muc.json"))]);
    let serpapi = SerpApi::new(&http, KEY);

    serpapi.lookup(&munich()).unwrap();

    let requested = http.requested.borrow();
    assert_eq!(*requested, [MUNICH_URL]);
    let (endpoint, query) = requested[0].split_once('?').unwrap();
    assert_eq!(endpoint, ENDPOINT);
    let params: Vec<(&str, &str)> = query
        .split('&')
        .map(|pair| pair.split_once('=').unwrap())
        .collect();
    assert_eq!(
        params,
        [
            ("engine", "google_flights"),
            // Every Home airport in one search, as the docs allow.
            ("departure_id", "BRU%2CCRL"),
            ("arrival_id", "MUC"),
            // The day before the Hackathon starts, and the day after it ends.
            ("outbound_date", "2026-12-04"),
            ("return_date", "2026-12-07"),
            // Round trip, economy, one adult.
            ("type", "1"),
            ("travel_class", "1"),
            ("adults", "1"),
            ("currency", "EUR"),
            ("hl", "en"),
            ("api_key", KEY),
        ]
    );
}

#[test]
fn the_cheapest_option_across_home_airports_is_the_fare() {
    let http = FixtureHttp::new(&[(MUNICH_URL, fixture("serpapi/bru-crl-muc.json"))]);

    let found = SerpApi::new(&http, KEY).lookup(&munich()).unwrap();

    // 212 and 185 from BRU, 96 from CRL, and one option without a price.
    assert_eq!(
        found,
        Some(FoundFare {
            price: 96,
            from_airport: Some("CRL".into()),
        })
    );
}

#[test]
fn the_currency_comes_from_the_query() {
    let http = FixtureHttp::new(&[]);
    let in_dollars = FareQuery {
        currency: "USD".into(),
        ..munich()
    };

    let _ = SerpApi::new(&http, KEY).lookup(&in_dollars);

    assert!(http.requested.borrow()[0].contains("&currency=USD&"));
}

#[test]
fn no_results_means_no_flight() {
    assert_eq!(
        read_reply(&fixture("serpapi/no-results.json")).unwrap(),
        None
    );
}

#[test]
fn an_error_reply_fails_the_lookup_without_the_key() {
    let http = FixtureHttp::new(&[(MUNICH_URL, fixture("serpapi/error.json"))]);

    let error = SerpApi::new(&http, KEY)
        .lookup(&munich())
        .unwrap_err()
        .to_string();

    assert!(error.contains("Invalid API key"), "{error}");
    assert!(error.contains("BRU/CRL→MUC"), "{error}");
    assert!(!error.contains(KEY), "{error}");
}

#[test]
fn a_reply_that_is_not_the_documented_json_fails_the_lookup() {
    let http = FixtureHttp::new(&[(MUNICH_URL, "<html>Bad gateway</html>".to_owned())]);

    let error = SerpApi::new(&http, KEY).lookup(&munich()).unwrap_err();

    assert!(
        error.to_string().contains("not the documented JSON"),
        "{error}"
    );
}

/// Fails the way the real client does, naming the URL it fetched.
struct FailingHttp {
    requested: RefCell<Vec<String>>,
}

impl HttpGet for FailingHttp {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        self.requested.borrow_mut().push(url.to_owned());
        Err(anyhow::anyhow!("http status: 401").context(format!("fetching {url}")))
    }
}

#[test]
fn the_key_never_appears_in_errors_or_debug_output() {
    let http = FailingHttp {
        requested: RefCell::new(Vec::new()),
    };
    let serpapi = SerpApi::new(&http, KEY);

    let error = format!("{:#}", serpapi.lookup(&munich()).unwrap_err());

    assert!(http.requested.borrow()[0].contains(KEY), "the key is sent");
    assert!(error.contains("http status: 401"), "{error}");
    assert!(error.contains("api_key=<hidden>"), "{error}");
    assert!(!error.contains(KEY), "{error}");
    assert!(!format!("{serpapi:?}").contains(KEY));
}
