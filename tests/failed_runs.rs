//! Seam 1: whole Runs that fail (ticket 10): the Run history, nothing lost,
//! Alerts still sent, and the "needs attention" Alert after 2 failed Runs in a row.

mod support;

use hackathon_radar::candidate::Candidate;
use hackathon_radar::sources::Source;
use hackathon_radar::store::RunCounts;
use support::*;

const NEEDS_ATTENTION: &str = "hackathon-radar needs attention";

fn candidate(name: &str, day: u32) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start: date(2026, 12, day),
        end: date(2026, 12, day),
        city: Some(format!("{name} City")),
        location: None,
        country: Some("Belgium".into()),
        link: None,
    }
}

/// One Source listing these Hackathons, on 1, 2, 3… December.
fn listing(names: &[&str]) -> Vec<Box<dyn Source>> {
    let candidates = names
        .iter()
        .zip(1..)
        .map(|(name, day)| candidate(name, day))
        .collect();
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(candidates),
    })]
}

fn broken(name: &str) -> Box<dyn Source> {
    Box::new(FakeSource {
        name: name.into(),
        result: Err("connection refused".into()),
    })
}

fn texts(harness: &Harness) -> Vec<String> {
    harness
        .channel
        .sent
        .iter()
        .map(|a| a.text.clone())
        .collect()
}

fn attention_alerts(harness: &Harness) -> usize {
    harness
        .channel
        .sent
        .iter()
        .filter(|a| a.text.contains(NEEDS_ATTENTION))
        .count()
}

/// Whether each recorded Run succeeded, oldest first.
fn history(harness: &Harness) -> Vec<bool> {
    harness
        .store
        .runs()
        .unwrap()
        .iter()
        .map(|r| r.succeeded())
        .collect()
}

#[test]
fn each_run_is_recorded_with_its_counts() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let mut sources = listing(&["Ghent Hack", "Bruges Hack"]);
    sources.push(broken("down"));

    let first = harness.run(&sources, date(2026, 11, 9), LIVE_RUN);
    assert_eq!(first.failure, None, "one Source failing doesn't fail a Run");

    harness.agent = FakeAgent::failing("Claude Code used all 50 steps before answering");
    let second = harness.run(
        &listing(&["Ghent Hack", "Bruges Hack", "Mons Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );

    let runs = harness.store.runs().unwrap();
    assert_eq!(runs.len(), 2);

    assert!(runs[0].succeeded());
    assert_eq!(runs[0].day, date(2026, 11, 9));
    assert!(runs[0].started_at <= runs[0].finished_at);
    assert_eq!(
        runs[0].counts,
        RunCounts {
            collected: 2,
            judged: 2,
            unjudged: 0,
            fit: 2,
            announced: 2,
            sent: true,
            failed_sources: 1,
        }
    );
    assert!(!runs[0].attention_sent);

    assert!(!runs[1].succeeded());
    assert_eq!(
        runs[1].failure.as_deref(),
        Some("the Agent failed: Claude Code used all 50 steps before answering")
    );
    assert_eq!(runs[1].failure, second.failure);
    assert_eq!(runs[1].day, date(2026, 11, 12));
    assert!(runs[0].finished_at <= runs[1].started_at);
    assert_eq!(
        runs[1].counts,
        RunCounts {
            collected: 3,
            judged: 0,
            unjudged: 1,
            fit: 2,
            announced: 0,
            sent: false,
            failed_sources: 0,
        },
        "the two already judged still Fit; nothing new to send"
    );
}

#[test]
fn every_source_failing_fails_the_run() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let outcome = harness.run(&[broken("a"), broken("b")], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(
        outcome.failure.as_deref(),
        Some("every Source failed (a: connection refused)")
    );
    assert_eq!(history(&harness), [false]);
    assert_eq!(harness.store.runs().unwrap()[0].counts.failed_sources, 2);
}

#[test]
fn a_run_without_sources_has_not_failed() {
    let mut harness = Harness::new(profile(&["Belgium"]));

    let outcome = harness.run(&[], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(outcome.failure, None);
    assert_eq!(history(&harness), [true]);
}

#[test]
fn unjudged_candidates_survive_a_failed_run_and_are_judged_on_the_next() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("timed out");

    let failed = harness.run(
        &listing(&["Ghent Hack", "Bruges Hack"]),
        date(2026, 11, 9),
        LIVE_RUN,
    );

    assert!(failed.failure.is_some());
    assert_eq!(failed.unjudged, 2);
    assert_eq!(harness.unjudged_names(), ["Ghent Hack", "Bruges Hack"]);
    // What the failed Run collected is stored.
    let listed: Vec<String> = harness
        .store
        .listings()
        .unwrap()
        .into_iter()
        .map(|l| l.candidate.name)
        .collect();
    assert_eq!(listed, ["Ghent Hack", "Bruges Hack"]);

    // The next Run's Source is down, yet the queued Candidates are judged and alerted.
    harness.agent = FakeAgent::approving();
    let mut sources = listing(&[]);
    sources.push(broken("down"));
    let next = harness.run(&sources, date(2026, 11, 12), LIVE_RUN);

    assert_eq!(next.failure, None);
    assert_eq!(
        *harness.agent.asked.borrow(),
        [["Ghent Hack", "Bruges Hack"]]
    );
    assert_eq!((next.judged, next.unjudged), (2, 0));
    assert!(harness.unjudged_names().is_empty());
    assert_eq!(harness.stored_names(), ["Ghent Hack", "Bruges Hack"]);
    assert_eq!(harness.announced_names(), ["Ghent Hack", "Bruges Hack"]);
    assert_eq!(history(&harness), [false, true]);
}

#[test]
fn a_failed_agent_step_still_sends_an_alert_for_judged_hackathons_not_yet_announced() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    // Judged on a dry run, so stored but not Announced.
    harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), DRY_RUN);
    assert_eq!(harness.stored_names(), ["Ghent Hack"]);
    assert!(harness.announced_names().is_empty());

    // A new Candidate needs judging, and the Agent fails.
    harness.agent = FakeAgent::failing("Claude Code failed (exit code 1): overloaded");
    let outcome = harness.run(
        &listing(&["Ghent Hack", "Bruges Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );

    assert!(outcome.agent_failure.is_some());
    assert!(outcome.failure.is_some());
    assert!(outcome.sent);
    assert!(!outcome.needs_attention, "only one failed Run so far");
    assert_eq!(
        texts(&harness),
        ["hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
          Ghent Hack\nGhent Hack City, 1 Dec 2026\nStatus unknown · deadline unknown"]
    );
    assert_eq!(harness.announced_names(), ["Ghent Hack"]);
    assert_eq!(harness.unjudged_names(), ["Bruges Hack"]);
}

// Ticket 09 (#10) adds Deadline reminders. Whichever of #10 and #11 merges
// second adds the test "a Deadline reminder is sent on a Run whose Agent failed" here.

#[test]
fn two_failed_runs_in_a_row_send_exactly_one_needs_attention_alert() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("Claude Code used all 50 steps\nbefore answering");
    let sources = || listing(&["Ghent Hack"]);

    let first = harness.run(&sources(), date(2026, 11, 9), LIVE_RUN);
    assert!(!first.needs_attention);
    assert!(harness.channel.sent.is_empty(), "nothing to send yet");

    let second = harness.run(&sources(), date(2026, 11, 12), LIVE_RUN);
    assert!(second.needs_attention);
    assert_eq!(
        texts(&harness),
        ["hackathon-radar needs attention: the last 2 Runs failed.\n\
          Last error: the Agent failed: Claude Code used all 50 steps before answering\n\
          Run `hackathon-radar doctor` to find out what's wrong."]
    );

    for day in [16, 19, 23] {
        let again = harness.run(&sources(), date(2026, 11, day), LIVE_RUN);
        assert!(again.failure.is_some());
        assert!(!again.needs_attention);
    }
    assert_eq!(attention_alerts(&harness), 1);
    assert_eq!(harness.channel.sent.len(), 1);
    assert_eq!(history(&harness), [false; 5]);
    let attention: Vec<bool> = harness
        .store
        .runs()
        .unwrap()
        .iter()
        .map(|r| r.attention_sent)
        .collect();
    assert_eq!(attention, [false, true, false, false, false]);
}

#[test]
fn a_success_resets_the_count() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let sources = || listing(&["Ghent Hack"]);

    harness.agent = FakeAgent::failing("timed out");
    harness.run(&sources(), date(2026, 11, 2), LIVE_RUN);
    harness.run(&sources(), date(2026, 11, 5), LIVE_RUN);
    assert_eq!(attention_alerts(&harness), 1);

    // A success, then one failure: not two in a row since the success.
    harness.agent = FakeAgent::approving();
    let success = harness.run(&sources(), date(2026, 11, 9), LIVE_RUN);
    assert_eq!(success.failure, None);
    harness.agent = FakeAgent::failing("timed out");
    let once = harness.run(
        &listing(&["Ghent Hack", "Bruges Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );
    assert!(!once.needs_attention);
    assert_eq!(attention_alerts(&harness), 1);

    // A second failure after the success sends the attention Alert again, once.
    let twice = harness.run(
        &listing(&["Ghent Hack", "Bruges Hack"]),
        date(2026, 11, 16),
        LIVE_RUN,
    );
    assert!(twice.needs_attention);
    assert_eq!(attention_alerts(&harness), 2);
    assert_eq!(history(&harness), [false, false, true, false, false]);
}

#[test]
fn the_attention_alert_is_merged_into_the_runs_alert() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    // The first Run judges Ghent Hack, but its Alert can't be sent.
    harness.channel.fail_with = Some("ntfy is down".into());
    let first = harness.run(&listing(&["Ghent Hack"]), date(2026, 11, 9), LIVE_RUN);
    assert_eq!(
        first.failure.as_deref(),
        Some("the Alert channel failed: ntfy is down")
    );

    // The second fails too, at the Agent, and sends one Alert with both parts.
    harness.channel.fail_with = None;
    harness.agent = FakeAgent::failing("timed out");
    let second = harness.run(
        &listing(&["Ghent Hack", "Bruges Hack"]),
        date(2026, 11, 12),
        LIVE_RUN,
    );

    assert!(second.needs_attention);
    assert_eq!(
        texts(&harness),
        ["hackathon-radar needs attention: the last 2 Runs failed.\n\
          Last error: the Agent failed: timed out\n\
          Run `hackathon-radar doctor` to find out what's wrong.\n\n\
          hackathon-radar: welcome! 1 Hackathon already fits you\n\n\
          Ghent Hack\nGhent Hack City, 1 Dec 2026\nStatus unknown · deadline unknown"]
    );
    assert_eq!(second.alert.unwrap().text, texts(&harness)[0]);
    assert_eq!(harness.announced_names(), ["Ghent Hack"]);
}

#[test]
fn when_the_alert_channel_fails_the_attention_alert_waits_for_it() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("timed out");
    let sources = || listing(&["Ghent Hack"]);
    harness.run(&sources(), date(2026, 11, 2), LIVE_RUN);

    harness.channel.fail_with = Some("ntfy is down".into());
    let unsent = harness.run(&sources(), date(2026, 11, 5), LIVE_RUN);
    assert!(unsent.needs_attention);
    assert!(!unsent.sent);
    assert_eq!(unsent.alert_failure.as_deref(), Some("ntfy is down"));
    assert_eq!(
        unsent.failure.as_deref(),
        Some("the Agent failed: timed out; the Alert channel failed: ntfy is down")
    );

    harness.channel.fail_with = None;
    let sent = harness.run(&sources(), date(2026, 11, 9), LIVE_RUN);
    assert!(sent.needs_attention && sent.sent);
    assert!(texts(&harness)[0].contains("the last 3 Runs failed"));

    harness.run(&sources(), date(2026, 11, 12), LIVE_RUN);
    assert_eq!(attention_alerts(&harness), 1);
}

#[test]
fn a_dry_run_is_not_recorded_and_never_counts() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::failing("timed out");
    let sources = || listing(&["Ghent Hack"]);

    harness.run(&sources(), date(2026, 11, 2), LIVE_RUN);
    let dry = harness.run(&sources(), date(2026, 11, 5), DRY_RUN);
    assert!(dry.failure.is_some(), "a dry run still says it failed");
    assert!(!dry.needs_attention);
    assert_eq!(dry.alert, None);
    assert_eq!(history(&harness), [false], "the dry run isn't recorded");
    assert!(harness.channel.sent.is_empty());

    // The dry run didn't count, so this is the second failed Run in a row.
    let second = harness.run(&sources(), date(2026, 11, 9), LIVE_RUN);
    assert!(second.needs_attention);
    assert!(texts(&harness)[0].contains("the last 2 Runs failed"));
    assert_eq!(history(&harness), [false, false]);
}
