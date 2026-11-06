//! Seam 1: whole Runs in which flights within Countries are checked against
//! the Flight budget. A fake Fare lookup stands in for SerpApi on the person's
//! key; without it, the fake Agent estimates Unverified fares. Nothing touches
//! the network.
//!
//! Home is Brussels, flying from BRU or CRL, with a 4-hour Train limit and a
//! €200 Flight budget, as in the personal tracker (docs/background.md).

mod support;

use hackathon_radar::agent::TravelFunded;
use hackathon_radar::candidate::Candidate;
use hackathon_radar::fares::FareQuery;
use hackathon_radar::money::Currency;
use hackathon_radar::profile::Profile;
use hackathon_radar::run::RunOutcome;
use hackathon_radar::sources::Source;
use support::*;

const COUNTRIES: [&str; 4] = ["Belgium", "France", "Germany", "Portugal"];

fn hackathon(name: &str, city: &str, country: &str, start: (u32, u32), days: u64) -> Candidate {
    let start = date(2026, start.0, start.1);
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start,
        end: start + chrono::Days::new(days - 1),
        city: Some(city.into()),
        location: None,
        country: Some(country.into()),
        link: None,
    }
}

fn munich() -> Candidate {
    hackathon("Munich Hack", "Munich", "Germany", (12, 5), 2)
}

fn lisbon() -> Candidate {
    hackathon("Lisbon Hack", "Lisbon", "Portugal", (12, 12), 2)
}

fn berlin() -> Candidate {
    hackathon("Berlin Hack", "Berlin", "Germany", (12, 19), 1)
}

fn paris() -> Candidate {
    hackathon("Paris Hack", "Paris", "France", (11, 28), 2)
}

fn listing(hackathons: &[Candidate]) -> Vec<Box<dyn Source>> {
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(hackathons.to_vec()),
    })]
}

fn estimating(agent: FakeAgent) -> FakeAgent {
    agent
        .with_estimate("Munich", Some(6.4), Some("MUC"))
        .with_estimate("Lisbon", None, Some("LIS"))
        .with_estimate("Berlin", Some(6.0), Some("BER"))
        .with_estimate("Paris", Some(1.4), Some("CDG"))
}

fn budget_profile(extra: &str) -> Profile {
    travel_profile(
        &COUNTRIES,
        &format!("[travel]\nflight_budget = 200\n{extra}"),
    )
}

/// The second line of every entry here: the Agent gave no status or deadline.
const UNKNOWN: &str = "Status unknown · deadline unknown";

/// A harness with the €200 Flight budget and a SerpApi key.
fn with_key(fares: FakeFares) -> Harness {
    let mut harness = Harness::new(budget_profile(""));
    harness.agent = estimating(FakeAgent::approving());
    harness.fares = Some(fares);
    harness
}

/// A harness with the €200 Flight budget and no SerpApi key.
fn without_key(agent: FakeAgent) -> Harness {
    let mut harness = Harness::new(budget_profile(""));
    harness.agent = estimating(agent);
    harness
}

fn fake_fares() -> FakeFares {
    FakeFares::default()
        .with("MUC", 96, "CRL")
        .with("LIS", 340, "BRU")
        .with("BER", 200, "BRU")
}

fn alert_text(outcome: &RunOutcome) -> String {
    outcome
        .alert
        .as_ref()
        .map(|a| a.text.clone())
        .unwrap_or_default()
}

fn monday() -> chrono::NaiveDate {
    date(2026, 11, 9)
}

#[test]
fn a_whole_run_checks_each_flight_against_the_flight_budget() {
    let mut harness = with_key(fake_fares());
    harness.profile = budget_profile("travel_funded_anywhere = true\n");
    let funded = hackathon("Porto Funded Hack", "Lisbon", "Portugal", (12, 26), 2);
    harness.agent = estimating(FakeAgent::approving()).with_verdict(
        "Porto Funded Hack",
        travel_funded(TravelFunded::Full, "Flights and hotel covered."),
    );

    let outcome = harness.run(
        &listing(&[munich(), lisbon(), berlin(), paris(), funded]),
        monday(),
        LIVE_RUN,
    );

    // Under, over and at the budget; the train trip and the Travel-funded
    // flight need no Fare.
    assert_eq!(
        harness.fares.as_ref().unwrap().airports(),
        ["MUC", "LIS", "BER"]
    );
    assert_eq!(outcome.fares_looked_up, 3);
    assert_eq!(outcome.over_budget, 1);
    assert_eq!(outcome.waiting_on_fares, 0);
    assert_eq!(outcome.fare_failure, None);
    assert_eq!(
        harness.stored_names(),
        [
            "Paris Hack",
            "Munich Hack",
            "Berlin Hack",
            "Porto Funded Hack"
        ]
    );
    // Travel-funded first, then soonest first; the Fare follows the Trip.
    assert_eq!(
        alert_text(&outcome),
        format!(
            "hackathon-radar: welcome! 4 Hackathons already fit you\n\n\
             Porto Funded Hack\nLisbon, 26–27 Dec 2026 · Travel-funded (full): Flights and hotel covered.\n{UNKNOWN}\n\n\
             Paris Hack\nParis, 28–29 Nov 2026 · train ~1h30\n{UNKNOWN}\n\n\
             Munich Hack\nMunich, 5–6 Dec 2026 · fly BRU→MUC · ~€96 from CRL\n{UNKNOWN}\n\n\
             Berlin Hack\nBerlin, 19 Dec 2026 · fly BRU→BER · ~€200\n{UNKNOWN}"
        )
    );
}

#[test]
fn the_lookup_asks_for_the_hackathon_dates_from_every_home_airport() {
    let mut harness = with_key(fake_fares());

    harness.run(&listing(&[munich()]), monday(), DRY_RUN);

    assert_eq!(
        *harness.fares.as_ref().unwrap().asked.borrow(),
        [FareQuery {
            from: vec!["BRU".into(), "CRL".into()],
            to: "MUC".into(),
            outbound: date(2026, 12, 4),
            inbound: date(2026, 12, 7),
            currency: "EUR".into(),
        }]
    );
}

#[test]
fn a_flight_over_the_budget_does_not_fit() {
    let mut harness = with_key(fake_fares());

    let outcome = harness.run(&listing(&[lisbon(), paris()]), monday(), LIVE_RUN);

    assert_eq!(outcome.over_budget, 1);
    assert_eq!(harness.stored_names(), ["Paris Hack"]);
    assert!(
        !alert_text(&outcome).contains("Lisbon"),
        "{}",
        alert_text(&outcome)
    );
    assert_eq!(outcome.fitting.len(), 1);
}

#[test]
fn the_budget_is_in_the_profile_currency() {
    let mut harness = Harness::new(travel_profile(
        &COUNTRIES,
        "[travel]\nflight_budget = 300\n",
    ));
    harness.profile.currency = Currency::parse("USD").unwrap();
    harness.agent = estimating(FakeAgent::approving());
    harness.fares = Some(FakeFares::default().with("LIS", 280, "BRU"));

    let outcome = harness.run(&listing(&[lisbon()]), monday(), DRY_RUN);

    assert_eq!(
        harness.fares.as_ref().unwrap().asked.borrow()[0].currency,
        "USD"
    );
    assert!(
        alert_text(&outcome).contains("Lisbon, 12–13 Dec 2026 · fly BRU→LIS · ~$280"),
        "{}",
        alert_text(&outcome)
    );
}

#[test]
fn the_travel_funded_rule_overrides_the_flight_budget() {
    let mut harness = with_key(fake_fares());
    harness.profile = budget_profile("travel_funded_anywhere = true\n");
    harness.agent = estimating(FakeAgent::approving()).with_verdict(
        "Lisbon Hack",
        travel_funded(TravelFunded::Partial, "A €150 travel grant."),
    );

    let outcome = harness.run(&listing(&[lisbon()]), monday(), DRY_RUN);

    assert_eq!(
        harness.fares.as_ref().unwrap().lookups(),
        0,
        "no Fare needed"
    );
    assert_eq!(outcome.over_budget, 0);
    assert_eq!(harness.stored_names(), ["Lisbon Hack"]);
    assert_eq!(
        alert_text(&outcome),
        format!(
            "hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
             Lisbon Hack\nLisbon, 12–13 Dec 2026 · Travel-funded (partial): A €150 travel grant.\n{UNKNOWN}"
        )
    );
}

#[test]
fn without_the_travel_funded_rule_a_funded_flight_still_needs_its_fare() {
    // travel_funded_anywhere is off by default.
    let mut harness = with_key(fake_fares());
    harness.agent = estimating(FakeAgent::approving())
        .with_verdict(
            "Lisbon Hack",
            travel_funded(TravelFunded::Partial, "A €150 travel grant."),
        )
        .with_verdict(
            "Munich Hack",
            travel_funded(TravelFunded::Full, "Flights and hotel covered."),
        );

    let outcome = harness.run(&listing(&[munich(), lisbon()]), monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().airports(), ["MUC", "LIS"]);
    assert_eq!(outcome.over_budget, 1);
    assert_eq!(harness.stored_names(), ["Munich Hack"]);
}

#[test]
fn without_a_key_the_agent_estimate_is_an_unverified_fare() {
    let mut harness = without_key(
        FakeAgent::approving()
            .with_fare_estimate("MUC", Some(185), Some("BRU"))
            .with_fare_estimate("LIS", Some(420), None),
    );

    let outcome = harness.run(&listing(&[munich(), lisbon()]), monday(), LIVE_RUN);

    assert_eq!(*harness.agent.fares_asked.borrow(), [["MUC", "LIS"]]);
    assert_eq!(outcome.fares_looked_up, 2);
    // An Unverified fare is still compared with the budget.
    assert_eq!(outcome.over_budget, 1);
    assert_eq!(harness.stored_names(), ["Munich Hack"]);
    assert_eq!(
        alert_text(&outcome),
        format!(
            "hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
             Munich Hack\nMunich, 5–6 Dec 2026 · fly BRU→MUC · ~€185 (unverified)\n{UNKNOWN}"
        )
    );
    let stored = harness
        .store
        .fare(&FareQuery {
            from: vec!["BRU".into(), "CRL".into()],
            to: "MUC".into(),
            outbound: date(2026, 12, 4),
            inbound: date(2026, 12, 7),
            currency: "EUR".into(),
        })
        .unwrap()
        .unwrap();
    assert!(!stored.verified);
    assert_eq!(stored.price, Some(185));
}

#[test]
fn a_flight_with_no_fare_yet_is_held_back_until_it_has_one() {
    // The Agent leaves Munich's Fare out of its answer.
    let mut harness = without_key(FakeAgent::approving());

    let first = harness.run(&listing(&[munich(), paris()]), monday(), LIVE_RUN);

    assert_eq!(first.waiting_on_fares, 1);
    // Held back, not dropped: it still Fits and is stored, but no Alert lists it.
    assert_eq!(harness.stored_names(), ["Paris Hack", "Munich Hack"]);
    assert!(
        !alert_text(&first).contains("Munich"),
        "{}",
        alert_text(&first)
    );
    assert_eq!(harness.announced_names(), ["Paris Hack"]);

    harness.agent = estimating(FakeAgent::approving()).with_fare_estimate("MUC", Some(150), None);
    let second = harness.run(&listing(&[munich(), paris()]), date(2026, 11, 12), LIVE_RUN);

    assert_eq!(second.waiting_on_fares, 0);
    assert_eq!(
        alert_text(&second),
        format!(
            "hackathon-radar: 1 Hackathon newly fits you\n\n\
             Munich Hack\nMunich, 5–6 Dec 2026 · fly BRU→MUC · ~€150 (unverified)\n{UNKNOWN}"
        )
    );
}

/// Munich in late January: 11 weeks after `monday()`, beyond the default 10-week horizon.
fn munich_in_january() -> Candidate {
    Candidate {
        start: date(2027, 1, 25),
        end: date(2027, 1, 26),
        ..munich()
    }
}

#[test]
fn a_flight_beyond_the_horizon_is_announced_with_no_fare_yet() {
    let mut harness = with_key(fake_fares());
    let sources = listing(&[munich_in_january()]);

    let first = harness.run(&sources, monday(), LIVE_RUN);

    // Not held back: its application deadline could pass before it's in the horizon.
    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 0);
    assert_eq!(first.waiting_on_fares, 0);
    assert_eq!(first.over_budget, 0);
    assert_eq!(
        alert_text(&first),
        format!(
            "hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
             Munich Hack\nMunich, 25–26 Jan 2027 · fly BRU→MUC · fare checked closer to the date\n{UNKNOWN}"
        )
    );
    assert_eq!(harness.announced_names(), ["Munich Hack"]);

    // A week later it is within the horizon: its Fare is looked up, and it
    // isn't announced again.
    let second = harness.run(&sources, date(2026, 11, 16), LIVE_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 1);
    assert_eq!(second.fitting.len(), 1);
    assert_eq!(second.alert, None);
}

#[test]
fn an_announced_flight_later_found_over_budget_is_simply_not_announced_again() {
    // Lisbon's Fare (€340) is over the €200 budget, but no Fare is looked up
    // until it is inside the horizon.
    let mut harness = with_key(fake_fares());
    let lisbon_in_january = Candidate {
        start: date(2027, 1, 25),
        end: date(2027, 1, 26),
        ..lisbon()
    };
    let sources = listing(&[lisbon_in_january]);

    let first = harness.run(&sources, monday(), LIVE_RUN);
    assert!(alert_text(&first).contains("fare checked closer to the date"));
    assert_eq!(harness.announced_names(), ["Lisbon Hack"]);
    let sent = harness.channel.sent.len();

    let second = harness
        .try_run(&sources, date(2026, 11, 16), LIVE_RUN)
        .expect("no error");

    assert_eq!(harness.fares.as_ref().unwrap().airports(), ["LIS"]);
    assert_eq!(second.over_budget, 1);
    assert!(second.fitting.is_empty());
    assert_eq!(second.alert, None, "no retraction");
    assert_eq!(second.failure, None);
    assert_eq!(harness.channel.sent.len(), sent);
}

#[test]
fn the_horizon_comes_from_the_profile() {
    let mut harness = with_key(fake_fares());
    harness.profile = budget_profile("[fares]\nhorizon_weeks = 2\n");

    // Munich starts in 26 days.
    let outcome = harness.run(&listing(&[munich()]), monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 0);
    assert!(
        alert_text(&outcome).contains("fly BRU→MUC · fare checked closer to the date"),
        "{}",
        alert_text(&outcome)
    );
}

#[test]
fn a_hackathon_that_has_started_is_not_looked_up() {
    let mut harness = with_key(fake_fares());

    let outcome = harness.run(&listing(&[munich()]), date(2026, 12, 5), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 0);
    assert_eq!(outcome.waiting_on_fares, 1);
}

#[test]
fn fares_are_kept_until_the_refresh_interval_passes() {
    let mut harness = with_key(fake_fares());
    let sources = listing(&[munich()]);

    harness.run(&sources, monday(), LIVE_RUN);
    harness.run(&sources, date(2026, 11, 12), LIVE_RUN);
    harness.run(&sources, date(2026, 11, 16), LIVE_RUN);
    assert_eq!(
        harness.fares.as_ref().unwrap().lookups(),
        1,
        "cached for 14 days"
    );

    // 14 days on, the Fare is looked up again, and has gone up.
    harness.fares.as_mut().unwrap().fares[0].1 = 260;
    let outcome = harness.run(&sources, date(2026, 11, 23), LIVE_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 2);
    assert_eq!(outcome.over_budget, 1);
    assert!(outcome.fitting.is_empty());
}

#[test]
fn the_refresh_interval_comes_from_the_profile() {
    let mut harness = with_key(fake_fares());
    harness.profile = budget_profile("[fares]\nrefresh_days = 3\n");
    let sources = listing(&[munich()]);

    harness.run(&sources, monday(), DRY_RUN);
    harness.run(&sources, date(2026, 11, 11), DRY_RUN);
    harness.run(&sources, date(2026, 11, 12), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 2);
}

#[test]
fn a_run_makes_at_most_the_capped_lookups_soonest_first() {
    let mut harness = with_key(fake_fares());
    harness.profile = budget_profile("[fares]\nlookups_per_run = 2\n");
    let sources = listing(&[berlin(), lisbon(), munich()]);

    let first = harness.run(&sources, monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().airports(), ["MUC", "LIS"]);
    assert_eq!(first.fares_looked_up, 2);
    assert_eq!(first.waiting_on_fares, 1, "Berlin waits");

    let second = harness.run(&sources, date(2026, 11, 12), DRY_RUN);

    assert_eq!(
        harness.fares.as_ref().unwrap().airports(),
        ["MUC", "LIS", "BER"]
    );
    assert_eq!(second.waiting_on_fares, 0);
}

#[test]
fn hackathons_sharing_a_route_and_dates_share_one_lookup() {
    let mut harness = with_key(fake_fares());
    let twin = hackathon("Munich AI Night", "Munich", "Germany", (12, 5), 2);

    let outcome = harness.run(&listing(&[munich(), twin]), monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 1);
    assert_eq!(alert_text(&outcome).matches("~€96").count(), 2);
}

#[test]
fn with_a_key_an_unverified_fare_is_replaced_by_a_real_one() {
    let mut harness =
        without_key(FakeAgent::approving().with_fare_estimate("MUC", Some(185), None));
    harness.run(&listing(&[munich()]), monday(), DRY_RUN);

    // The person adds a SerpApi key.
    harness.fares = Some(fake_fares());
    let outcome = harness.run(&listing(&[munich()]), date(2026, 11, 10), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 1);
    assert_eq!(
        harness.agent.fare_requests(),
        1,
        "the Agent isn't asked again"
    );
    assert!(
        alert_text(&outcome).contains("fly BRU→MUC · ~€96 from CRL"),
        "{}",
        alert_text(&outcome)
    );
    assert!(!alert_text(&outcome).contains("unverified"));
}

#[test]
fn a_failed_lookup_stops_this_runs_lookups_and_they_are_retried() {
    let mut harness = with_key(FakeFares {
        fail_with: Some("SerpApi could not look up the Fare: http status: 429".into()),
        ..fake_fares()
    });
    let sources = listing(&[munich(), lisbon()]);

    let first = harness.run(&sources, monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 1);
    assert!(first.fare_failure.as_deref().unwrap().contains("429"));
    assert_eq!(first.waiting_on_fares, 2);
    assert_eq!(first.alert, None);

    harness.fares.as_mut().unwrap().fail_with = None;
    let second = harness.run(&sources, date(2026, 11, 12), DRY_RUN);

    assert_eq!(second.fare_failure, None);
    assert_eq!(second.waiting_on_fares, 0);
    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 3);
}

#[test]
fn a_malformed_fare_answer_is_rejected_and_asked_again() {
    let mut harness = without_key(FakeAgent::approving());
    harness.agent.fare_raw = Some(fixture("fares/malformed/price-as-text.json"));

    let first = harness.run(&listing(&[munich()]), monday(), DRY_RUN);

    assert!(
        first
            .fare_failure
            .as_deref()
            .unwrap()
            .contains("doesn't match the fares schema"),
        "{:?}",
        first.fare_failure
    );
    assert_eq!(first.agent_failure, None, "judging went fine");
    assert_eq!(first.waiting_on_fares, 1);

    harness.agent.fare_raw = None;
    harness.agent.fare_estimates = vec![("MUC".into(), Some(120), Some("CRL".into()))];
    let second = harness.run(&listing(&[munich()]), date(2026, 11, 12), DRY_RUN);

    assert_eq!(*harness.agent.fares_asked.borrow(), [["MUC"], ["MUC"]]);
    assert!(
        alert_text(&second).contains("fly BRU→MUC · ~€120 from CRL (unverified)"),
        "{}",
        alert_text(&second)
    );
}

#[test]
fn an_agent_that_failed_to_judge_is_not_asked_for_fares() {
    let mut harness = without_key(FakeAgent::approving());
    harness.run(&listing(&[munich()]), monday(), DRY_RUN);
    assert_eq!(harness.agent.fare_requests(), 1);

    // A new Candidate needs judging, and the Agent fails.
    harness.agent = estimating(FakeAgent::failing("Claude Code used all 50 steps"));
    let outcome = harness.run(&listing(&[munich(), paris()]), date(2026, 11, 12), DRY_RUN);

    assert!(outcome.agent_failure.is_some());
    assert_eq!(harness.agent.fare_requests(), 0);
    assert_eq!(outcome.waiting_on_fares, 1);
}

#[test]
fn without_a_flight_budget_no_fare_is_looked_up() {
    let mut harness = Harness::new(travel_profile(&COUNTRIES, ""));
    harness.agent = estimating(FakeAgent::approving());
    harness.fares = Some(fake_fares());

    let outcome = harness.run(&listing(&[lisbon()]), monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 0);
    assert_eq!(harness.agent.fare_requests(), 0);
    assert!(alert_text(&outcome).contains("Lisbon, 12–13 Dec 2026 · fly BRU→LIS\n"));
}

#[test]
fn no_deadline_reminder_for_a_flight_over_budget_or_waiting_for_its_fare() {
    // Lisbon is over the budget; Munich has no flight found, so no Fare yet.
    let mut harness = with_key(FakeFares::default().with("LIS", 340, "BRU"));
    let closing = open_until(date(2026, 11, 11));
    harness.agent = estimating(FakeAgent::approving())
        .with_verdict("Lisbon Hack", closing.clone())
        .with_verdict("Munich Hack", closing);

    let outcome = harness.run(&listing(&[munich(), lisbon()]), monday(), LIVE_RUN);

    assert_eq!(outcome.over_budget, 1);
    assert_eq!(outcome.waiting_on_fares, 1);
    assert_eq!(outcome.alert, None, "{}", alert_text(&outcome));
    assert!(harness.announced_names().is_empty());
}

#[test]
fn a_submission_period_gets_no_fare_and_is_not_held_back() {
    // Devpost's dates are a submission window, not the days to be there.
    let mut harness = with_key(fake_fares());
    let devpost = Candidate {
        source: "devpost".into(),
        ..lisbon()
    };

    let outcome = harness.run(&listing(&[devpost]), monday(), DRY_RUN);

    assert_eq!(harness.fares.as_ref().unwrap().lookups(), 0);
    assert_eq!(outcome.waiting_on_fares, 0);
    assert_eq!(outcome.over_budget, 0);
    assert!(
        alert_text(&outcome).contains("Lisbon, 12–13 Dec 2026 · fly BRU→LIS\n"),
        "{}",
        alert_text(&outcome)
    );
}
