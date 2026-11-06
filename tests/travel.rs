//! Seam 1: whole Runs in which each Hackathon that Fits gets its way of getting
//! there from Home: local, by train within the Train limit, or by flight. The
//! fake Agent answers travel estimates; nothing touches the network.
//!
//! The test data follows the personal tracker's rule from Brussels-Midi (see
//! docs/background.md): a 4-hour Train limit, flying from BRU or CRL.

mod support;

use hackathon_radar::candidate::Candidate;
use hackathon_radar::sources::Source;
use hackathon_radar::travel::TripKind;
use support::*;

const COUNTRIES: [&str; 3] = ["Belgium", "France", "Germany"];

fn hackathon(name: &str, city: Option<&str>, country: &str) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start: date(2026, 12, 5),
        end: date(2026, 12, 6),
        city: city.map(str::to_owned),
        location: None,
        country: Some(country.into()),
        link: None,
    }
}

fn listing(hackathons: &[Candidate]) -> Vec<Box<dyn Source>> {
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(hackathons.to_vec()),
    })]
}

fn paris() -> Candidate {
    hackathon("Paris Hack", Some("Paris"), "France")
}

fn munich() -> Candidate {
    hackathon("Munich Hack", Some("Munich"), "Germany")
}

/// Estimates for the cities these tests use, as the Agent might answer them.
fn estimating(agent: FakeAgent) -> FakeAgent {
    agent
        .with_estimate("Paris", Some(1.4), Some("CDG"))
        .with_estimate("Munich", Some(6.4), Some("MUC"))
        .with_estimate("Ghent", Some(0.6), None)
        .with_estimate("Lyon", Some(3.9), Some("LYS"))
}

fn harness(travel: &str) -> Harness {
    let mut harness = Harness::new(travel_profile(&COUNTRIES, travel));
    harness.agent = estimating(FakeAgent::approving());
    harness
}

fn alert_text(outcome: &hackathon_radar::run::RunOutcome) -> String {
    outcome.alert.as_ref().unwrap().text.clone()
}

#[test]
fn a_trip_within_the_train_limit_goes_by_train() {
    let mut harness = harness("");

    let outcome = harness.run(&listing(&[paris()]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(*harness.agent.travel_asked.borrow(), [["Paris"]]);
    assert_eq!(outcome.travel_failure, None);
    assert_eq!(outcome.unestimated, 0);
    let trip = harness.store.trip("fake", "Paris Hack").unwrap().unwrap();
    assert_eq!(trip.kind, TripKind::Train);
    assert_eq!(trip.train_minutes, Some(84));
    assert_eq!(trip.label, "train ~1h30");
    assert_eq!(
        alert_text(&outcome),
        "hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
         Paris Hack\nParis, 5–6 Dec 2026 · train ~1h30\nStatus unknown · deadline unknown"
    );
}

#[test]
fn a_trip_over_the_train_limit_becomes_a_flight_and_stays_in_scope() {
    let mut harness = harness("");

    let outcome = harness.run(&listing(&[munich()]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.stored_names(), ["Munich Hack"]);
    let trip = harness.store.trip("fake", "Munich Hack").unwrap().unwrap();
    assert_eq!(trip.kind, TripKind::Flight);
    assert_eq!(trip.airport.as_deref(), Some("MUC"));
    assert_eq!(trip.train_minutes, Some(384));
    assert!(
        alert_text(&outcome)
            .ends_with("Munich, 5–6 Dec 2026 · fly BRU→MUC\nStatus unknown · deadline unknown"),
        "{}",
        alert_text(&outcome)
    );
}

#[test]
fn the_train_limit_comes_from_the_profile() {
    let mut harness = harness("[travel]\ntrain_limit_hours = 7\n");

    harness.run(&listing(&[munich()]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(
        harness.trip_label("Munich Hack").as_deref(),
        Some("train ~6h30")
    );
}

#[test]
fn trips_in_home_city_or_country_within_the_limit_are_local() {
    let mut harness = harness("");
    let brussels = hackathon("Brussels Hack", Some("Brussels"), "Belgium");
    let ghent = hackathon("Ghent Hack", Some("Ghent"), "Belgium");

    harness.run(&listing(&[brussels, ghent]), date(2026, 11, 9), DRY_RUN);

    // Home's own city needs no estimate.
    assert_eq!(*harness.agent.travel_asked.borrow(), [["Ghent"]]);
    assert_eq!(
        harness.trip_label("Brussels Hack").as_deref(),
        Some("local")
    );
    assert_eq!(
        harness.trip_label("Ghent Hack").as_deref(),
        Some("local, train ~30min")
    );
    let ghent = harness.store.trip("fake", "Ghent Hack").unwrap().unwrap();
    assert_eq!(ghent.kind, TripKind::Local);
}

#[test]
fn a_second_run_does_not_ask_again_about_a_cached_city() {
    let mut harness = harness("");
    harness.run(&listing(&[paris(), munich()]), date(2026, 11, 9), DRY_RUN);
    assert_eq!(*harness.agent.travel_asked.borrow(), [["Paris", "Munich"]]);

    // Another Hackathon in a cached city, and one in a new city.
    let sources = listing(&[
        paris(),
        munich(),
        hackathon("Paris AI Days", Some(" paris "), "France"),
        hackathon("Lyon Hack", Some("Lyon"), "France"),
    ]);
    harness.run(&sources, date(2026, 11, 12), DRY_RUN);
    harness.run(&sources, date(2026, 11, 16), DRY_RUN);

    assert_eq!(
        *harness.agent.travel_asked.borrow(),
        [vec!["Paris", "Munich"], vec!["Lyon"]]
    );
    assert_eq!(
        harness.trip_label("Paris AI Days").as_deref(),
        Some("train ~1h30")
    );
    assert_eq!(
        harness.trip_label("Lyon Hack").as_deref(),
        Some("train ~4h")
    );
}

#[test]
fn an_override_beats_the_cache() {
    let mut harness = harness("");
    harness.run(&listing(&[munich(), paris()]), date(2026, 11, 9), DRY_RUN);
    assert_eq!(
        harness.trip_label("Munich Hack").as_deref(),
        Some("fly BRU→MUC")
    );

    // The person corrects the cached estimates in their Profile.
    harness.profile = travel_profile(
        &COUNTRIES,
        "[[travel.overrides]]\ncity = \"munich\"\ntrain_hours = 3.5\n\
         [[travel.overrides]]\ncity = \"Paris\"\ntrain_hours = 4.5\nairport = \"ORY\"\n",
    );
    let outcome = harness.run(&listing(&[munich(), paris()]), date(2026, 11, 12), DRY_RUN);

    assert_eq!(harness.agent.travel_requests(), 1, "nothing new to ask");
    assert_eq!(
        harness.trip_label("Munich Hack").as_deref(),
        Some("train ~3h30")
    );
    assert_eq!(
        harness.trip_label("Paris Hack").as_deref(),
        Some("fly BRU→ORY")
    );
    assert!(
        alert_text(&outcome).contains("Munich, 5–6 Dec 2026 · train ~3h30"),
        "{}",
        alert_text(&outcome)
    );
}

#[test]
fn an_override_that_settles_the_trip_means_the_agent_is_not_asked() {
    let mut harness = harness(
        "[[travel.overrides]]\ncity = \"Strasbourg\"\ntrain_hours = 4.5\nno_practical_flight = true\n",
    );
    let strasbourg = hackathon("Strasbourg Hack", Some("Strasbourg"), "France");

    harness.run(&listing(&[strasbourg]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.agent.travel_requests(), 0);
    assert_eq!(
        harness.trip_label("Strasbourg Hack").as_deref(),
        Some("no practical flight (train ~4h30)")
    );
}

#[test]
fn countries_still_apply_whatever_the_train_time() {
    // Aachen is well within the Train limit, but Germany isn't in Countries.
    let mut harness = Harness::new(travel_profile(&["Belgium", "France"], ""));
    harness.agent = estimating(FakeAgent::approving()).with_estimate("Aachen", Some(1.5), None);
    let aachen = hackathon("Aachen Hack", Some("Aachen"), "Germany");

    let outcome = harness.run(&listing(&[aachen, paris()]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.stored_names(), ["Paris Hack"]);
    assert_eq!(*harness.agent.travel_asked.borrow(), [["Paris"]]);
    assert_eq!(harness.trip_label("Aachen Hack"), None);
    assert!(!alert_text(&outcome).contains("Aachen"));
}

#[test]
fn a_malformed_answer_is_rejected_and_its_cities_are_asked_again() {
    let mut harness = harness("");
    harness.agent.travel_raw = Some(fixture("travel/malformed/airport-name.json"));

    let first = harness.run(&listing(&[paris()]), date(2026, 11, 9), DRY_RUN);

    assert!(
        first
            .travel_failure
            .as_deref()
            .unwrap()
            .contains("doesn't match the travel schema"),
        "{:?}",
        first.travel_failure
    );
    assert_eq!(first.unestimated, 1);
    assert_eq!(first.agent_failure, None, "judging went fine");
    assert_eq!(harness.stored_names(), ["Paris Hack"]);
    assert_eq!(harness.trip_label("Paris Hack"), None);
    assert_eq!(
        alert_text(&first),
        "hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
         Paris Hack\nParis, 5–6 Dec 2026\nStatus unknown · deadline unknown"
    );

    harness.agent.travel_raw = None;
    let second = harness.run(&listing(&[paris()]), date(2026, 11, 12), DRY_RUN);

    assert_eq!(*harness.agent.travel_asked.borrow(), [["Paris"], ["Paris"]]);
    assert_eq!(second.travel_failure, None);
    assert_eq!(second.unestimated, 0);
    assert_eq!(
        harness.trip_label("Paris Hack").as_deref(),
        Some("train ~1h30")
    );
}

#[test]
fn cities_left_out_of_the_answer_are_asked_again() {
    let mut harness = Harness::new(travel_profile(&COUNTRIES, ""));
    harness.agent = FakeAgent::approving().with_estimate("Paris", Some(1.4), Some("CDG"));

    let first = harness.run(&listing(&[paris(), munich()]), date(2026, 11, 9), DRY_RUN);
    assert_eq!(first.unestimated, 1);
    assert_eq!(harness.trip_label("Munich Hack"), None);

    harness.run(&listing(&[paris(), munich()]), date(2026, 11, 12), DRY_RUN);

    assert_eq!(
        *harness.agent.travel_asked.borrow(),
        [vec!["Paris", "Munich"], vec!["Munich"]]
    );
}

#[test]
fn an_agent_that_failed_to_judge_is_not_asked_for_travel_estimates() {
    let mut harness = harness("");
    // Paris is judged, but its city is left out of the travel answer.
    harness.agent = FakeAgent::approving();
    harness.run(&listing(&[paris()]), date(2026, 11, 9), DRY_RUN);

    // A new Candidate needs judging, and the Agent fails.
    harness.agent = estimating(FakeAgent::failing("Claude Code used all 50 steps"));
    let outcome = harness.run(&listing(&[paris(), munich()]), date(2026, 11, 12), DRY_RUN);

    assert!(outcome.agent_failure.is_some());
    assert_eq!(harness.agent.travel_requests(), 0);
    assert_eq!(outcome.travel_failure, None);
    assert_eq!(outcome.unestimated, 1, "Paris waits for the next Run");
}

#[test]
fn estimates_are_kept_per_home() {
    let mut harness = harness("");
    harness.run(&listing(&[paris()]), date(2026, 11, 9), DRY_RUN);

    // The person moves to Lyon: Paris is asked about again, from there.
    harness.profile = hackathon_radar::profile::Profile::from_toml(
        "countries = [\"France\"]\n[home]\ncity = \"Lyon\"\ncountry = \"France\"\nairports = [\"LYS\"]\n",
        std::path::Path::new("test-profile.toml"),
    )
    .unwrap();
    harness.run(&listing(&[paris()]), date(2026, 11, 12), DRY_RUN);

    assert_eq!(*harness.agent.travel_asked.borrow(), [["Paris"], ["Paris"]]);
}

#[test]
fn a_hackathon_without_a_city_gets_no_trip_and_no_question() {
    let mut harness = harness("");
    let somewhere = hackathon("Somewhere Hack", None, "France");

    let outcome = harness.run(&listing(&[somewhere]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.agent.travel_requests(), 0);
    assert_eq!(harness.trip_label("Somewhere Hack"), None);
    assert_eq!(outcome.unestimated, 0);
}
