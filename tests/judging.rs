//! Seam 1: whole Runs in which the Agent judges the Candidates that passed the
//! data-only rules, through the harness's fake Agent.

mod support;

use std::path::Path;

use chrono::{Days, NaiveDate};
use hackathon_radar::agent::{INSTRUCTIONS_VERSION, TravelFunded};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::judging::MAX_CANDIDATES_PER_RUN;
use hackathon_radar::profile::{Eligibility, Profile};
use hackathon_radar::sources::Source;
use support::*;

const BELGIAN_HACKATHONS: [&str; 5] = [
    "Odoo Hackathon #6",
    "MoMu Hackathon",
    "Hack The Future",
    "CrossTechXperience Hackathon #3",
    "Hacking Health Liège",
];

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|n| n.to_string()).collect()
}

fn without(name: &str) -> Vec<String> {
    BELGIAN_HACKATHONS
        .iter()
        .filter(|n| **n != name)
        .map(|n| n.to_string())
        .collect()
}

fn candidate(name: &str, day: u32) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start: date(2026, 12, day),
        end: date(2026, 12, day),
        city: Some("Ghent".into()),
        location: None,
        country: Some("Belgium".into()),
        link: None,
    }
}

#[test]
fn the_agent_is_sent_the_candidates_that_passed_the_rules_with_the_eligibility_facts() {
    let profile = Profile::from_toml(
        "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n\
         [eligibility]\nstudent = true\nnationalities = [\"French\"]\nage = 23\n",
        Path::new("test-profile.toml"),
    )
    .unwrap();
    let mut harness = Harness::new(profile);

    // EUROAVIA Ideathon ended on 8 Nov, so the rules drop it before the Agent sees it.
    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(*harness.agent.asked.borrow(), [names(&BELGIAN_HACKATHONS)]);
    assert_eq!(
        *harness.agent.eligibility.borrow(),
        [Eligibility {
            student: Some(true),
            nationalities: vec!["French".into()],
            age: Some(23),
        }]
    );
    assert_eq!(outcome.judged, 5);
    assert_eq!(outcome.unjudged, 0);
    assert_eq!(outcome.agent_failure, None);
}

#[test]
fn non_hackathons_are_excluded() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict("MoMu Hackathon", not_a_hackathon());

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(harness.stored_names(), without("MoMu Hackathon"));
    let alert = &harness.channel.sent[0].text;
    assert!(
        alert.starts_with("hackathon-radar: welcome! 4 Hackathons already fit you"),
        "{alert}"
    );
    assert!(!alert.contains("MoMu"), "{alert}");
    assert_eq!(outcome.fitting.len(), 4);

    let momu = harness
        .store
        .judgements()
        .unwrap()
        .into_iter()
        .find(|j| j.name == "MoMu Hackathon")
        .expect("the judgement is stored even when it doesn't fit");
    assert!(!momu.judgement.is_hackathon);
    assert_eq!(
        momu.judgement.reason,
        "A pitch competition; nothing is built."
    );
}

#[test]
fn events_the_person_is_not_eligible_for_are_excluded() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict(
        "Hack The Future",
        ineligible("Students of Belgian universities only"),
    );

    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.stored_names(), without("Hack The Future"));
    let judgements = harness.store.judgements().unwrap();
    let htf = judgements
        .iter()
        .find(|j| j.name == "Hack The Future")
        .unwrap();
    assert!(htf.judgement.is_hackathon);
    assert!(!htf.judgement.eligible);
    assert_eq!(
        htf.judgement.eligibility_restriction.as_deref(),
        Some("Students of Belgian universities only")
    );
}

#[test]
fn travel_funded_status_is_stored_with_its_detail() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving()
        .with_verdict(
            "Odoo Hackathon #6",
            travel_funded(TravelFunded::Full, "Train tickets and two hotel nights"),
        )
        .with_verdict(
            "Hacking Health Liège",
            travel_funded(TravelFunded::Partial, "Up to €100 of travel for students"),
        );

    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(harness.stored_names(), names(&BELGIAN_HACKATHONS));
    let judgements = harness.store.judgements().unwrap();
    let travel: Vec<(&str, TravelFunded, Option<&str>)> = judgements
        .iter()
        .map(|j| {
            (
                j.name.as_str(),
                j.judgement.travel_funded,
                j.judgement.travel_detail.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        travel,
        [
            ("CrossTechXperience Hackathon #3", TravelFunded::None, None),
            ("Hack The Future", TravelFunded::None, None),
            (
                "Hacking Health Liège",
                TravelFunded::Partial,
                Some("Up to €100 of travel for students")
            ),
            ("MoMu Hackathon", TravelFunded::None, None),
            (
                "Odoo Hackathon #6",
                TravelFunded::Full,
                Some("Train tickets and two hotel nights")
            ),
        ]
    );
    let odoo = judgements
        .iter()
        .find(|j| j.name == "Odoo Hackathon #6")
        .unwrap();
    assert_eq!(odoo.agent, "fake");
    assert_eq!(odoo.instructions_version, INSTRUCTIONS_VERSION);
    assert_eq!(odoo.judged_on, date(2026, 11, 9));
    assert_eq!(odoo.source_key, "odoo-hackathon-6-2026@hackathon.be");
}

#[test]
fn judged_candidates_are_not_sent_to_the_agent_again() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict("MoMu Hackathon", not_a_hackathon());

    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);
    let second = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 12), DRY_RUN);

    assert_eq!(harness.agent.requests(), 1);
    assert_eq!(second.judged, 0);
    assert_eq!(
        second.fitting.len(),
        4,
        "the stored judgements still decide Fit"
    );
}

#[test]
fn when_the_agent_fails_the_candidates_are_kept_and_judged_on_the_next_run() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("Claude Code used all 50 steps before answering");

    let failed = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(
        failed.agent_failure.as_deref(),
        Some("Claude Code used all 50 steps before answering")
    );
    assert_eq!((failed.judged, failed.unjudged), (0, 5));
    assert!(failed.fitting.is_empty());
    assert!(harness.stored_names().is_empty());
    assert!(harness.store.judgements().unwrap().is_empty());
    assert!(
        harness.channel.sent.is_empty(),
        "nothing was judged, so nothing fits yet"
    );
    let queued = harness.store.unjudged().unwrap();
    assert_eq!(queued.len(), 5);
    assert!(queued.iter().all(|u| u.attempts == 1
        && u.first_queued == date(2026, 11, 9)
        && u.last_error.as_deref() == Some("Claude Code used all 50 steps before answering")));

    harness.agent = FakeAgent::approving();
    let next = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 12), LIVE_RUN);

    assert_eq!(*harness.agent.asked.borrow(), [names(&BELGIAN_HACKATHONS)]);
    assert_eq!(next.agent_failure, None);
    assert_eq!((next.judged, next.unjudged), (5, 0));
    assert_eq!(harness.stored_names(), names(&BELGIAN_HACKATHONS));
    assert!(harness.unjudged_names().is_empty());
    assert_eq!(harness.channel.sent.len(), 1);
}

#[test]
fn output_that_does_not_match_the_schema_is_rejected_and_retried_next_run() {
    let invalid = [
        "I couldn't open the pages, sorry.",
        r#"{"judgements": [{"id": "1", "is_hackathon": "probably"}]}"#,
        r#"{"judgements": [], "note": "nothing to say"}"#,
    ];
    for answer in invalid {
        let mut harness = Harness::new(profile(&["Belgium"]));
        harness.agent = FakeAgent::answering(answer);

        let rejected = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

        let failure = rejected.agent_failure.expect("the answer is rejected");
        assert!(
            failure.contains("doesn't match the judgements schema"),
            "{failure}"
        );
        assert!(harness.store.judgements().unwrap().is_empty(), "{answer}");
        assert!(harness.stored_names().is_empty());
        assert_eq!(harness.unjudged_names(), names(&BELGIAN_HACKATHONS));

        harness.agent = FakeAgent::approving();
        harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 12), DRY_RUN);

        assert_eq!(harness.agent.requests(), 1);
        assert_eq!(harness.stored_names(), names(&BELGIAN_HACKATHONS));
        assert!(harness.unjudged_names().is_empty());
    }
}

#[test]
fn queued_candidates_are_judged_even_when_their_source_fails_next_time() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("timed out");
    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    harness.agent = FakeAgent::approving();
    let broken = FakeSource {
        name: "hackathon.be".into(),
        result: Err("connection refused".into()),
    };
    let next = harness.run(&[Box::new(broken)], date(2026, 11, 12), DRY_RUN);

    assert_eq!(next.collected, 0);
    assert_eq!(next.judged, 5);
    assert_eq!(harness.stored_names(), names(&BELGIAN_HACKATHONS));
}

#[test]
fn queued_candidates_that_no_longer_pass_the_rules_leave_the_queue() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("timed out");
    let source = || -> Vec<Box<dyn hackathon_radar::sources::Source>> {
        vec![Box::new(FakeSource {
            name: "fake".into(),
            result: Ok(vec![candidate("Early Hack", 1), candidate("Late Hack", 20)]),
        })]
    };
    harness.run(&source(), date(2026, 11, 9), DRY_RUN);
    assert_eq!(harness.unjudged_names(), ["Early Hack", "Late Hack"]);

    // By 10 Dec, Early Hack is over; the Source no longer lists it either.
    harness.agent = FakeAgent::approving();
    let next = harness.run(&[], date(2026, 12, 10), DRY_RUN);

    assert_eq!(*harness.agent.asked.borrow(), [names(&["Late Hack"])]);
    assert_eq!(next.unjudged, 0);
    assert!(harness.unjudged_names().is_empty());
    assert_eq!(harness.stored_names(), ["Late Hack"]);
}

#[test]
fn candidates_the_answer_leaves_out_wait_for_the_next_run() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent {
        leave_out: vec!["Hack The Future".into()],
        ..FakeAgent::approving()
    };

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(outcome.agent_failure, None);
    assert_eq!((outcome.judged, outcome.unjudged), (4, 1));
    assert_eq!(harness.stored_names(), without("Hack The Future"));
    assert_eq!(harness.unjudged_names(), ["Hack The Future"]);
}

#[test]
fn a_run_sends_at_most_a_batch_and_the_rest_wait_their_turn() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let many: Vec<Candidate> = (1..=25)
        .map(|d| candidate(&format!("Hack {d:02}"), d))
        .collect();
    let source = || -> Vec<Box<dyn hackathon_radar::sources::Source>> {
        vec![Box::new(FakeSource {
            name: "fake".into(),
            result: Ok(many.clone()),
        })]
    };

    let first = harness.run(&source(), date(2026, 11, 9), DRY_RUN);
    assert_eq!(first.judged, MAX_CANDIDATES_PER_RUN);
    assert_eq!(first.unjudged, 25 - MAX_CANDIDATES_PER_RUN);
    assert_eq!(
        harness.agent.asked.borrow()[0][0],
        "Hack 01",
        "soonest first"
    );

    let second = harness.run(&source(), date(2026, 11, 12), DRY_RUN);
    assert_eq!(second.judged, 25 - MAX_CANDIDATES_PER_RUN);
    assert_eq!(second.unjudged, 0);
    assert_eq!(harness.stored_names().len(), 25);
}

/// A Candidate from the fake Source, held in `city`, `country` on one day.
fn held(name: &str, city: &str, country: &str, start: NaiveDate) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start,
        end: start,
        city: Some(city.into()),
        location: None,
        country: Some(country.into()),
        link: None,
    }
}

/// `count` Hackathons in Ghent, one a day from 1 Dec, and `abroad` in Boston,
/// one a day from 1 Mar: every Ghent one is sooner than every Boston one.
fn ghent_and_boston(count: u64, abroad: u64) -> Vec<Candidate> {
    let ghent = (0..count).map(|i| {
        held(
            &format!("Ghent Hack {:02}", i + 1),
            "Ghent",
            "Belgium",
            date(2026, 12, 1) + Days::new(i),
        )
    });
    let boston = (0..abroad).map(|i| {
        held(
            &format!("Boston Hack {:02}", i + 1),
            "Boston",
            "United States",
            date(2027, 3, 1) + Days::new(i),
        )
    });
    ghent.chain(boston).collect()
}

fn listing(candidates: &[Candidate]) -> Vec<Box<dyn Source>> {
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(candidates.to_vec()),
    })]
}

fn numbered(prefix: &str, from: usize, to: usize) -> Vec<String> {
    (from..=to).map(|i| format!("{prefix} {i:02}")).collect()
}

fn travel_funded_anywhere() -> Profile {
    travel_profile(&["Belgium"], "[travel]\ntravel_funded_anywhere = true\n")
}

#[test]
fn candidates_outside_countries_are_judged_every_run_while_more_in_countries_wait() {
    let mut harness = Harness::new(travel_funded_anywhere());
    let listed = ghent_and_boston(40, 12);

    // Forty wait in Countries, more than two batches hold, yet each Run also
    // judges the soonest of those outside Countries.
    for day in [17, 18, 19] {
        harness.run(&listing(&listed), date(2026, 11, day), DRY_RUN);
    }

    let expected = [
        [numbered("Ghent Hack", 1, 15), numbered("Boston Hack", 1, 5)].concat(),
        [numbered("Ghent Hack", 16, 30), numbered("Boston Hack", 6, 10)].concat(),
        // Ten left in Countries: the share they don't use goes to those outside.
        [numbered("Ghent Hack", 31, 40), numbered("Boston Hack", 11, 12)].concat(),
    ];
    assert_eq!(*harness.agent.asked.borrow(), expected);
    assert!(harness.unjudged_names().is_empty());
}

#[test]
fn a_share_one_group_does_not_use_goes_to_the_other() {
    // Few outside Countries: those in Countries take the rest of the batch.
    let mut harness = Harness::new(travel_funded_anywhere());
    let outcome = harness.run(
        &listing(&ghent_and_boston(30, 2)),
        date(2026, 11, 17),
        DRY_RUN,
    );
    assert_eq!(
        *harness.agent.asked.borrow(),
        [[numbered("Ghent Hack", 1, 18), numbered("Boston Hack", 1, 2)].concat()]
    );
    assert_eq!((outcome.judged, outcome.unjudged), (20, 12));

    // Few in Countries: those outside take the rest of the batch.
    let mut harness = Harness::new(travel_funded_anywhere());
    let outcome = harness.run(
        &listing(&ghent_and_boston(3, 30)),
        date(2026, 11, 17),
        DRY_RUN,
    );
    assert_eq!(
        *harness.agent.asked.borrow(),
        [[numbered("Ghent Hack", 1, 3), numbered("Boston Hack", 1, 17)].concat()]
    );
    assert_eq!((outcome.judged, outcome.unjudged), (20, 13));
}

#[test]
fn started_candidates_leave_the_queue_without_being_sent_to_the_agent() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let ongoing = Candidate {
        end: date(2026, 11, 20),
        ..held("Ongoing Hack", "Ghent", "Belgium", date(2026, 11, 15))
    };
    let starts_today = Candidate {
        end: date(2026, 11, 18),
        ..held("Starts Today Hack", "Ghent", "Belgium", date(2026, 11, 17))
    };
    // Devpost gives a submission period, which opens before the event, so its
    // start isn't the day the Hackathon starts: it is still judged.
    let submissions_open = Candidate {
        source: "devpost".into(),
        end: date(2026, 12, 15),
        ..held("Devpost Hack", "Ghent", "Belgium", date(2026, 11, 1))
    };
    let next_week = held("Next Week Hack", "Ghent", "Belgium", date(2026, 11, 24));
    harness.agent = FakeAgent::failing("timed out");
    harness.run(
        &listing(&[ongoing, starts_today, submissions_open, next_week]),
        date(2026, 11, 10),
        DRY_RUN,
    );
    assert_eq!(harness.unjudged_names().len(), 4);

    // A week later two of the queued have started, and the Source lists only
    // a new one that started yesterday.
    harness.agent = FakeAgent::approving();
    let fresh = Candidate {
        end: date(2026, 11, 19),
        ..held("Fresh Ongoing Hack", "Ghent", "Belgium", date(2026, 11, 16))
    };
    let next = harness.run(&listing(&[fresh]), date(2026, 11, 17), DRY_RUN);

    assert_eq!(
        *harness.agent.asked.borrow(),
        [names(&["Devpost Hack", "Next Week Hack"])]
    );
    assert_eq!((next.judged, next.unjudged), (2, 0));
    assert!(harness.unjudged_names().is_empty());
}
