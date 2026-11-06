//! Seam 1: whole Runs over several Sources, where the same Hackathon is listed
//! more than once and its Duplicate listings are merged (ADR 0008).

mod support;

use std::rc::Rc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use hackathon_radar::candidate::Candidate;
use hackathon_radar::sources::Source;
use hackathon_radar::sources::calendar::IcsCalendar;
use hackathon_radar::sources::devpost::{self, Devpost};
use hackathon_radar::sources::hackathonhub::{self, HackathonHub};
use hackathon_radar::sources::polite::PoliteHttp;
use support::*;

const COMMUNITY_CALENDAR: &str = "https://calendar.example.org/hackathons.ics";
const EVENTS: &str = "https://hackathonhub.eu/events/";

const WELCOME_ON_5_NOV_2026: &str = "\
hackathon-radar: welcome! 2 Hackathons already fit you

Since AI Hackathon 2026
Turku, 6–8 Nov 2026
https://sinceai.ai/hackathon

Junction 2026
Espoo, 13–15 Nov 2026
https://www.hackjunction.com/";

/// Every saved page the fixture Sources read, robots.txt files included.
fn pages() -> Vec<(String, String)> {
    let mut pages = vec![
        (
            "https://hackathonhub.eu/robots.txt".to_owned(),
            fixture("hackathonhub/robots.txt"),
        ),
        (
            hackathonhub::SITEMAP_URL.to_owned(),
            fixture("hackathonhub/sitemap-events.xml"),
        ),
        (
            "https://devpost.com/robots.txt".to_owned(),
            fixture("devpost/robots.txt"),
        ),
        (devpost::API_URL.to_owned(), fixture("devpost/page-1.json")),
        (devpost::page_url(2), fixture("devpost/page-2.json")),
        (COMMUNITY_CALENDAR.to_owned(), fixture("ics/community.ics")),
    ];
    for slug in [
        "big-berlin-hack-350-builders-from-all-over-europe-berlin-2026",
        "since-ai-hackathon-2026-turku-2026-3",
        "netcompany-hackathon-athens-2026-athens-2026",
        "tech-europe-agentic-ai-hack-london-2026",
        "pixel-pushers-jam-ghent-2026",
    ] {
        pages.push((
            format!("{EVENTS}{slug}"),
            fixture(&format!("hackathonhub/{slug}.html")),
        ));
    }
    pages
}

/// The real hackathonhub.eu, Devpost and personal-calendar Sources over saved
/// pages, fetching politely (without waiting) as a real Run does. Since AI
/// Hackathon 2026 is listed on all three.
fn three_sources(harness: &Harness, today: NaiveDate) -> (Rc<FixtureHttp>, Vec<Box<dyn Source>>) {
    let pages = pages();
    let pages: Vec<(&str, String)> = pages.iter().map(|(u, b)| (u.as_str(), b.clone())).collect();
    let fixtures = Rc::new(FixtureHttp::new(&pages));
    let http = Rc::new(PoliteHttp::with_clock(
        fixtures.clone(),
        Duration::ZERO,
        Box::new(Instant::now),
        Box::new(|_| {}),
    ));
    let known = harness.store.listings_from(hackathonhub::NAME).unwrap();
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(HackathonHub::new(http.clone(), known, 40, today)),
        Box::new(Devpost::new(http.clone())),
        Box::new(IcsCalendar::new(http, COMMUNITY_CALENDAR)),
    ];
    (fixtures, sources)
}

#[test]
fn a_hackathon_listed_on_three_sources_is_stored_alerted_and_announced_once() {
    let mut harness = Harness::new(profile(&["Finland"]));
    let today = date(2026, 11, 5);
    let (_, sources) = three_sources(&harness, today);

    let outcome = harness.run(&sources, today, LIVE_RUN);

    // hackathonhub.eu 4, Devpost 4, the calendar 3.
    assert_eq!(outcome.collected, 11);
    assert_eq!(outcome.duplicates, 2);
    assert_eq!(
        harness.stored_names(),
        ["Since AI Hackathon 2026", "Junction 2026"]
    );
    assert_eq!(harness.channel.sent.len(), 1);
    assert_eq!(harness.channel.sent[0].text, WELCOME_ON_5_NOV_2026);
    assert_eq!(
        harness.announced_names(),
        ["Since AI Hackathon 2026", "Junction 2026"]
    );

    // The Hackathon carries its first Source's listing, and keeps all three.
    let stored = harness.store.hackathons().unwrap();
    let since_ai = &stored[0];
    assert_eq!(since_ai.source, "hackathonhub.eu");
    assert_eq!(
        since_ai.source_key,
        format!("{EVENTS}since-ai-hackathon-2026-turku-2026-3")
    );
    let mut listed_on: Vec<String> = harness
        .store
        .listings_of(&since_ai.source, &since_ai.source_key)
        .unwrap()
        .into_iter()
        .map(|l| l.source)
        .collect();
    listed_on.sort();
    assert_eq!(listed_on, ["devpost", "hackathonhub.eu", "ics"]);

    // The Agent judged one Candidate per Hackathon, and was asked nothing else.
    assert_eq!(
        *harness.agent.asked.borrow(),
        [vec!["Since AI Hackathon 2026", "Junction 2026"]]
    );
    assert!(harness.agent.pairs_asked.borrow().is_empty());
}

#[test]
fn later_runs_keep_the_merge_and_announce_nothing_again() {
    let mut harness = Harness::new(profile(&["Finland"]));
    let (_, sources) = three_sources(&harness, date(2026, 11, 2));
    harness.run(&sources, date(2026, 11, 2), LIVE_RUN);

    let (fixtures, sources) = three_sources(&harness, date(2026, 11, 5));
    let outcome = harness.run(&sources, date(2026, 11, 5), LIVE_RUN);

    assert_eq!(outcome.duplicates, 2);
    assert_eq!(outcome.alert, None);
    assert_eq!(harness.channel.sent.len(), 1);
    assert_eq!(
        harness.stored_names(),
        ["Since AI Hackathon 2026", "Junction 2026"]
    );
    assert_eq!(
        harness.store.hackathons().unwrap()[0].last_seen,
        date(2026, 11, 5)
    );
    // hackathonhub.eu event pages stored by the first Run aren't fetched again;
    // only the one that had no structured data is tried again.
    let requested = fixtures.requested.borrow();
    let pages: Vec<&String> = requested.iter().filter(|u| u.starts_with(EVENTS)).collect();
    assert_eq!(pages, [&format!("{EVENTS}pixel-pushers-jam-ghent-2026")]);
}

fn listed(source: &str, name: &str, city: &str, link: &str) -> Candidate {
    Candidate {
        source: source.into(),
        source_key: name.into(),
        name: name.into(),
        start: date(2027, 3, 12),
        end: date(2027, 3, 13),
        city: Some(city.into()),
        location: None,
        country: Some("Belgium".into()),
        link: Some(link.into()),
    }
}

fn source(name: &str, listings: Vec<Candidate>) -> Box<dyn Source> {
    Box::new(FakeSource {
        name: name.into(),
        result: Ok(listings),
    })
}

/// The same day and city, and names only somewhat alike: the rules can't tell.
fn unclear_pair() -> Vec<Box<dyn Source>> {
    vec![
        source(
            "hackathon.be",
            vec![listed(
                "hackathon.be",
                "Hacking Health Camp",
                "Liège",
                "https://hackathon.be/hackathons/hacking-health-liege-2027/",
            )],
        ),
        source(
            "ics",
            vec![listed(
                "ics",
                "Hacking Health Weekend",
                "Luik",
                "https://hackinghealth.be/camp",
            )],
        ),
    ]
}

#[test]
fn an_unclear_pair_goes_to_the_agent_and_its_same_answer_merges_them() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_same_hackathon(
        "Hacking Health Weekend",
        "Hacking Health Camp",
        true,
    );

    let outcome = harness.run(&unclear_pair(), date(2026, 11, 9), LIVE_RUN);

    assert_eq!(
        *harness.agent.pairs_asked.borrow(),
        [vec![(
            "Hacking Health Camp".to_owned(),
            "Hacking Health Weekend".to_owned()
        )]]
    );
    assert_eq!(outcome.duplicates, 1);
    assert_eq!(harness.stored_names(), ["Hacking Health Camp"]);
    let alert = &harness.channel.sent[0].text;
    assert!(
        alert.starts_with("hackathon-radar: welcome! 1 Hackathon already fits you"),
        "{alert}"
    );

    // The answer is stored, so the pair isn't asked again.
    let [first, second] = &unclear_pair()
        .iter()
        .map(|s| s.fetch().unwrap().remove(0))
        .collect::<Vec<_>>()[..]
    else {
        unreachable!()
    };
    assert_eq!(
        harness.store.same_hackathon(first, second).unwrap(),
        Some(true)
    );
    harness.run(&unclear_pair(), date(2026, 11, 12), LIVE_RUN);
    assert_eq!(harness.agent.pairs_asked.borrow().len(), 1);
    assert_eq!(harness.stored_names(), ["Hacking Health Camp"]);
}

#[test]
fn an_unclear_pair_the_agent_calls_different_stays_two_hackathons() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let outcome = harness.run(&unclear_pair(), date(2026, 11, 9), LIVE_RUN);

    assert_eq!(harness.agent.pairs_asked.borrow().len(), 1);
    assert_eq!(outcome.duplicates, 0);
    assert_eq!(
        harness.stored_names(),
        ["Hacking Health Camp", "Hacking Health Weekend"]
    );
}

#[test]
fn an_unclear_pair_waits_while_the_agent_fails_and_is_asked_again() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("Claude Code failed (exit code 1): overloaded");

    let outcome = harness.run(&unclear_pair(), date(2026, 11, 9), LIVE_RUN);

    assert_eq!(outcome.waiting_on_duplicates, 2);
    assert!(outcome.agent_failure.unwrap().contains("overloaded"));
    assert!(harness.stored_names().is_empty());
    assert!(
        harness.channel.sent.is_empty(),
        "neither is announced while it may be the other"
    );

    harness.agent = FakeAgent::approving().with_same_hackathon(
        "Hacking Health Camp",
        "Hacking Health Weekend",
        true,
    );
    let outcome = harness.run(&unclear_pair(), date(2026, 11, 12), LIVE_RUN);

    assert_eq!(outcome.waiting_on_duplicates, 0);
    assert_eq!(harness.agent.pairs_asked.borrow().len(), 1);
    assert_eq!(harness.stored_names(), ["Hacking Health Camp"]);
    assert_eq!(harness.channel.sent.len(), 1);
}

#[test]
fn a_new_listing_of_an_announced_hackathon_is_not_announced_again() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);
    assert_eq!(harness.channel.sent.len(), 1);

    // A calendar now lists Odoo Hackathon #6 too, linking to the same page with tracking.
    let odoo = Candidate {
        source: "ics".into(),
        source_key: "odoo-6@calendar.example.org".into(),
        name: "Odoo Hackathon 6".into(),
        start: date(2026, 11, 20),
        end: date(2026, 11, 22),
        city: None,
        location: None,
        country: Some("Belgium".into()),
        link: Some(
            "https://www.hackathon.be/hackathons/odoo-hackathon-6-2026?utm_source=ics".into(),
        ),
    };
    let outcome = harness.run(
        &[hackathon_be_from_fixture(), source("ics", vec![odoo])],
        date(2026, 11, 12),
        LIVE_RUN,
    );

    assert_eq!(outcome.duplicates, 1);
    assert_eq!(outcome.alert, None);
    assert_eq!(harness.store.hackathons().unwrap().len(), 5);
    assert_eq!(
        harness
            .store
            .listings_of("hackathon.be", "odoo-hackathon-6-2026@hackathon.be")
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn listings_from_one_source_are_merged_too() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let twice = source(
        "hackathonhub.eu",
        vec![
            listed(
                "hackathonhub.eu",
                "Ghent Climate Hack",
                "Ghent",
                "https://climatehack.be/",
            ),
            listed(
                "hackathonhub.eu",
                "Ghent Climate Hack 2027",
                "Gent",
                "https://climatehack.be/?ref=hub",
            ),
        ],
    );

    let outcome = harness.run(&[twice], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(outcome.duplicates, 1);
    assert_eq!(harness.stored_names(), ["Ghent Climate Hack"]);
}
