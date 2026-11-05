//! Seam 2: the hackathon.be Source against its recorded calendar.

mod support;

use hackathon_radar::candidate::Candidate;
use hackathon_radar::sources::Source;
use hackathon_radar::sources::hackathon_be::{self, HackathonBe};
use support::*;

fn listing(
    key: &str,
    name: &str,
    (start, end): (chrono::NaiveDate, chrono::NaiveDate),
    city: &str,
    location: &str,
) -> Candidate {
    Candidate {
        source: "hackathon.be".into(),
        source_key: format!("{key}@hackathon.be"),
        name: name.into(),
        start,
        end,
        city: Some(city.into()),
        location: Some(location.into()),
        country: Some("Belgium".into()),
        link: Some(format!("https://hackathon.be/hackathons/{key}/")),
    }
}

#[test]
fn turns_the_saved_calendar_into_candidates() {
    let candidates = hackathon_be::parse(&fixture("hackathon-be/calendar.ics"));

    assert_eq!(
        candidates,
        [
            listing(
                "euroavia-ideathon-bruges-2026",
                "EUROAVIA Ideathon",
                (date(2026, 11, 3), date(2026, 11, 8)),
                "Bruges",
                "Bruges",
            ),
            listing(
                "odoo-hackathon-6-2026",
                "Odoo Hackathon #6",
                (date(2026, 11, 20), date(2026, 11, 22)),
                "Ramillies",
                "9 Rue des Bourlottes, 1367 Ramillies",
            ),
            listing(
                "momu-hackathon-2026",
                "MoMu Hackathon",
                (date(2026, 11, 21), date(2026, 11, 22)),
                "Antwerp",
                "MoMu Library & Dries Van Noten Study Center, Nationalestraat 28, Antwerp",
            ),
            listing(
                "hack-the-future-2026",
                "Hack The Future",
                (date(2026, 11, 24), date(2026, 11, 25)),
                "Antwerp",
                "Antwerp",
            ),
            listing(
                "crosstechxperience-hackathon-3-2026",
                "CrossTechXperience Hackathon #3",
                (date(2026, 12, 11), date(2026, 12, 13)),
                "Kortrijk",
                "Howest, 8500 Kortrijk",
            ),
            listing(
                "hacking-health-liege-2027",
                "Hacking Health Liège",
                (date(2027, 3, 12), date(2027, 3, 12)),
                "Liège",
                "Liège",
            ),
        ]
    );
}

#[test]
fn fetches_only_the_calendar_url() {
    let http = FixtureHttp::new(&[(
        hackathon_be::CALENDAR_URL,
        fixture("hackathon-be/calendar.ics"),
    )]);
    let source = HackathonBe::new(&http);

    assert_eq!(source.name(), "hackathon.be");
    assert_eq!(source.fetch().unwrap().len(), 6);
    assert_eq!(
        *http.requested.borrow(),
        ["https://hackathon.be/calendar.ics"]
    );
}

#[test]
fn a_fetch_failure_is_an_error() {
    let source = HackathonBe::new(FixtureHttp::new(&[]));
    assert!(source.fetch().is_err());
}

#[test]
fn listings_without_a_name_or_start_are_skipped() {
    let calendar = "BEGIN:VCALENDAR\r\n\
        BEGIN:VEVENT\r\nUID:a\r\nDTSTART;VALUE=DATE:20261201\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:No date\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nSUMMARY:No UID\r\nDTSTART;VALUE=DATE:20261201\r\nURL:https://hackathon.be/x/\r\nEND:VEVENT\r\n\
        END:VCALENDAR\r\n";

    let candidates = hackathon_be::parse(calendar);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "No UID");
    // Without a UID, the link identifies the listing.
    assert_eq!(candidates[0].source_key, "https://hackathon.be/x/");
    assert_eq!(candidates[0].city, None);
    assert_eq!(candidates[0].country.as_deref(), Some("Belgium"));
}
