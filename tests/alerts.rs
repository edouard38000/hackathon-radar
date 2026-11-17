//! Seam 1: which Hackathons an Alert covers across Runs, and when they become Announced.

mod support;

use chrono::NaiveDate;
use hackathon_radar::agent::{ApplicationStatus, Judgement, TravelFunded};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::export::sheet;
use hackathon_radar::report::{self, Scope};
use hackathon_radar::sources::Source;
use support::*;

fn hackathon(name: &str, start: NaiveDate) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start,
        end: start,
        city: Some("Ghent".into()),
        location: None,
        country: Some("Belgium".into()),
        link: Some(format!("https://example.org/{}", name.replace(' ', "-"))),
    }
}

/// A Source listing these Hackathons, the nth starting on 1 Dec 2026 + n days.
fn listing(names: &[&str]) -> Vec<Box<dyn Source>> {
    let hackathons = names
        .iter()
        .enumerate()
        .map(|(i, name)| hackathon(name, date(2026, 12, 1 + i as u32)))
        .collect();
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(hackathons),
    })]
}

/// Judged a Hackathon that Fits, whose applications are closed.
fn closed(deadline: Option<NaiveDate>) -> Judgement {
    Judgement {
        status: ApplicationStatus::Closed,
        deadline,
        ..support::hackathon()
    }
}

fn texts(harness: &Harness) -> Vec<&str> {
    harness
        .channel
        .sent
        .iter()
        .map(|a| a.text.as_str())
        .collect()
}

#[test]
fn the_first_alert_is_a_welcome_alert_and_announces_what_it_lists() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert!(outcome.sent);
    let [welcome] = texts(&harness)[..] else {
        panic!("expected exactly one Alert, got {:?}", texts(&harness));
    };
    assert!(
        welcome.starts_with("hackathon-radar: welcome! 5 Hackathons already fit you\n\n"),
        "{welcome}"
    );
    assert_eq!(harness.announced_names(), harness.stored_names());
    assert!(
        harness
            .store
            .hackathons()
            .unwrap()
            .iter()
            .all(|h| h.announced_on == Some(date(2026, 11, 9)))
    );
}

#[test]
fn the_welcome_alert_lists_6_soonest_first_and_announces_the_rest_too() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let names = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8"];

    harness.run(&listing(&names), date(2026, 11, 9), LIVE_RUN);

    assert_eq!(
        texts(&harness),
        ["hackathon-radar: welcome! 8 Hackathons already fit you

H1
Ghent, 1 Dec 2026
Status unknown · deadline unknown
https://example.org/H1

H2
Ghent, 2 Dec 2026
Status unknown · deadline unknown
https://example.org/H2

H3
Ghent, 3 Dec 2026
Status unknown · deadline unknown
https://example.org/H3

H4
Ghent, 4 Dec 2026
Status unknown · deadline unknown
https://example.org/H4

H5
Ghent, 5 Dec 2026
Status unknown · deadline unknown
https://example.org/H5

H6
Ghent, 6 Dec 2026
Status unknown · deadline unknown
https://example.org/H6

+ 2 more"]
    );
    assert_eq!(harness.announced_names(), names);

    // H7 and H8 were counted in the Welcome Alert, so they don't come back.
    let later = harness.run(&listing(&names), date(2026, 11, 12), LIVE_RUN);
    assert_eq!(later.alert, None);
}

#[test]
fn later_alerts_only_cover_hackathons_that_newly_fit() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(
        &listing(&["Old Hack", "Older Hack"]),
        date(2026, 11, 9),
        LIVE_RUN,
    );

    harness.run(
        &listing(&["Old Hack", "Older Hack", "New Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );

    assert_eq!(harness.channel.sent.len(), 2);
    assert_eq!(
        harness.channel.sent[1].text,
        "hackathon-radar: 1 Hackathon newly fits you

New Hack
Ghent, 3 Dec 2026
Status unknown · deadline unknown
https://example.org/New-Hack"
    );
    assert_eq!(
        harness.announced_names(),
        ["Old Hack", "Older Hack", "New Hack"]
    );
    let new_hack = harness
        .store
        .hackathons()
        .unwrap()
        .into_iter()
        .find(|h| h.name == "New Hack")
        .unwrap();
    assert_eq!(new_hack.announced_on, Some(date(2026, 11, 12)));
}

#[test]
fn later_alerts_are_capped_at_6_too() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&listing(&["First"]), date(2026, 11, 9), LIVE_RUN);

    let names = ["First", "N1", "N2", "N3", "N4", "N5", "N6", "N7"];
    harness.run(&listing(&names), date(2026, 11, 12), LIVE_RUN);

    let later = &harness.channel.sent[1].text;
    assert!(
        later.starts_with("hackathon-radar: 7 Hackathons newly fit you\n\nN1\n"),
        "{later}"
    );
    assert!(later.ends_with("\n\n+ 1 more"), "{later}");
    assert!(!later.contains("N7"), "{later}");
    // N7 was only counted, never shown, so it isn't Announced yet.
    assert_eq!(harness.announced_names(), &names[..7]);
}

#[test]
fn a_backlog_of_new_hackathons_drains_6_per_run_and_none_is_lost() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&listing(&["Welcome Hack"]), date(2026, 11, 9), LIVE_RUN);

    let names = [
        "Welcome Hack",
        "N1",
        "N2",
        "N3",
        "N4",
        "N5",
        "N6",
        "N7",
        "N8",
    ];
    harness.run(&listing(&names), date(2026, 11, 12), LIVE_RUN);
    let first = harness.channel.sent[1].text.clone();
    assert!(
        first.starts_with("hackathon-radar: 8 Hackathons newly fit you\n\nN1\n"),
        "{first}"
    );
    assert!(first.contains("\n\nN6\n"), "{first}");
    assert!(first.ends_with("\n\n+ 2 more"), "{first}");
    assert!(!first.contains("N7") && !first.contains("N8"), "{first}");
    assert_eq!(harness.announced_names(), &names[..7]);

    harness.run(&listing(&names), date(2026, 11, 16), LIVE_RUN);
    assert_eq!(
        harness.channel.sent[2].text,
        "hackathon-radar: 2 Hackathons newly fit you

N7
Ghent, 8 Dec 2026
Status unknown · deadline unknown
https://example.org/N7

N8
Ghent, 9 Dec 2026
Status unknown · deadline unknown
https://example.org/N8"
    );
    assert_eq!(harness.announced_names(), names);

    let after = harness.run(&listing(&names), date(2026, 11, 19), LIVE_RUN);
    assert_eq!(after.alert, None);
    assert_eq!(harness.channel.sent.len(), 3);
}

#[test]
fn printing_without_an_alert_channel_announces_nothing() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.channel.print_only = true;

    let printed = harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), LIVE_RUN);
    assert_eq!(harness.channel.sent.len(), 1, "the Alert is still printed");
    assert!(
        printed
            .alert
            .unwrap()
            .text
            .starts_with("hackathon-radar: welcome! 1 Hackathon already fits you")
    );
    assert!(harness.announced_names().is_empty());

    // Once an Alert channel is set, the person still gets the Welcome Alert.
    harness.channel.print_only = false;
    harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 12), LIVE_RUN);
    assert!(
        harness.channel.sent[1]
            .text
            .starts_with("hackathon-radar: welcome!")
    );
    assert_eq!(harness.announced_names(), ["Ghent Hack"]);
}

#[test]
fn an_announced_hackathon_is_never_included_again() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(
        &listing(&["Ghent Hack", "Liège Hack"]),
        date(2026, 11, 9),
        LIVE_RUN,
    );

    // Found again unchanged.
    let again = harness.run(
        &listing(&["Ghent Hack", "Liège Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );
    assert_eq!(again.alert, None);
    assert!(!again.sent);

    // Missing from one Run, then listed again.
    harness.run(&listing(&[]), date(2026, 11, 16), LIVE_RUN);
    let back = harness.run(
        &listing(&["Ghent Hack", "Liège Hack"]),
        date(2026, 11, 19),
        LIVE_RUN,
    );
    assert_eq!(back.alert, None);

    assert_eq!(harness.channel.sent.len(), 1, "only the Welcome Alert");
}

#[test]
fn nothing_qualifying_sends_no_alert_and_keeps_the_welcome_for_later() {
    let mut harness = Harness::new(profile(&["Japan"]));
    let empty = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);
    assert_eq!(empty.alert, None);
    assert!(!empty.sent);
    assert!(harness.channel.sent.is_empty());

    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&listing(&[]), date(2026, 11, 9), LIVE_RUN);
    harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 12), LIVE_RUN);
    assert_eq!(texts(&harness).len(), 1);
    assert!(
        texts(&harness)[0].starts_with("hackathon-radar: welcome! 1 Hackathon already fits you")
    );
}

#[test]
fn a_dry_run_composes_the_alert_but_announces_nothing() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let dry = harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), DRY_RUN);
    assert!(
        dry.alert
            .unwrap()
            .text
            .starts_with("hackathon-radar: welcome! 1 Hackathon already fits you")
    );
    assert!(harness.channel.sent.is_empty());
    assert!(harness.announced_names().is_empty());

    // The real Run afterwards is still the Welcome Alert, with everything in it.
    harness.run(
        &listing(&["Ghent Hack", "Bruges Hack"]),
        date(2026, 11, 9),
        LIVE_RUN,
    );
    assert!(
        texts(&harness)[0].starts_with("hackathon-radar: welcome! 2 Hackathons already fit you")
    );

    // A dry run after that shows only what's new, and still marks nothing.
    let names = ["Ghent Hack", "Bruges Hack", "Mons Hack"];
    let dry = harness.run(&listing(&names), date(2026, 11, 12), DRY_RUN);
    assert!(
        dry.alert
            .unwrap()
            .text
            .starts_with("hackathon-radar: 1 Hackathon newly fits you\n\nMons Hack\n")
    );
    assert_eq!(harness.announced_names(), ["Ghent Hack", "Bruges Hack"]);
    assert_eq!(harness.channel.sent.len(), 1);
}

#[test]
fn an_alert_that_fails_to_send_announces_nothing_and_is_tried_again() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.channel.fail_with = Some("ntfy is down".into());

    let failed = harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), LIVE_RUN);
    assert_eq!(failed.alert_failure.as_deref(), Some("ntfy is down"));
    assert!(!failed.sent);
    assert_eq!(
        failed.failure.as_deref(),
        Some("the Alert channel failed: ntfy is down")
    );
    assert_eq!(harness.stored_names(), ["Ghent Hack"]);
    assert!(harness.announced_names().is_empty());

    harness.channel.fail_with = None;
    harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 12), LIVE_RUN);
    assert!(
        texts(&harness)[0].starts_with("hackathon-radar: welcome! 1 Hackathon already fits you")
    );
    assert_eq!(harness.announced_names(), ["Ghent Hack"]);
}

// --- Closed Hackathons ---

#[test]
fn the_welcome_alert_leaves_out_closed_hackathons_and_doesnt_count_them() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    // Closed, and soonest, so it would be listed first and push H6 into "+ 1 more".
    harness.agent = FakeAgent::approving().with_verdict("LauzHack", closed(None));
    let names = ["LauzHack", "H1", "H2", "H3", "H4", "H5", "H6"];

    let outcome = harness.run(&listing(&names), date(2026, 11, 9), LIVE_RUN);

    assert!(outcome.sent);
    let [welcome] = texts(&harness)[..] else {
        panic!("expected exactly one Alert, got {:?}", texts(&harness));
    };
    assert!(
        welcome.starts_with("hackathon-radar: welcome! 6 Hackathons already fit you\n\nH1\n"),
        "{welcome}"
    );
    assert!(!welcome.contains("LauzHack"), "{welcome}");
    assert!(!welcome.contains("Closed"), "{welcome}");
    assert!(!welcome.contains("more"), "{welcome}");
    assert_eq!(outcome.announced, 6);
    // It still Fits and stays stored; it just isn't Announced while closed.
    assert!(outcome.fitting.iter().any(|h| h.name == "LauzHack"));
    assert!(harness.stored_names().contains(&"LauzHack".to_owned()));
    assert_eq!(harness.announced_names(), &names[1..]);
}

#[test]
fn a_later_alert_leaves_out_a_closed_hackathon_and_one_that_closed_since() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), LIVE_RUN);

    // Lovable Hack is judged closed; Bruges Hack's deadline passes before the
    // Run that finds it, which means closed too.
    harness.agent = FakeAgent::approving()
        .with_verdict("Lovable Hack", closed(None))
        .with_verdict("Bruges Hack", open_until(date(2026, 11, 10)));
    let only_closed = harness.run(
        &listing(&["Ghent Hack", "Lovable Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );
    assert_eq!(only_closed.alert, None, "nothing open is new");
    assert!(!only_closed.sent);

    let names = ["Ghent Hack", "Lovable Hack", "Bruges Hack", "Mons Hack"];
    let later = harness.run(&listing(&names), date(2026, 11, 16), LIVE_RUN);

    assert_eq!(
        later.alert.unwrap().text,
        "hackathon-radar: 1 Hackathon newly fits you

Mons Hack
Ghent, 4 Dec 2026
Status unknown · deadline unknown
https://example.org/Mons-Hack"
    );
    assert_eq!(harness.announced_names(), ["Ghent Hack", "Mons Hack"]);
}

#[test]
fn a_closed_hackathon_that_reopens_is_announced_then() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict("EDTH Paris", closed(None));
    let names = ["Ghent Hack", "EDTH Paris"];
    harness.run(&listing(&names), date(2026, 11, 9), LIVE_RUN);
    assert_eq!(harness.announced_names(), ["Ghent Hack"]);

    // Its waitlist opens, and the store holds that as it holds any recheck.
    let waitlist = Judgement {
        status: ApplicationStatus::Waitlist,
        ..support::hackathon()
    };
    harness.agent.set_verdict("EDTH Paris", waitlist.clone());
    let edth = hackathon("EDTH Paris", date(2026, 12, 2));
    harness
        .store
        .save_rechecks("fake", &[(&edth, waitlist)], date(2026, 11, 10))
        .unwrap();
    let reopened = harness.run(&listing(&names), date(2026, 11, 10), LIVE_RUN);

    assert_eq!(
        reopened.alert.unwrap().text,
        "hackathon-radar: 1 Hackathon newly fits you

EDTH Paris
Ghent, 2 Dec 2026
Waitlist · deadline unknown
https://example.org/EDTH-Paris"
    );
    assert_eq!(harness.announced_names(), names);
}

#[test]
fn a_hackathon_that_closes_gets_no_reminder_and_no_travel_funded_news() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), LIVE_RUN);

    // The recheck finds it Travel-funded, with a deadline in the reminder
    // window, but closed.
    harness.agent.set_verdict(
        "Ghent Hack",
        Judgement {
            travel_funded: TravelFunded::Full,
            travel_detail: Some("Flights and hotel".into()),
            ..closed(Some(date(2026, 11, 14)))
        },
    );
    let run = harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 12), LIVE_RUN);

    assert_eq!(run.rechecked, 1);
    assert_eq!(run.alert, None);
    assert_eq!(texts(&harness).len(), 1, "only the Welcome Alert");
}

#[test]
fn report_all_and_the_sheet_still_include_a_closed_hackathon() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving().with_verdict("LauzHack", closed(None));
    let today = date(2026, 11, 9);
    harness.run(&listing(&["Ghent Hack", "LauzHack"]), today, LIVE_RUN);
    assert!(!texts(&harness)[0].contains("LauzHack"));

    let collect = |scope| report::collect(&harness.store, &harness.profile, today, scope).unwrap();
    let names = |scope| -> Vec<String> {
        collect(scope)
            .into_iter()
            .map(|l| l.hackathon.name)
            .collect()
    };
    assert_eq!(names(Scope::Fitting), ["Ghent Hack"]);
    assert_eq!(names(Scope::All), ["Ghent Hack", "LauzHack"]);

    let exported = sheet::sheet(&collect(Scope::All), &harness.profile, today).unwrap();
    assert!(exported.text.contains("LauzHack"), "{}", exported.text);
}
