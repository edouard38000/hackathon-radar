//! Seam 2: the hackathonhub.eu, Luma, Devpost and personal ICS Sources against
//! their recorded fixtures, and polite fetching (robots.txt, pacing).

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use hackathon_radar::candidate::Candidate;
use hackathon_radar::http::HttpGet;
use hackathon_radar::sources::Source;
use hackathon_radar::sources::calendar::{self, IcsCalendar};
use hackathon_radar::sources::devpost::{self, Devpost};
use hackathon_radar::sources::hackathonhub::{self, HackathonHub};
use hackathon_radar::sources::luma::{self, LumaCalendar};
use hackathon_radar::sources::polite::PoliteHttp;
use support::*;

const EVENTS: &str = "https://hackathonhub.eu/events/";

fn hub_pages() -> Vec<(String, String)> {
    let mut pages = vec![(
        hackathonhub::SITEMAP_URL.to_owned(),
        fixture("hackathonhub/sitemap-events.xml"),
    )];
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

fn http(pages: &[(String, String)]) -> FixtureHttp {
    let pages: Vec<(&str, String)> = pages.iter().map(|(u, b)| (u.as_str(), b.clone())).collect();
    FixtureHttp::new(&pages)
}

#[allow(clippy::too_many_arguments)]
fn listing(
    source: &str,
    key: &str,
    name: &str,
    (start, end): (chrono::NaiveDate, chrono::NaiveDate),
    city: Option<&str>,
    location: Option<&str>,
    country: Option<&str>,
    link: Option<&str>,
) -> Candidate {
    Candidate {
        source: source.into(),
        source_key: key.into(),
        name: name.into(),
        start,
        end,
        city: city.map(str::to_owned),
        location: location.map(str::to_owned),
        country: country.map(str::to_owned),
        link: link.map(str::to_owned),
    }
}

fn hub(
    slug: &str,
    name: &str,
    days: (chrono::NaiveDate, chrono::NaiveDate),
    city: &str,
    country: &str,
    link: &str,
) -> Candidate {
    listing(
        "hackathonhub.eu",
        &format!("{EVENTS}{slug}"),
        name,
        days,
        Some(city),
        Some(city),
        Some(country),
        Some(link),
    )
}

fn netcompany() -> Candidate {
    hub(
        "netcompany-hackathon-athens-2026-athens-2026",
        "Netcompany Hackathon Athens 2026",
        (date(2026, 11, 12), date(2026, 11, 27)),
        "Athens",
        "Greece",
        "https://netcompany.com/hackathon-athens-2026/",
    )
}

fn big_berlin_hack() -> Candidate {
    hub(
        "big-berlin-hack-350-builders-from-all-over-europe-berlin-2026",
        "BIG BERLIN HACK - 350+ Builders from All Over Europe",
        (date(2026, 11, 28), date(2026, 11, 29)),
        "Berlin",
        "Germany",
        "https://luma.com/bbh",
    )
}

fn since_ai() -> Candidate {
    hub(
        "since-ai-hackathon-2026-turku-2026-3",
        "Since AI Hackathon 2026",
        (date(2026, 11, 6), date(2026, 11, 8)),
        "Turku",
        "Finland",
        "https://sinceai.ai/hackathon",
    )
}

fn london() -> Candidate {
    hub(
        "tech-europe-agentic-ai-hack-london-2026",
        "{Tech: Europe} Agentic AI Hack",
        (date(2026, 9, 19), date(2026, 9, 19)),
        "London",
        "United Kingdom",
        "https://luma.com/ldn-hack",
    )
}

#[test]
fn hackathonhub_reads_each_event_pages_structured_data() {
    let http = http(&hub_pages());
    let source = HackathonHub::new(&http, Vec::new(), 40, date(2026, 11, 5));

    assert_eq!(source.name(), "hackathonhub.eu");
    // Upcoming events (sitemap priority 0.6) come first. The Ghent page has no
    // structured data, so it gives no Candidate.
    assert_eq!(
        source.fetch().unwrap(),
        [netcompany(), big_berlin_hack(), since_ai(), london()]
    );
    let requested = http.requested.borrow();
    assert_eq!(requested[0], hackathonhub::SITEMAP_URL);
    assert_eq!(requested.len(), 6);
    assert!(
        !requested.iter().any(|u| u.contains("edinburgh-2020")),
        "an event from a past year isn't fetched: {requested:?}"
    );
}

#[test]
fn hackathonhub_only_fetches_pages_it_hasnt_stored_up_to_the_cap() {
    let http = http(&hub_pages());
    let source = HackathonHub::new(
        &http,
        vec![big_berlin_hack(), since_ai()],
        1,
        date(2026, 11, 5),
    );

    let candidates = source.fetch().unwrap();

    assert_eq!(
        *http.requested.borrow(),
        [
            hackathonhub::SITEMAP_URL.to_owned(),
            format!("{EVENTS}netcompany-hackathon-athens-2026-athens-2026"),
        ]
    );
    // Stored listings still in the sitemap are returned without fetching their pages.
    assert_eq!(candidates, [big_berlin_hack(), since_ai(), netcompany()]);
}

#[test]
fn hackathonhub_without_its_sitemap_is_an_error() {
    let source = HackathonHub::new(FixtureHttp::new(&[]), Vec::new(), 40, date(2026, 11, 5));
    assert!(source.fetch().is_err());
}

fn luma_listing(
    uid: &str,
    name: &str,
    days: (chrono::NaiveDate, chrono::NaiveDate),
    city: Option<&str>,
    country: Option<&str>,
    slug: &str,
) -> Candidate {
    listing(
        "luma",
        &format!("{uid}@events.lu.ma"),
        name,
        days,
        city,
        None,
        country,
        Some(&format!("https://luma.com/{slug}")),
    )
}

#[test]
fn luma_keeps_the_hack_events_and_places_them_from_their_coordinates() {
    let candidates = luma::parse(&fixture("luma/tech-europe.ics"));

    assert_eq!(
        candidates,
        [
            luma_listing(
                "evt-8WHnNAmytN2nepk",
                "H Company × 42AI | Computer Use Agents Hackathon",
                (date(2026, 11, 7), date(2026, 11, 8)),
                Some("Paris"),
                Some("France"),
                "hz9db5u9",
            ),
            luma_listing(
                "evt-sNWJnECt0wl2HWO",
                "{Tech: Europe} Agentic AI Hack",
                (date(2026, 11, 14), date(2026, 11, 14)),
                Some("Karlsruhe"),
                Some("Germany"),
                "kar-hack",
            ),
            luma_listing(
                "evt-PYaG9FME34om1Gr",
                "BIG BERLIN HACK - 350+ Builders from all over Europe",
                (date(2026, 11, 28), date(2026, 11, 29)),
                Some("Berlin"),
                Some("Germany"),
                "bbh",
            ),
            luma_listing(
                "evt-7tMH9VskMUGCqcE",
                "Computer Use Agents Hackathon - London Edition",
                (date(2026, 11, 28), date(2026, 11, 28)),
                Some("London"),
                Some("United Kingdom"),
                "hcompany-6xo3",
            ),
            // Online, with no address or coordinates: its country is unknown.
            luma_listing(
                "evt-Qw3rTy7UiOp9AsD",
                "Voice AI Hack Night",
                (date(2026, 12, 3), date(2026, 12, 3)),
                None,
                None,
                "voice-hack-night",
            ),
        ]
    );
}

#[test]
fn luma_reads_an_address_when_the_calendar_gives_one() {
    let feed = fixture("luma/tech-europe.ics")
        .replace("SUMMARY:Physical AI Meetup", "SUMMARY:Physical AI Hack");
    let zurich = luma::parse(&feed).into_iter().next().unwrap();
    assert_eq!(zurich.city.as_deref(), Some("Zurich"));
    assert_eq!(zurich.country.as_deref(), Some("Switzerland"));
    assert_eq!(
        zurich.location.as_deref(),
        Some("J floor, Josefstrasse 206, 8005 Zürich, Switzerland")
    );
    assert_eq!(
        zurich.link.as_deref(),
        Some("https://luma.com/physical-ai-zur")
    );
}

#[test]
fn each_luma_calendar_is_fetched_from_its_url() {
    let http = FixtureHttp::new(&[
        (luma::CALENDARS[0], fixture("luma/tech-europe.ics")),
        (
            luma::CALENDARS[1],
            fixture("luma/european-defense-tech-hub.ics"),
        ),
    ]);
    let defense = LumaCalendar::new(&http, luma::CALENDARS[1]);

    let names: Vec<String> = defense
        .fetch()
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();

    assert_eq!(defense.name(), "luma");
    assert_eq!(
        names,
        [
            "European Defense Tech Hackathon - Brussels",
            "{Tech: Europe} Agentic AI Hack"
        ]
    );
    assert_eq!(*http.requested.borrow(), [luma::CALENDARS[1]]);
}

#[test]
fn devpost_reads_every_page_and_keeps_in_person_events() {
    let http = FixtureHttp::new(&[
        (devpost::API_URL, fixture("devpost/page-1.json")),
        (&devpost::page_url(2), fixture("devpost/page-2.json")),
    ]);
    let source = Devpost::new(&http);

    let candidates = source.fetch().unwrap();

    let devpost = |id: &str,
                   name: &str,
                   days,
                   city: Option<&str>,
                   venue: &str,
                   country: Option<&str>,
                   sub: &str| {
        listing(
            "devpost",
            id,
            name,
            days,
            city,
            Some(venue),
            country,
            Some(&format!("https://{sub}.devpost.com/")),
        )
    };
    assert_eq!(
        candidates,
        [
            devpost(
                "30516",
                "Since AI 2026",
                (date(2026, 11, 6), date(2026, 11, 8)),
                Some("Turku"),
                "Kupittaa Campus, Turku",
                Some("Finland"),
                "sinceai2026",
            ),
            devpost(
                "31655",
                "HackSussex GameJam 2026",
                (date(2026, 12, 5), date(2026, 12, 6)),
                Some("Brighton"),
                "Brighton",
                Some("United Kingdom"),
                "hacksussex-gamejam-2026",
            ),
            // Online-only "Teens Hack Houston" is left out.
            devpost(
                "30463",
                "DeveloperWeek 2027 Hackathon",
                (date(2027, 1, 25), date(2027, 2, 11)),
                None,
                "Santa Clara Convention Center + Online",
                None,
                "developerweek-2027-hackathon",
            ),
            devpost(
                "31980",
                "Mini VikingHacks",
                (date(2026, 11, 3), date(2026, 11, 5)),
                None,
                "Irvington High School",
                None,
                "mini-vikinghacks",
            ),
        ]
    );
    assert_eq!(source.name(), "devpost");
    // Page 2 brings the count to the total, so there is no page 3.
    assert_eq!(
        *http.requested.borrow(),
        [devpost::API_URL.to_owned(), devpost::page_url(2)]
    );
}

#[test]
fn devpost_stops_at_an_empty_page() {
    let http = FixtureHttp::new(&[
        (
            devpost::API_URL,
            fixture("devpost/page-1.json").replace("\"total_count\":5", "\"total_count\":500"),
        ),
        (
            &devpost::page_url(2),
            r#"{"hackathons":[],"meta":{"total_count":500,"per_page":3}}"#.to_owned(),
        ),
    ]);
    assert_eq!(Devpost::new(&http).fetch().unwrap().len(), 2);
    assert_eq!(http.requested.borrow().len(), 2);
}

#[test]
fn the_persons_own_calendar_gives_every_event() {
    let url = "https://calendar.example.org/hackathons.ics";
    let http = FixtureHttp::new(&[(url, fixture("ics/community.ics"))]);
    let source = IcsCalendar::new(&http, url);

    let candidates = source.fetch().unwrap();

    assert_eq!(source.name(), calendar::NAME);
    assert_eq!(
        candidates,
        [
            listing(
                "ics",
                "since-ai-2026@calendar.example.org",
                "Since AI Hackathon",
                (date(2026, 11, 6), date(2026, 11, 8)),
                Some("Turku"),
                Some("Kupittaa Campus, Joukahaisenkatu 3, 20520 Turku, Finland"),
                Some("Finland"),
                Some("https://sinceai.ai/hackathon/?utm_source=community-calendar&utm_medium=ics"),
            ),
            listing(
                "ics",
                "junction-2026@calendar.example.org",
                "Junction 2026",
                (date(2026, 11, 13), date(2026, 11, 15)),
                Some("Espoo"),
                Some("Dipoli, Otakaari 24, 02150 Espoo, Finland"),
                Some("Finland"),
                Some("https://www.hackjunction.com/"),
            ),
            listing(
                "ics",
                "office-hours@calendar.example.org",
                "Builders' office hours (online)",
                (date(2026, 12, 1), date(2026, 12, 1)),
                None,
                None,
                None,
                Some("https://meet.example.org/office-hours"),
            ),
        ]
    );
}

/// A [`PoliteHttp`] over saved pages that never waits.
fn polite(pages: &[(String, String)]) -> (Rc<FixtureHttp>, PoliteHttp<Rc<FixtureHttp>>) {
    let inner = Rc::new(http(pages));
    let polite = PoliteHttp::with_clock(
        inner.clone(),
        Duration::ZERO,
        Box::new(Instant::now),
        Box::new(|_| {}),
    );
    (inner, polite)
}

#[test]
fn robots_disallow_rules_are_honoured_and_read_once_per_site() {
    let mut pages = hub_pages();
    let robots = fixture("hackathonhub/robots.txt").replace(
        "Disallow: /newsletter",
        "Disallow: /newsletter\nDisallow: /events/big-berlin-hack",
    );
    pages.push(("https://hackathonhub.eu/robots.txt".into(), robots));
    let (inner, polite) = polite(&pages);
    let source = HackathonHub::new(&polite, Vec::new(), 40, date(2026, 11, 5));

    let names: Vec<String> = source
        .fetch()
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();

    assert!(!names.iter().any(|n| n.contains("BIG BERLIN")), "{names:?}");
    assert_eq!(names.len(), 3);
    let requested = inner.requested.borrow();
    assert!(
        !requested.iter().any(|u| u.contains("big-berlin-hack")),
        "{requested:?}"
    );
    assert_eq!(
        requested
            .iter()
            .filter(|u| u.ends_with("/robots.txt"))
            .count(),
        1
    );
    assert_eq!(requested[0], "https://hackathonhub.eu/robots.txt");
}

#[test]
fn a_disallowed_url_is_an_error_naming_it() {
    let pages = vec![
        (
            "https://api.lu.ma/robots.txt".to_owned(),
            fixture("luma/robots.txt"),
        ),
        (
            "https://api.lu.ma/insights/x".to_owned(),
            "secret".to_owned(),
        ),
    ];
    let (inner, polite) = polite(&pages);

    let error = polite.get_text("https://api.lu.ma/insights/x").unwrap_err();

    assert!(
        error
            .to_string()
            .contains("disallowed by its site's robots.txt"),
        "{error}"
    );
    assert_eq!(*inner.requested.borrow(), ["https://api.lu.ma/robots.txt"]);
}

#[test]
fn the_real_robots_files_allow_the_paths_sources_fetch() {
    let mut pages = vec![
        (
            "https://devpost.com/robots.txt".to_owned(),
            fixture("devpost/robots.txt"),
        ),
        (
            "https://api.lu.ma/robots.txt".to_owned(),
            fixture("luma/robots.txt"),
        ),
        (
            "https://hackathonhub.eu/robots.txt".to_owned(),
            fixture("hackathonhub/robots.txt"),
        ),
        (devpost::API_URL.to_owned(), fixture("devpost/page-1.json")),
        (devpost::page_url(2), fixture("devpost/page-2.json")),
        (
            luma::CALENDARS[0].to_owned(),
            fixture("luma/tech-europe.ics"),
        ),
    ];
    pages.extend(hub_pages());
    let (_, polite) = polite(&pages);
    let polite = Rc::new(polite);

    assert_eq!(Devpost::new(polite.clone()).fetch().unwrap().len(), 4);
    assert_eq!(
        LumaCalendar::new(polite.clone(), luma::CALENDARS[0])
            .fetch()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        HackathonHub::new(polite, Vec::new(), 40, date(2026, 11, 5))
            .fetch()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn a_missing_robots_file_allows_everything() {
    let url = "https://calendar.example.org/hackathons.ics";
    let pages = vec![(url.to_owned(), fixture("ics/community.ics"))];
    let (inner, polite) = polite(&pages);

    assert!(polite.get_text(url).is_ok());
    assert_eq!(
        *inner.requested.borrow(),
        ["https://calendar.example.org/robots.txt", url]
    );
}

#[test]
fn requests_to_the_same_site_are_spaced_out() {
    let pages = vec![
        ("https://a.example.org/1".to_owned(), "one".to_owned()),
        ("https://a.example.org/2".to_owned(), "two".to_owned()),
        ("https://b.example.org/1".to_owned(), "three".to_owned()),
    ];
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let slept = Rc::new(RefCell::new(Vec::new()));
    let (now_clock, sleep_clock, sleep_log) = (clock.clone(), clock.clone(), slept.clone());
    let polite = PoliteHttp::with_clock(
        http(&pages),
        Duration::from_secs(1),
        Box::new(move || *now_clock.borrow()),
        Box::new(move |d| {
            sleep_log.borrow_mut().push(d);
            *sleep_clock.borrow_mut() += d;
        }),
    )
    .ignoring_robots();

    polite.get_text("https://a.example.org/1").unwrap();
    *clock.borrow_mut() += Duration::from_millis(300);
    polite.get_text("https://a.example.org/2").unwrap();
    polite.get_text("https://b.example.org/1").unwrap();

    assert_eq!(*slept.borrow(), [Duration::from_millis(700)]);
}
