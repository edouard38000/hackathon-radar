//! Worldwide (ADR 0009): the global Sources and the North America pack's
//! against their recorded fixtures (seam 2), the Source pack picked from Home,
//! and whole Runs for a San Francisco Profile (seam 1).

mod support;

use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use hackathon_radar::candidate::Candidate;
use hackathon_radar::profile::Profile;
use hackathon_radar::sources::Source;
use hackathon_radar::sources::cassini::{self, Cassini};
use hackathon_radar::sources::dev_events::{self, DevEvents};
use hackathon_radar::sources::devpost;
use hackathon_radar::sources::ethglobal::{self, EthGlobal};
use hackathon_radar::sources::luma::{self, LumaCalendar};
use hackathon_radar::sources::mlh::{self, Mlh};
use hackathon_radar::sources::packs::{self, Fetching, Pack, SourceSpec};
use hackathon_radar::sources::polite::PoliteHttp;
use support::*;

const TODAY: (i32, u32, u32) = (2026, 11, 6);

fn today() -> chrono::NaiveDate {
    date(TODAY.0, TODAY.1, TODAY.2)
}

/// Name, city and country of each Candidate, for compact comparisons.
fn places(candidates: &[Candidate]) -> Vec<(String, Option<String>, Option<String>)> {
    candidates
        .iter()
        .map(|c| (c.name.clone(), c.city.clone(), c.country.clone()))
        .collect()
}

fn place(name: &str, city: &str, country: &str) -> (String, Option<String>, Option<String>) {
    (name.into(), Some(city.into()), Some(country.into()))
}

fn mlh_pages() -> Vec<(String, String)> {
    vec![
        (mlh::page_url(2027, "EMEA", 1), fixture("mlh/emea.html")),
        (mlh::page_url(2027, "APAC", 1), fixture("mlh/apac.html")),
        (mlh::page_url(2027, "AMER", 1), fixture("mlh/amer-1.html")),
        // The recorded Americas listing is cut short: its second page is its last.
        (mlh::page_url(2027, "AMER", 2), fixture("mlh/amer-2.html")),
    ]
}

/// Every page the global Sources and both Source packs read, from fixtures.
fn every_page() -> Vec<(String, String)> {
    let mut pages = mlh_pages();
    pages.extend([
        (devpost::API_URL.to_owned(), fixture("devpost/page-1.json")),
        (devpost::page_url(2), fixture("devpost/page-2.json")),
        (
            ethglobal::EVENTS_URL.to_owned(),
            fixture("ethglobal/events.html"),
        ),
        (
            cassini::PAGE_URL.to_owned(),
            fixture("cassini/hackathons.html"),
        ),
        (
            dev_events::FEED_URL.to_owned(),
            fixture("dev-events/rss.xml"),
        ),
        (
            luma::NORTH_AMERICA_CALENDARS[0].to_owned(),
            fixture("luma/san-francisco.ics"),
        ),
        (
            luma::NORTH_AMERICA_CALENDARS[1].to_owned(),
            fixture("luma/new-york.ics"),
        ),
        (
            luma::NORTH_AMERICA_CALENDARS[2].to_owned(),
            fixture("luma/boston.ics"),
        ),
        (
            luma::EUROPE_CALENDARS[0].to_owned(),
            fixture("luma/tech-europe.ics"),
        ),
        (
            luma::EUROPE_CALENDARS[1].to_owned(),
            fixture("luma/european-defense-tech-hub.ics"),
        ),
        (
            hackathon_radar::sources::hackathon_be::CALENDAR_URL.to_owned(),
            fixture("hackathon-be/calendar.ics"),
        ),
        (
            hackathon_radar::sources::hackathonhub::SITEMAP_URL.to_owned(),
            fixture("hackathonhub/sitemap-events.xml"),
        ),
    ]);
    pages
}

fn fixture_http(pages: &[(String, String)]) -> Rc<FixtureHttp> {
    let pages: Vec<(&str, String)> = pages.iter().map(|(u, b)| (u.as_str(), b.clone())).collect();
    Rc::new(FixtureHttp::new(&pages))
}

#[test]
fn mlh_reads_the_regions_outside_the_americas_for_everyone() {
    let http = fixture_http(&mlh_pages());
    let source = Mlh::new(http.clone(), mlh::OUTSIDE_THE_AMERICAS, today());

    let candidates = source.fetch().unwrap();

    assert_eq!(source.name(), "mlh");
    // The online-only Solana Hackathon is left out; the region's name after a
    // city (`Turku, Varsinais-Suomi`) stays in the location.
    assert_eq!(
        places(&candidates),
        [
            place("Since AI Hackathon", "Turku", "Finland"),
            place("HackTheBurgh", "Edinburgh", "United Kingdom"),
            place("Junction", "Espoo", "Finland"),
            place("HackEPS", "Lleida", "Spain"),
        ]
    );
    let since_ai = &candidates[0];
    assert_eq!(
        (since_ai.start, since_ai.end),
        (date(2026, 11, 6), date(2026, 11, 8))
    );
    assert_eq!(since_ai.source_key, "01a0eeb9-5924-af9e-cdc9-e95db9dced9e");
    assert_eq!(since_ai.link.as_deref(), Some("https://sinceai.ai/"));
    assert_eq!(since_ai.location.as_deref(), Some("Turku, Varsinais-Suomi"));
    // The 2027 season, one page per region.
    assert_eq!(
        *http.requested.borrow(),
        [
            "https://www.mlh.com/seasons/2027/events?region=EMEA",
            "https://www.mlh.com/seasons/2027/events?region=APAC",
        ]
    );
}

#[test]
fn mlh_reads_every_page_of_the_americas() {
    let http = fixture_http(&mlh_pages());
    let source = Mlh::new(http.clone(), mlh::AMERICAS, today());

    let candidates = source.fetch().unwrap();

    assert_eq!(
        places(&candidates),
        [
            place("HackUTD Roadshow", "Richardson", "United States"),
            place("SharkByte", "Miami", "United States"),
            place("HackTrent", "Peterborough", "Canada"),
            place("YCP Hacks", "York PA", "United States"),
            // Hybrid events have a venue, so they stay.
            place("MakeUC", "Cincinnati", "United States"),
            // London, Ontario and Athens, Georgia aren't the European cities,
            // and Manhattan, Kansas isn't New York.
            place("Hack Western 13", "London", "Canada"),
            place("BostonHacks", "Boston", "United States"),
            place("Hack Cats", "Manhattan", "United States"),
            place("QWER Hacks", "Los Angeles", "United States"),
            place("UGAHacks", "Athens", "United States"),
            place("SF Hacks", "San Francisco", "United States"),
        ]
    );
    assert_eq!(
        *http.requested.borrow(),
        [
            "https://www.mlh.com/seasons/2027/events?region=AMER",
            "https://www.mlh.com/seasons/2027/events?region=AMER&upcoming_page=2",
        ]
    );
}

#[test]
fn mlh_without_its_page_data_is_an_error() {
    let http = fixture_http(&[(
        mlh::page_url(2027, "EMEA", 1),
        "<html><body>Maintenance</body></html>".to_owned(),
    )]);
    let error = Mlh::new(http, &["EMEA"], today()).fetch().unwrap_err();
    assert!(error.to_string().contains("no page data"), "{error}");
}

#[test]
fn ethglobal_keeps_its_upcoming_in_person_hackathons() {
    let http = fixture_http(&[(
        ethglobal::EVENTS_URL.to_owned(),
        fixture("ethglobal/events.html"),
    )]);
    let source = EthGlobal::new(http);

    // The fixture also holds ETHConf 2027 (a summit), ETHGlobal Tokyo 2026
    // (finished) and ETHOnline 2026 (virtual).
    assert_eq!(
        source.fetch().unwrap(),
        [Candidate {
            source: "ethglobal".into(),
            source_key: "mumbai".into(),
            name: "ETHGlobal Mumbai".into(),
            start: date(2026, 11, 5),
            end: date(2026, 11, 7),
            city: Some("Mumbai".into()),
            location: Some("Mumbai".into()),
            country: Some("India".into()),
            link: Some("https://ethglobal.com/events/mumbai".into()),
        }]
    );
    assert_eq!(source.name(), "ethglobal");
}

#[test]
fn ethglobal_without_event_data_is_an_error() {
    let http = fixture_http(&[(
        ethglobal::EVENTS_URL.to_owned(),
        "<html><body>Events</body></html>".to_owned(),
    )]);
    assert!(EthGlobal::new(http).fetch().is_err());
}

#[test]
fn cassini_gives_one_candidate_per_city_of_the_edition() {
    let http = fixture_http(&[(
        cassini::PAGE_URL.to_owned(),
        fixture("cassini/hackathons.html"),
    )]);
    let source = Cassini::new(http);

    let candidates = source.fetch().unwrap();

    assert_eq!(source.name(), "cassini");
    assert_eq!(
        places(&candidates),
        [
            place("12th CASSINI Hackathon – Sofia", "Sofia", "Bulgaria"),
            place("12th CASSINI Hackathon – Prague", "Prague", "Czechia"),
            place("12th CASSINI Hackathon – Espoo", "Espoo", "Finland"),
            place("12th CASSINI Hackathon – Darmstadt", "Darmstadt", "Germany"),
            place("12th CASSINI Hackathon – Dublin", "Dublin", "Ireland"),
            place("12th CASSINI Hackathon – Turin", "Turin", "Italy"),
            place("12th CASSINI Hackathon – Kaunas", "Kaunas", "Lithuania"),
            place("12th CASSINI Hackathon – Warsaw", "Warsaw", "Poland"),
            place("12th CASSINI Hackathon – Bucharest", "Bucharest", "Romania"),
            place(
                "12th CASSINI Hackathon – Ivano-Frankivsk",
                "Ivano-Frankivsk",
                "Ukraine"
            ),
        ]
    );
    assert!(
        candidates
            .iter()
            .all(|c| (c.start, c.end) == (date(2026, 11, 27), date(2026, 11, 29)))
    );
    // Each city links to its own organiser's page, so they aren't merged as
    // Duplicate listings of one another.
    assert_eq!(
        candidates[0].link.as_deref(),
        Some("https://www.cassini.eu/hackathons/bulgaria")
    );
    assert_eq!(candidates[0].source_key, "12th-cassini-hackathon/sofia");
    let mut links: Vec<_> = candidates.iter().map(|c| c.link.clone()).collect();
    links.dedup();
    assert_eq!(links.len(), 10);
}

#[test]
fn dev_events_keeps_the_in_person_hackathons_of_its_feed() {
    let http = fixture_http(&[(
        dev_events::FEED_URL.to_owned(),
        fixture("dev-events/rss.xml"),
    )]);
    let source = DevEvents::new(http);

    let candidates = source.fetch().unwrap();

    assert_eq!(source.name(), "dev.events");
    // Conferences, a masterclass and the online Aevum Builder Challenge are left out.
    assert_eq!(
        places(&candidates),
        [
            place(
                "Do Not Disturb: Phone Use Hackathon",
                "San Francisco",
                "United States"
            ),
            place("SCALEFORGE NATIONAL AI HACKATHON", "Islamabad", "Pakistan"),
            place("Q2 DataRide 2027", "Mannheim", "Germany"),
            place(
                "AI for Social Good Hackathon at West Valley College",
                "Saratoga",
                "United States"
            ),
        ]
    );
    let data_ride = &candidates[2];
    // The end date comes from the preview image's `Apr 16-17 2027`.
    assert_eq!(
        (data_ride.start, data_ride.end),
        (date(2027, 4, 16), date(2027, 4, 17))
    );
    assert_eq!(data_ride.source_key, "q2-data-ride-2027-fslcvit8");
    assert_eq!(
        data_ride.link.as_deref(),
        Some("https://dev.events/conferences/q2-data-ride-2027-fslcvit8")
    );
    assert_eq!(data_ride.location.as_deref(), Some("Mannheim, Germany"));
}

#[test]
fn dev_events_refuses_a_page_that_isnt_its_feed() {
    let http = fixture_http(&[(
        dev_events::FEED_URL.to_owned(),
        "<html>Not found</html>".to_owned(),
    )]);
    assert!(DevEvents::new(http).fetch().is_err());
}

#[test]
fn the_north_american_luma_calendars_keep_their_hack_events() {
    let http = fixture_http(&every_page());
    let names = |url: &str| -> Vec<(String, Option<String>, Option<String>)> {
        places(&LumaCalendar::new(http.clone(), url).fetch().unwrap())
    };

    assert_eq!(
        names(luma::NORTH_AMERICA_CALENDARS[0]),
        [
            place(
                "Qualcomm UNO Q Hack Night #3",
                "Santa Clara",
                "United States"
            ),
            // Its address is hidden: its coordinates place it.
            place(
                "DNAForge & MTC Berkeley: AI × Biology Hackathon",
                "Berkeley",
                "United States"
            ),
            place(
                "She Builds: A Women in AI × Lovable Hackathon",
                "San Francisco",
                "United States"
            ),
        ]
    );
    // This week's New York calendar has no hack events.
    assert_eq!(names(luma::NORTH_AMERICA_CALENDARS[1]), []);
    assert_eq!(
        names(luma::NORTH_AMERICA_CALENDARS[2]),
        [place(
            "Lovable: Hackathon x Startup Trek",
            "Boston",
            "United States"
        )]
    );
}

fn profile(text: &str) -> Profile {
    Profile::from_toml(text, Path::new("test-profile.toml")).unwrap()
}

fn home(city: &str, country: Option<&str>, extra: &str) -> Profile {
    let country = country.map_or(String::new(), |c| format!("country = {c:?}\n"));
    profile(&format!(
        "countries = [\"North America\"]\n{extra}\n[home]\ncity = {city:?}\n{country}airports = [\"XXX\"]\n"
    ))
}

#[test]
fn the_source_pack_is_picked_from_homes_country() {
    assert_eq!(
        packs::packs(&home("Brussels", Some("Belgium"), "")),
        [Pack::Europe]
    );
    assert_eq!(
        packs::packs(&home("San Francisco", Some("United States"), "")),
        [Pack::NorthAmerica]
    );
    assert_eq!(
        packs::packs(&home("Toronto", Some("CA"), "")),
        [Pack::NorthAmerica]
    );
    // Without a country, a known city gives it.
    assert_eq!(packs::packs(&home("Lyon", None, "")), [Pack::Europe]);
    assert_eq!(
        packs::packs(&home("Seattle", None, "")),
        [Pack::NorthAmerica]
    );
    // No pack covers Kenya yet: only the global Sources apply.
    let nairobi = home("Nairobi", Some("Kenya"), "");
    assert_eq!(packs::packs(&nairobi), []);
    assert_eq!(packs::plan(&nairobi), packs::global());
    assert_eq!(packs::packs(&home("Smallville", None, "")), []);
}

#[test]
fn each_plan_has_the_global_sources_and_its_packs() {
    let global = [
        SourceSpec::Devpost,
        SourceSpec::Mlh(mlh::OUTSIDE_THE_AMERICAS),
        SourceSpec::EthGlobal,
        SourceSpec::Cassini,
        SourceSpec::DevEvents,
    ];
    let mut brussels = global.to_vec();
    brussels.extend([
        SourceSpec::HackathonBe,
        SourceSpec::HackathonHub,
        SourceSpec::Luma(luma::EUROPE_CALENDARS[0]),
        SourceSpec::Luma(luma::EUROPE_CALENDARS[1]),
    ]);
    assert_eq!(
        packs::plan(&home("Brussels", Some("Belgium"), "")),
        brussels
    );

    let mut san_francisco = global.to_vec();
    san_francisco.extend([
        SourceSpec::Mlh(mlh::AMERICAS),
        SourceSpec::Luma(luma::NORTH_AMERICA_CALENDARS[0]),
        SourceSpec::Luma(luma::NORTH_AMERICA_CALENDARS[1]),
        SourceSpec::Luma(luma::NORTH_AMERICA_CALENDARS[2]),
    ]);
    assert_eq!(
        packs::plan(&home("San Francisco", Some("United States"), "")),
        san_francisco
    );
}

#[test]
fn the_profile_can_add_packs_and_keeps_its_own_calendars() {
    let both = home(
        "Brussels",
        Some("Belgium"),
        "[sources]\npacks = [\"north-america\", \"europe\"]\nics = [\"https://example.org/my.ics\"]\nhackathonhub_pages_per_run = 5\n",
    );
    assert_eq!(packs::packs(&both), [Pack::Europe, Pack::NorthAmerica]);
    let plan = packs::plan(&both);
    assert!(plan.contains(&SourceSpec::HackathonBe));
    assert!(plan.contains(&SourceSpec::Mlh(mlh::AMERICAS)));
    assert_eq!(
        plan.last(),
        Some(&SourceSpec::Ics("https://example.org/my.ics".into()))
    );
    assert_eq!(both.sources.hackathonhub_pages_per_run, 5);

    let error = Profile::from_toml(
        "countries = [\"France\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n[sources]\npacks = [\"asia\"]\n",
        Path::new("test-profile.toml"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("asia"), "{error}");
    assert!(error.contains("north-america"), "{error}");
}

#[test]
fn the_planned_sources_are_built_in_order() {
    let plan = packs::plan(&home(
        "Brussels",
        Some("Belgium"),
        "[sources]\npacks = [\"north-america\"]\nics = [\"https://example.org/my.ics\"]\n",
    ));
    let http = fixture_http(&[]);
    let sources = packs::build(
        &plan,
        Fetching {
            http: http.clone(),
            own_calendars: http,
            today: today(),
            hackathonhub_known: Vec::new(),
            hackathonhub_pages_per_run: 40,
        },
    );
    let names: Vec<&str> = sources.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        [
            "devpost",
            "mlh",
            "ethglobal",
            "cassini",
            "dev.events",
            "hackathon.be",
            "hackathonhub.eu",
            "luma",
            "luma",
            "mlh",
            "luma",
            "luma",
            "luma",
            "ics",
        ]
    );
}

/// The Sources a Run with this Profile reads, built over the fixtures.
fn sources_for(profile: &Profile) -> Vec<Box<dyn Source>> {
    let http = fixture_http(&every_page());
    packs::build(
        &packs::plan(profile),
        Fetching {
            http: http.clone(),
            own_calendars: http,
            today: today(),
            hackathonhub_known: Vec::new(),
            hackathonhub_pages_per_run: 0,
        },
    )
}

fn san_francisco(countries: &str) -> Profile {
    profile(&format!(
        "countries = [{countries}]\n\n[home]\ncity = \"San Francisco\"\ncountry = \"United States\"\nairports = [\"SFO\", \"OAK\"]\n"
    ))
}

#[test]
fn a_san_francisco_run_fits_north_american_hackathons_and_not_european_ones() {
    let profile = san_francisco("\"North America\"");
    assert_eq!(profile.currency.code(), "USD");
    let sources = sources_for(&profile);
    let mut harness = Harness::new(profile);

    let outcome = harness.run(&sources, today(), DRY_RUN);

    assert!(
        outcome.failed_sources.is_empty(),
        "{:?}",
        outcome.failed_sources
    );
    let mut stored = harness.stored_names();
    stored.sort();
    assert_eq!(
        stored,
        [
            "AI for Social Good Hackathon at West Valley College",
            "BostonHacks",
            "DNAForge & MTC Berkeley: AI × Biology Hackathon",
            "DeveloperWeek 2027 Hackathon",
            "Do Not Disturb: Phone Use Hackathon",
            "Hack Cats",
            "Hack Western 13",
            "HackTrent",
            "HackUTD Roadshow",
            "Lovable: Hackathon x Startup Trek",
            "MakeUC",
            "QWER Hacks",
            "Qualcomm UNO Q Hack Night #3",
            "SF Hacks",
            "SharkByte",
            "She Builds: A Women in AI × Lovable Hackathon",
            "UGAHacks",
            "YCP Hacks",
        ]
    );
    // Every European Hackathon the global Sources list is left out.
    for european in [
        "Since AI Hackathon",
        "Junction",
        "HackEPS",
        "12th CASSINI Hackathon – Darmstadt",
        "Q2 DataRide 2027",
    ] {
        assert!(!stored.iter().any(|s| s == european), "{european}");
    }
    assert!(
        outcome
            .fitting
            .iter()
            .all(|h| matches!(h.country.as_deref(), Some("United States" | "Canada")))
    );
}

#[test]
fn a_us_region_in_countries_fits_only_its_states() {
    let profile = san_francisco("\"US West Coast\"");
    let sources = sources_for(&profile);
    let mut harness = Harness::new(profile);

    harness.run(&sources, today(), DRY_RUN);

    let mut stored = harness.stored_names();
    stored.sort();
    // California only: Boston, Texas, Canada and the event in Saratoga (whose
    // state the listing doesn't give) are left out.
    assert_eq!(
        stored,
        [
            "DNAForge & MTC Berkeley: AI × Biology Hackathon",
            "DeveloperWeek 2027 Hackathon",
            "Do Not Disturb: Phone Use Hackathon",
            "QWER Hacks",
            "Qualcomm UNO Q Hack Night #3",
            "SF Hacks",
            "She Builds: A Women in AI × Lovable Hackathon",
        ]
    );
}

#[test]
fn a_brussels_run_with_the_same_sources_fits_european_hackathons_only() {
    let profile = travel_profile(&["Schengen"], "");
    let sources = sources_for(&profile);
    let mut harness = Harness::new(profile);

    let outcome = harness.run(&sources, today(), DRY_RUN);

    let stored = harness.stored_names();
    assert!(stored.iter().any(|s| s == "Junction"), "{stored:?}");
    assert!(
        stored
            .iter()
            .any(|s| s.contains("CASSINI Hackathon – Darmstadt"))
    );
    // Ireland, Ukraine and the United Kingdom aren't in Schengen.
    assert!(
        !stored
            .iter()
            .any(|s| s.contains("Dublin") || s.contains("Ivano"))
    );
    assert!(!stored.iter().any(|s| s == "HackTheBurgh"));
    assert!(!stored.iter().any(|s| s == "SF Hacks"));
    assert!(
        outcome
            .fitting
            .iter()
            .all(|h| h.country.as_deref() != Some("United States"))
    );
}

#[test]
fn the_real_robots_files_allow_the_new_sources_paths() {
    let mut pages = every_page();
    pages.extend([
        (
            "https://www.mlh.com/robots.txt".to_owned(),
            fixture("mlh/robots.txt"),
        ),
        (
            "https://www.cassini.eu/robots.txt".to_owned(),
            fixture("cassini/robots.txt"),
        ),
        (
            "https://dev.events/robots.txt".to_owned(),
            fixture("dev-events/robots.txt"),
        ),
        (
            "https://api.lu.ma/robots.txt".to_owned(),
            fixture("luma/robots.txt"),
        ),
    ]);
    // ethglobal.com has no robots.txt, which allows everything.
    let inner = fixture_http(&pages);
    let polite = Rc::new(PoliteHttp::with_clock(
        inner.clone(),
        Duration::ZERO,
        Box::new(Instant::now),
        Box::new(|_| {}),
    ));

    let found = |source: &dyn Source| source.fetch().unwrap().len();
    assert_eq!(found(&Mlh::new(polite.clone(), mlh::AMERICAS, today())), 11);
    assert_eq!(found(&EthGlobal::new(polite.clone())), 1);
    assert_eq!(found(&Cassini::new(polite.clone())), 10);
    assert_eq!(found(&DevEvents::new(polite.clone())), 4);
    assert_eq!(
        found(&LumaCalendar::new(polite, luma::NORTH_AMERICA_CALENDARS[0])),
        3
    );
    assert!(
        inner
            .requested
            .borrow()
            .iter()
            .any(|u| u == "https://ethglobal.com/robots.txt")
    );
}
