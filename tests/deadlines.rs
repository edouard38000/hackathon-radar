//! Seam 1: deadlines, application status and Travel-funded news across Runs
//! (ticket 09): Deadline reminders, rechecks, newly Travel-funded Hackathons,
//! Travel-funded ordering, the 6-entry cap, and what each entry shows.

mod support;

use chrono::NaiveDate;
use hackathon_radar::agent::{ApplicationStatus, Judgement, TravelFunded};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::sources::Source;
use support::*;

fn hackathon(name: &str, city: &str, country: &str, start: NaiveDate) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start,
        end: start,
        city: Some(city.into()),
        location: None,
        country: Some(country.into()),
        link: Some(format!("https://example.org/{}", name.replace(' ', "-"))),
    }
}

fn ghent(name: &str, start: NaiveDate) -> Candidate {
    hackathon(name, "Ghent", "Belgium", start)
}

fn source(hackathons: &[Candidate]) -> Vec<Box<dyn Source>> {
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(hackathons.to_vec()),
    })]
}

fn texts(harness: &Harness) -> Vec<String> {
    harness
        .channel
        .sent
        .iter()
        .map(|a| a.text.clone())
        .collect()
}

fn with_status(status: ApplicationStatus, deadline: Option<NaiveDate>) -> Judgement {
    Judgement {
        status,
        deadline,
        ..support::hackathon()
    }
}

fn funded(how: TravelFunded, detail: &str, base: Judgement) -> Judgement {
    Judgement {
        travel_funded: how,
        travel_detail: Some(detail.into()),
        ..base
    }
}

fn rechecked(harness: &Harness) -> Vec<Vec<String>> {
    harness.agent.rechecked.borrow().clone()
}

// --- Deadline reminders ---

#[test]
fn a_deadline_reminder_goes_out_exactly_once_3_days_ahead() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent =
        FakeAgent::approving().with_verdict("Ghent Hack", open_until(date(2026, 11, 28)));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);

    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);
    assert_eq!(texts(&harness).len(), 1, "the Welcome Alert");
    assert!(!texts(&harness)[0].contains("days left"));

    // Four days ahead: not yet.
    let early = harness.run(&listed, date(2026, 11, 24), LIVE_RUN);
    assert_eq!(early.alert, None);

    // Three days ahead, though it was Announced long ago.
    let due = harness.run(&listed, date(2026, 11, 25), LIVE_RUN);
    assert!(due.sent);
    assert_eq!(
        texts(&harness)[1],
        "hackathon-radar: 1 deadline reminder

Ghent Hack
Ghent, 10 Dec 2026
Open · deadline 28 Nov 2026 (3 days left)
https://example.org/Ghent-Hack"
    );
    assert_eq!(
        harness
            .store
            .reminded_on("fake", "Ghent Hack", date(2026, 11, 28))
            .unwrap(),
        Some(date(2026, 11, 25))
    );
    // The Announced day is untouched: reminders are tracked apart from it.
    assert_eq!(
        harness.store.hackathons().unwrap()[0].announced_on,
        Some(date(2026, 11, 9))
    );

    for day in [26, 27, 28] {
        let later = harness.run(&listed, date(2026, 11, day), LIVE_RUN);
        assert_eq!(later.alert, None, "no second reminder on {day} Nov");
    }
    assert_eq!(texts(&harness).len(), 2);
}

#[test]
fn the_first_run_within_the_window_reminds_even_when_it_is_not_3_days_ahead() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent =
        FakeAgent::approving().with_verdict("Ghent Hack", open_until(date(2026, 11, 28)));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    // Runs twice a week can skip the day exactly 3 days ahead.
    harness.run(&listed, date(2026, 11, 27), LIVE_RUN);

    assert!(
        texts(&harness)[1].contains("Open · deadline 28 Nov 2026 (1 day left)"),
        "{}",
        texts(&harness)[1]
    );
}

#[test]
fn a_new_hackathon_due_for_a_reminder_gets_one_entry_marked_with_it() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&source(&[]), date(2026, 11, 20), LIVE_RUN);
    harness.agent =
        FakeAgent::approving().with_verdict("Ghent Hack", open_until(date(2026, 11, 22)));

    harness.run(
        &source(&[ghent("Ghent Hack", date(2026, 12, 10))]),
        date(2026, 11, 20),
        LIVE_RUN,
    );

    assert_eq!(
        texts(&harness),
        ["hackathon-radar: welcome! 1 Hackathon already fits you

Ghent Hack
Ghent, 10 Dec 2026
Open · deadline 22 Nov 2026 (2 days left)
https://example.org/Ghent-Hack"]
    );
    let again = harness.run(
        &source(&[ghent("Ghent Hack", date(2026, 12, 10))]),
        date(2026, 11, 21),
        LIVE_RUN,
    );
    assert_eq!(again.alert, None);
}

#[test]
fn the_reminder_days_come_from_the_profile() {
    let profile = travel_profile(&["Belgium"], "[deadlines]\nreminder_days = 5\n");
    let mut harness = Harness::new(profile);
    harness.agent =
        FakeAgent::approving().with_verdict("Ghent Hack", open_until(date(2026, 11, 28)));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    assert_eq!(
        harness.run(&listed, date(2026, 11, 22), LIVE_RUN).alert,
        None
    );
    let due = harness.run(&listed, date(2026, 11, 23), LIVE_RUN);
    assert!(due.alert.unwrap().text.contains("(5 days left)"));
}

#[test]
fn a_reminder_goes_out_even_when_the_agent_fails() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent =
        FakeAgent::approving().with_verdict("Ghent Hack", open_until(date(2026, 11, 28)));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    harness.agent.answer = FakeAnswer::Fail("the Agent ran out of steps".into());
    let new = ghent("Bruges Hack", date(2026, 12, 12));
    let failed = harness.run(
        &source(&[ghent("Ghent Hack", date(2026, 12, 10)), new]),
        date(2026, 11, 25),
        LIVE_RUN,
    );

    assert!(failed.agent_failure.is_some());
    assert_eq!(
        harness.agent.recheck_requests(),
        0,
        "no recheck after a failure"
    );
    assert!(
        texts(&harness)[1].starts_with("hackathon-radar: 1 deadline reminder\n\nGhent Hack\n"),
        "{}",
        texts(&harness)[1]
    );
}

#[test]
fn closed_waitlisted_or_passed_deadlines_get_no_reminder() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let deadline = Some(date(2026, 11, 28));
    harness.agent = FakeAgent::approving()
        .with_verdict(
            "Closed Hack",
            with_status(ApplicationStatus::Closed, deadline),
        )
        .with_verdict(
            "Waitlist Hack",
            with_status(ApplicationStatus::Waitlist, deadline),
        )
        .with_verdict(
            "Unknown Hack",
            with_status(ApplicationStatus::Unknown, deadline),
        )
        .with_verdict("Passed Hack", open_until(date(2026, 11, 20)));
    let listed = source(&[
        ghent("Closed Hack", date(2026, 12, 10)),
        ghent("Waitlist Hack", date(2026, 12, 11)),
        ghent("Unknown Hack", date(2026, 12, 12)),
        ghent("Passed Hack", date(2026, 12, 13)),
    ]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    // The Passed Hack's deadline passes between Runs, so no Run was in its window.
    let run = harness.run(&listed, date(2026, 11, 25), LIVE_RUN);

    assert_eq!(run.alert, None);
    let passed = harness
        .store
        .judgement("fake", "Passed Hack")
        .unwrap()
        .unwrap();
    assert_eq!(
        passed.status,
        ApplicationStatus::Closed,
        "a passed deadline means closed"
    );
    let changes = harness
        .store
        .judgement_changes("fake", "Passed Hack")
        .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (
            changes[0].field.as_str(),
            changes[0].old_value.as_deref(),
            changes[0].new_value.as_deref()
        ),
        ("application_status", Some("open"), Some("closed"))
    );
    assert_eq!(changes[0].changed_on, date(2026, 11, 25));
}

// --- Rechecks ---

#[test]
fn a_recheck_updates_the_status_and_deadline_and_stores_each_change() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict(
        "Ghent Hack",
        with_status(ApplicationStatus::NotOpenYet, None),
    );
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);
    assert!(texts(&harness)[0].contains("\nNot open yet · deadline unknown\n"));

    harness
        .agent
        .set_verdict("Ghent Hack", open_until(date(2026, 11, 30)));
    // Checked a day ago: too soon to ask again.
    harness.run(&listed, date(2026, 11, 10), LIVE_RUN);
    assert!(rechecked(&harness).is_empty());

    let run = harness.run(&listed, date(2026, 11, 12), LIVE_RUN);

    assert_eq!(rechecked(&harness), [["Ghent Hack"]]);
    assert_eq!(run.rechecked, 1);
    assert_eq!(harness.agent.requests(), 1, "rechecks are not new judging");
    let now = harness
        .store
        .judgement("fake", "Ghent Hack")
        .unwrap()
        .unwrap();
    assert_eq!(now.status, ApplicationStatus::Open);
    assert_eq!(now.deadline, Some(date(2026, 11, 30)));
    let changes: Vec<(String, Option<String>, Option<String>)> = harness
        .store
        .judgement_changes("fake", "Ghent Hack")
        .unwrap()
        .into_iter()
        .map(|c| (c.field, c.old_value, c.new_value))
        .collect();
    assert_eq!(
        changes,
        [
            (
                "application_deadline".into(),
                None,
                Some("2026-11-30".into())
            ),
            (
                "application_status".into(),
                Some("not_open_yet".into()),
                Some("open".into())
            ),
        ]
    );
    // A status change alone isn't news; the Deadline reminder comes in its window.
    assert_eq!(run.alert, None);
    // Rechecked today, so not again for 3 days.
    harness.run(&listed, date(2026, 11, 14), LIVE_RUN);
    assert_eq!(rechecked(&harness).len(), 1);
    harness.run(&listed, date(2026, 11, 15), LIVE_RUN);
    assert_eq!(rechecked(&harness).len(), 2);
}

#[test]
fn rechecks_are_capped_nearest_deadline_first_then_nearest_start() {
    let mut harness = Harness::new(travel_profile(
        &["Belgium"],
        "[deadlines]\nrechecks_per_run = 3\n",
    ));
    let mut agent = FakeAgent::approving();
    agent.set_verdict("Late Deadline", open_until(date(2026, 12, 20)));
    agent.set_verdict("Soon Deadline", open_until(date(2026, 11, 30)));
    let listed = source(&[
        ghent("Late Deadline", date(2026, 12, 1)),
        ghent("No Deadline Early", date(2026, 12, 2)),
        ghent("No Deadline Late", date(2026, 12, 9)),
        ghent("Soon Deadline", date(2026, 12, 30)),
        // Starts beyond the 120-day horizon.
        ghent("Far Away", date(2027, 6, 1)),
    ]);
    harness.agent = agent;
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    let first = harness.run(&listed, date(2026, 11, 12), LIVE_RUN);
    assert_eq!(
        rechecked(&harness),
        [["Soon Deadline", "Late Deadline", "No Deadline Early"]]
    );
    assert_eq!(first.rechecked, 3);

    // The one left out goes first on the next Run; the others were just checked.
    harness.run(&listed, date(2026, 11, 13), LIVE_RUN);
    assert_eq!(rechecked(&harness)[1], ["No Deadline Late"]);
}

#[test]
fn a_failed_recheck_keeps_the_hackathons_due_and_the_run_goes_on() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    harness.agent.recheck_fails_with = Some("timed out".into());
    let failed = harness.run(&listed, date(2026, 11, 12), LIVE_RUN);
    assert_eq!(failed.recheck_failure.as_deref(), Some("timed out"));
    assert_eq!(failed.agent_failure, None);

    harness.agent.recheck_fails_with = None;
    let retried = harness.run(&listed, date(2026, 11, 13), LIVE_RUN);
    assert_eq!(retried.rechecked, 1);
    assert_eq!(rechecked(&harness).len(), 2);
}

// --- Newly Travel-funded ---

#[test]
fn a_known_hackathon_that_becomes_travel_funded_is_announced_again_once() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10))]);
    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);

    harness.agent.set_verdict(
        "Ghent Hack",
        funded(
            TravelFunded::Partial,
            "Up to €150 for trains",
            support::hackathon(),
        ),
    );
    let news = harness.run(&listed, date(2026, 11, 12), LIVE_RUN);

    assert!(news.sent);
    assert_eq!(
        texts(&harness)[1],
        "hackathon-radar: 1 Hackathon newly Travel-funded

Ghent Hack
Ghent, 10 Dec 2026 · newly Travel-funded (partial): Up to €150 for trains
Status unknown · deadline unknown
https://example.org/Ghent-Hack"
    );
    let changes = harness
        .store
        .judgement_changes("fake", "Ghent Hack")
        .unwrap();
    assert_eq!(
        (changes[0].field.as_str(), changes[0].new_value.as_deref()),
        ("travel_funded", Some("partial"))
    );

    // Still funded on later rechecks: not news again.
    for day in [16, 19, 23] {
        let later = harness.run(&listed, date(2026, 11, day), LIVE_RUN);
        assert_eq!(later.alert, None, "{day} Nov");
    }
    assert_eq!(rechecked(&harness).len(), 4);
}

#[test]
fn a_hackathon_outside_countries_that_becomes_travel_funded_is_announced_under_the_rule() {
    let rule = "[travel]\ntravel_funded_anywhere = true\n";
    let mut harness = Harness::new(travel_profile(&["Belgium"], rule));
    let lisbon = hackathon("Lisbon Hack", "Lisbon", "Portugal", date(2026, 12, 5));
    let listed = source(&[ghent("Ghent Hack", date(2026, 12, 10)), lisbon]);

    harness.run(&listed, date(2026, 11, 9), LIVE_RUN);
    // Judged, but not Travel-funded, so it doesn't Fit outside Countries.
    assert_eq!(
        *harness.agent.asked.borrow(),
        [["Ghent Hack", "Lisbon Hack"]]
    );
    assert_eq!(harness.stored_names(), ["Ghent Hack"]);
    assert!(!texts(&harness)[0].contains("Lisbon"));

    harness.agent.set_verdict(
        "Lisbon Hack",
        funded(
            TravelFunded::Full,
            "Flights and hotel",
            support::hackathon(),
        ),
    );
    let news = harness.run(&listed, date(2026, 11, 12), LIVE_RUN);

    assert_eq!(rechecked(&harness), [["Lisbon Hack", "Ghent Hack"]]);
    assert!(news.sent);
    assert_eq!(
        texts(&harness)[1],
        "hackathon-radar: 1 Hackathon newly Travel-funded

Lisbon Hack
Lisbon, 5 Dec 2026 · newly Travel-funded (full): Flights and hotel
Status unknown · deadline unknown
https://example.org/Lisbon-Hack"
    );
    assert_eq!(harness.announced_names(), ["Lisbon Hack", "Ghent Hack"]);

    // Now it Fits, but it was already announced, as newly Travel-funded.
    let later = harness.run(&listed, date(2026, 11, 16), LIVE_RUN);
    assert_eq!(later.alert, None);
    assert_eq!(texts(&harness).len(), 2);
}

#[test]
fn without_the_rule_a_hackathon_outside_countries_is_never_judged_or_announced() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict(
        "Lisbon Hack",
        funded(
            TravelFunded::Full,
            "Flights and hotel",
            support::hackathon(),
        ),
    );
    let lisbon = hackathon("Lisbon Hack", "Lisbon", "Portugal", date(2026, 12, 5));

    for day in [9, 12, 16] {
        harness.run(
            &source(std::slice::from_ref(&lisbon)),
            date(2026, 11, day),
            LIVE_RUN,
        );
    }

    assert!(harness.agent.asked.borrow().is_empty());
    assert!(rechecked(&harness).is_empty());
    assert!(harness.channel.sent.is_empty());
}

// --- Ordering, the cap and entries ---

#[test]
fn travel_funded_hackathons_come_first_then_the_rest_soonest_first() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving()
        .with_verdict(
            "Late Funded",
            funded(TravelFunded::Partial, "Lodging", support::hackathon()),
        )
        .with_verdict(
            "Later Funded",
            funded(TravelFunded::Full, "Everything", support::hackathon()),
        );

    harness.run(
        &source(&[
            ghent("Soon", date(2026, 12, 1)),
            ghent("Later Funded", date(2026, 12, 20)),
            ghent("Middle", date(2026, 12, 5)),
            ghent("Late Funded", date(2026, 12, 15)),
        ]),
        date(2026, 11, 9),
        LIVE_RUN,
    );

    let text = texts(&harness).remove(0);
    let names: Vec<&str> = text
        .split("\n\n")
        .skip(1)
        .map(|entry| entry.lines().next().unwrap())
        .collect();
    assert_eq!(names, ["Late Funded", "Later Funded", "Soon", "Middle"]);
}

#[test]
fn the_cap_counts_the_rest_and_carries_reminders_and_new_hackathons_over() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent =
        FakeAgent::approving().with_verdict("Reminded Hack", open_until(date(2026, 12, 1)));
    let reminded = ghent("Reminded Hack", date(2026, 12, 20));
    harness.run(
        &source(std::slice::from_ref(&reminded)),
        date(2026, 11, 9),
        LIVE_RUN,
    );

    let mut listed = vec![reminded];
    for n in 1..=7 {
        listed.push(ghent(&format!("N{n}"), date(2026, 12, n)));
    }
    harness.run(&source(&listed), date(2026, 11, 28), LIVE_RUN);

    let first = &texts(&harness)[1];
    assert!(
        first.starts_with(
            "hackathon-radar: 7 Hackathons newly fit you, 1 deadline reminder\n\nN1\n"
        ),
        "{first}"
    );
    assert!(first.ends_with("\n\n+ 2 more"), "{first}");
    assert!(
        !first.contains("N7") && !first.contains("Reminded Hack"),
        "{first}"
    );
    assert_eq!(
        harness
            .store
            .reminded_on("fake", "Reminded Hack", date(2026, 12, 1))
            .unwrap(),
        None,
        "a counted reminder stays due"
    );

    harness.run(&source(&listed), date(2026, 11, 29), LIVE_RUN);
    assert_eq!(
        texts(&harness)[2],
        "hackathon-radar: 1 Hackathon newly fits you, 1 deadline reminder

N7
Ghent, 7 Dec 2026
Status unknown · deadline unknown
https://example.org/N7

Reminded Hack
Ghent, 20 Dec 2026
Open · deadline 1 Dec 2026 (2 days left)
https://example.org/Reminded-Hack"
    );
    let after = harness.run(&source(&listed), date(2026, 11, 30), LIVE_RUN);
    assert_eq!(after.alert, None);
}

#[test]
fn each_entry_shows_name_city_dates_status_deadline_travel_and_link() {
    let mut harness = Harness::new(travel_profile(&["Belgium", "France"], ""));
    harness.agent = FakeAgent::approving()
        .with_estimate("Paris", Some(1.4), Some("CDG"))
        .with_verdict(
            "Paris Hack",
            with_status(ApplicationStatus::OpenUntilFull, Some(date(2026, 11, 30))),
        )
        .with_verdict(
            "Lille Hack",
            funded(
                TravelFunded::Full,
                "Train tickets and two hotel nights",
                with_status(ApplicationStatus::Waitlist, None),
            ),
        );
    let paris = Candidate {
        end: date(2026, 12, 6),
        ..hackathon("Paris Hack", "Paris", "France", date(2026, 12, 5))
    };
    let lille = hackathon("Lille Hack", "Lille", "France", date(2026, 12, 12));

    let outcome = harness.run(&source(&[paris, lille]), date(2026, 11, 9), DRY_RUN);

    assert_eq!(
        outcome.alert.unwrap().text,
        "hackathon-radar: welcome! 2 Hackathons already fit you

Lille Hack
Lille, 12 Dec 2026 · Travel-funded (full): Train tickets and two hotel nights
Waitlist · deadline unknown
https://example.org/Lille-Hack

Paris Hack
Paris, 5–6 Dec 2026 · train ~1h30
Open until full · deadline 30 Nov 2026
https://example.org/Paris-Hack"
    );
}
