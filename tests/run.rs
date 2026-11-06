//! Seam 1: whole Runs, driven through the harness with fakes.

mod support;

use hackathon_radar::candidate::Candidate;
use support::*;

const ALERT_ON_9_NOV_2026: &str = "\
hackathon-radar: welcome! 5 Hackathons already fit you

Odoo Hackathon #6
Ramillies, 20–22 Nov 2026
Status unknown · deadline unknown
https://hackathon.be/hackathons/odoo-hackathon-6-2026/

MoMu Hackathon
Antwerp, 21–22 Nov 2026
Status unknown · deadline unknown
https://hackathon.be/hackathons/momu-hackathon-2026/

Hack The Future
Antwerp, 24–25 Nov 2026
Status unknown · deadline unknown
https://hackathon.be/hackathons/hack-the-future-2026/

CrossTechXperience Hackathon #3
Kortrijk, 11–13 Dec 2026
Status unknown · deadline unknown
https://hackathon.be/hackathons/crosstechxperience-hackathon-3-2026/

Hacking Health Liège
Liège, 12 Mar 2027
Status unknown · deadline unknown
https://hackathon.be/hackathons/hacking-health-liege-2027/";

fn candidate(name: &str, country: Option<&str>) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start: date(2026, 12, 5),
        end: date(2026, 12, 6),
        city: Some("Somewhere".into()),
        location: None,
        country: country.map(str::to_owned),
        link: None,
    }
}

#[test]
fn dry_run_stores_what_fits_and_composes_the_alert_without_sending() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(outcome.collected, 6);
    assert!(outcome.failed_sources.is_empty());
    assert_eq!(
        harness.stored_names(),
        [
            "Odoo Hackathon #6",
            "MoMu Hackathon",
            "Hack The Future",
            "CrossTechXperience Hackathon #3",
            "Hacking Health Liège",
        ]
    );
    assert_eq!(outcome.alert.unwrap().text, ALERT_ON_9_NOV_2026);
    assert!(!outcome.sent);
    assert!(harness.channel.sent.is_empty(), "a dry run sends nothing");
}

#[test]
fn stored_hackathons_keep_their_listing_details() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    let stored = harness.store.hackathons().unwrap();
    let odoo = &stored[0];
    assert_eq!(odoo.source, "hackathon.be");
    assert_eq!(odoo.source_key, "odoo-hackathon-6-2026@hackathon.be");
    assert_eq!(
        (odoo.start, odoo.end),
        (date(2026, 11, 20), date(2026, 11, 22))
    );
    assert_eq!(odoo.city.as_deref(), Some("Ramillies"));
    assert_eq!(odoo.country.as_deref(), Some("Belgium"));
    assert_eq!(
        odoo.location.as_deref(),
        Some("9 Rue des Bourlottes, 1367 Ramillies")
    );
    assert_eq!(
        (odoo.first_seen, odoo.last_seen),
        (date(2026, 11, 9), date(2026, 11, 9))
    );
}

#[test]
fn a_hackathon_ending_today_still_fits() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    // EUROAVIA Ideathon runs 3–8 Nov 2026.
    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 8), DRY_RUN);
    assert!(
        harness
            .stored_names()
            .contains(&"EUROAVIA Ideathon".to_owned())
    );

    let mut later = Harness::new(profile(&["Belgium"]));
    later.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);
    assert!(
        !later
            .stored_names()
            .contains(&"EUROAVIA Ideathon".to_owned())
    );
}

#[test]
fn nothing_outside_countries_is_stored_or_alerted() {
    let mut harness = Harness::new(profile(&["France", "Netherlands"]));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(outcome.collected, 6);
    assert!(harness.stored_names().is_empty());
    assert_eq!(outcome.alert, None);
}

#[test]
fn countries_filter_each_candidate_and_unknown_countries_are_left_out() {
    let mut harness = Harness::new(profile(&["belgium", "France"]));
    let source = FakeSource {
        name: "fake".into(),
        result: Ok(vec![
            candidate("Lille Hack", Some("France")),
            candidate("Ghent Hack", Some("Belgium")),
            candidate("Delft Hack", Some("Netherlands")),
            candidate("Nowhere Hack", None),
        ]),
    };

    harness.run(&[Box::new(source)], date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.stored_names(), ["Ghent Hack", "Lille Hack"]);
}

#[test]
fn running_again_updates_rows_instead_of_duplicating_them() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);
    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 12), DRY_RUN);

    let stored = harness.store.hackathons().unwrap();
    assert_eq!(stored.len(), 5);
    assert!(
        stored
            .iter()
            .all(|h| h.first_seen == date(2026, 11, 9) && h.last_seen == date(2026, 11, 12))
    );
}

#[test]
fn without_dry_run_the_alert_goes_to_the_alert_channel_once() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert!(outcome.sent);
    assert_eq!(harness.channel.sent.len(), 1);
    assert_eq!(harness.channel.sent[0].text, ALERT_ON_9_NOV_2026);
}

#[test]
fn nothing_is_sent_when_nothing_fits() {
    let mut harness = Harness::new(profile(&["Japan"]));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert!(!outcome.sent);
    assert!(harness.channel.sent.is_empty());
}

#[test]
fn a_failing_source_is_reported_and_the_others_still_count() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let broken = FakeSource {
        name: "broken".into(),
        result: Err("connection refused".into()),
    };

    let outcome = harness.run(
        &[Box::new(broken), hackathon_be_from_fixture()],
        date(2026, 11, 9),
        DRY_RUN,
    );

    assert_eq!(outcome.failed_sources.len(), 1);
    assert_eq!(outcome.failed_sources[0].source, "broken");
    assert_eq!(outcome.failed_sources[0].error, "connection refused");
    assert_eq!(harness.stored_names().len(), 5);
}
