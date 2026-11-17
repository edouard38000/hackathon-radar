//! Seam 1: whole Runs in which the Profile's Themes decide which Hackathons
//! Fit (ticket 24). The Agent answers the Theme question as part of judging,
//! through the harness's fake Agent.

mod support;

use hackathon_radar::agent::{ThemeAnswer, ThemeFit};
use support::*;

const BELGIAN_HACKATHONS: [&str; 5] = [
    "Odoo Hackathon #6",
    "MoMu Hackathon",
    "Hack The Future",
    "CrossTechXperience Hackathon #3",
    "Hacking Health Liège",
];

const ODOO: &str = "Odoo Hackathon #6";
const LIEGE: &str = "Hacking Health Liège";

fn without(name: &str) -> Vec<String> {
    BELGIAN_HACKATHONS
        .iter()
        .filter(|n| **n != name)
        .map(|n| n.to_string())
        .collect()
}

fn all() -> Vec<String> {
    BELGIAN_HACKATHONS.iter().map(|n| n.to_string()).collect()
}

fn names(outcome: &hackathon_radar::run::RunOutcome) -> Vec<String> {
    outcome.fitting.iter().map(|c| c.name.clone()).collect()
}

/// The Alert entry that starts with this Hackathon's name.
fn entry<'a>(alert: &'a str, name: &str) -> &'a str {
    alert
        .split("\n\n")
        .find(|e| e.starts_with(name))
        .unwrap_or_else(|| panic!("no entry for {name} in {alert}"))
}

fn theme_answer(harness: &Harness, name: &str) -> Option<ThemeAnswer> {
    harness
        .store
        .judgements()
        .unwrap()
        .into_iter()
        .find(|j| j.name == name)
        .unwrap()
        .judgement
        .theme
}

#[test]
fn with_themes_an_off_theme_hackathon_doesnt_fit_while_on_theme_and_open_themed_ones_do() {
    let mut harness = Harness::new(themed_profile(&["Belgium"], &["climate", "health"]));
    harness.agent = FakeAgent::approving()
        .with_theme_fit(ODOO, ThemeFit::OffTheme)
        .with_theme_fit(LIEGE, ThemeFit::OnTheme("health".into()));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(
        *harness.agent.themes_asked.borrow(),
        [vec!["climate".to_owned(), "health".to_owned()]],
        "the Theme question is part of the one judging request"
    );
    assert_eq!(harness.agent.requests(), 1);
    assert_eq!(names(&outcome), without(ODOO));
    assert_eq!(harness.stored_names(), without(ODOO));
    assert_eq!(harness.announced_names(), without(ODOO));

    let alert = &harness.channel.sent[0].text;
    assert!(
        alert.starts_with("hackathon-radar: welcome! 4 Hackathons already fit you"),
        "{alert}"
    );
    assert!(!alert.contains("Odoo"), "{alert}");
    // The on-Theme entry shows its Theme; open-themed ones show nothing extra.
    assert_eq!(
        entry(alert, LIEGE),
        "Hacking Health Liège\nLiège, 12 Mar 2027\n\
         Status unknown · deadline unknown\nTheme: health\n\
         https://hackathon.be/hackathons/hacking-health-liege-2027/"
    );
    assert_eq!(alert.matches("Theme:").count(), 1, "{alert}");

    // The off-Theme judgement is stored, with the Themes it answers about.
    assert_eq!(
        theme_answer(&harness, ODOO),
        Some(ThemeAnswer {
            asked: vec!["climate".into(), "health".into()],
            fit: ThemeFit::OffTheme,
        })
    );
    // A later Run neither asks again nor lets it Fit.
    let later = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 10), LIVE_RUN);
    assert_eq!(harness.agent.requests(), 1);
    assert_eq!(names(&later), without(ODOO));
    assert_eq!(later.alert, None);
}

#[test]
fn without_themes_the_agent_isnt_asked_about_them_and_every_subject_fits() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    // Would be off-Theme, but nobody asks.
    harness.agent = FakeAgent::approving().with_theme_fit(ODOO, ThemeFit::OffTheme);

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(*harness.agent.themes_asked.borrow(), [Vec::<String>::new()]);
    assert_eq!(names(&outcome), all());
    assert_eq!(harness.announced_names(), all());
    let alert = &harness.channel.sent[0].text;
    assert!(!alert.contains("Theme"), "{alert}");
    assert_eq!(theme_answer(&harness, ODOO), None);
}

#[test]
fn judgements_made_before_themes_were_set_count_as_fitting_until_rechecked() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    harness.agent = FakeAgent::approving()
        .with_theme_fit(ODOO, ThemeFit::OffTheme)
        .with_theme_fit(LIEGE, ThemeFit::OnTheme("health".into()));
    harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    // The person sets Themes. Nothing is judged again at once: the judgements
    // without a Theme answer still Fit, and the Welcome Alert lists them all.
    harness.profile = themed_profile(&["Belgium"], &["climate", "health"]);
    let next = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 10), LIVE_RUN);
    assert_eq!(harness.agent.requests(), 1);
    assert_eq!(harness.agent.recheck_requests(), 0);
    assert_eq!(names(&next), all());
    let alert = &harness.channel.sent[0].text;
    assert!(alert.contains("Odoo"), "{alert}");
    assert!(!alert.contains("Theme:"), "{alert}");

    // Once due, the recheck asks about the Themes, and the off-Theme one stops Fitting.
    let rechecked = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 12), LIVE_RUN);
    assert_eq!(harness.agent.recheck_requests(), 1);
    assert_eq!(
        harness.agent.themes_asked.borrow().last().unwrap(),
        &["climate".to_owned(), "health".to_owned()]
    );
    assert!(harness.agent.rechecked.borrow()[0].contains(&ODOO.to_owned()));
    assert_eq!(names(&rechecked), without(ODOO));
    assert_eq!(
        theme_answer(&harness, LIEGE).map(|a| a.fit),
        Some(ThemeFit::OnTheme("health".into()))
    );
}

#[test]
fn an_answer_about_other_themes_counts_as_fitting_until_rechecked_with_the_new_ones() {
    let mut harness = Harness::new(themed_profile(&["Belgium"], &["climate"]));
    harness.agent = FakeAgent::approving().with_theme_fit(ODOO, ThemeFit::OffTheme);
    let first = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);
    assert_eq!(names(&first), without(ODOO));

    // The person changes Themes: the answer was about climate only.
    harness.profile = themed_profile(&["Belgium"], &["climate", "open source"]);
    harness
        .agent
        .set_theme_fit(ODOO, ThemeFit::OnTheme("open source".into()));
    let next = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 10), DRY_RUN);
    assert_eq!(names(&next), all());
    assert_eq!(harness.agent.recheck_requests(), 0);

    let rechecked = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 12), LIVE_RUN);
    assert_eq!(harness.agent.recheck_requests(), 1);
    assert_eq!(names(&rechecked), all());
    let alert = &harness.channel.sent[0].text;
    assert!(
        entry(alert, ODOO).contains("\nTheme: open source\n"),
        "{alert}"
    );
}

#[test]
fn sloppy_theme_answers_are_normalised_so_the_whole_batch_is_still_judged() {
    let mut harness = Harness::new(themed_profile(&["Belgium"], &["climate", "AI agents"]));
    harness.agent = FakeAgent::approving()
        // A near-miss Theme: counts as open-themed, with no Theme line.
        .with_sloppy_theme(LIEGE, "on_theme", Some("AI"))
        // Off-Theme naming a Theme anyway: stays off-Theme.
        .with_sloppy_theme(ODOO, "off_theme", Some("climate"))
        // Open-themed naming a Theme: stays open-themed, the Theme dropped.
        .with_sloppy_theme("MoMu Hackathon", "open_themed", Some("climate"));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    assert_eq!(outcome.agent_failure, None);
    assert_eq!(outcome.judged, 5, "every Candidate in the batch is judged");
    assert!(harness.unjudged_names().is_empty(), "none stays queued");
    assert_eq!(outcome.themes_normalised, 3);
    assert_eq!(names(&outcome), without(ODOO));
    assert_eq!(
        theme_answer(&harness, LIEGE).map(|a| a.fit),
        Some(ThemeFit::OpenThemed),
        "the unknown Theme isn't stored"
    );
    let alert = &harness.channel.sent[0].text;
    assert!(!alert.contains("Theme:"), "{alert}");
    assert!(!alert.contains("Odoo"), "{alert}");
}

#[test]
fn a_clean_run_normalises_nothing() {
    let mut harness = Harness::new(themed_profile(&["Belgium"], &["health"]));
    harness.agent =
        FakeAgent::approving().with_theme_fit(LIEGE, ThemeFit::OnTheme("health".into()));

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), DRY_RUN);

    assert_eq!(outcome.themes_normalised, 0);
    assert_eq!(names(&outcome), all());
}

#[test]
fn an_answer_without_the_theme_question_is_rejected_and_its_candidates_wait() {
    let mut harness = Harness::new(themed_profile(&["Belgium"], &["climate"]));
    // A judgement in the old shape, with no Theme answer.
    harness.agent = FakeAgent::answering(
        r#"{"judgements": [{
            "id": "1", "is_hackathon": true, "eligible": true,
            "eligibility_restriction": null, "travel_funded": "none", "travel_detail": null,
            "application_deadline": null, "application_status": "open",
            "reason": "Teams build software."
        }]}"#,
    );

    let outcome = harness.run(&[hackathon_be_from_fixture()], date(2026, 11, 9), LIVE_RUN);

    let failure = outcome.agent_failure.expect("the answer is rejected");
    assert!(failure.contains("theme_fit"), "{failure}");
    assert!(outcome.fitting.is_empty());
    assert!(harness.store.judgements().unwrap().is_empty());
    assert_eq!(harness.unjudged_names().len(), 5);
}
