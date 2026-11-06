//! Reports and exports (ticket 15), from a store filled by a whole Run with
//! fakes and a fixed clock: the Markdown and HTML report, the deadline
//! calendar and the Sheet CSV against golden copies, the calendar read back
//! through the repo's own iCalendar reader, the CSV parsed back, and an empty
//! store. Nothing touches the network.
//!
//! To refresh the golden copies after a deliberate change, run
//! `UPDATE_GOLDEN=1 cargo test --test reports` and review the diff.

mod support;

use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};

use hackathon_radar::agent::{ApplicationStatus, Judgement, TravelFunded};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::clock::Clock;
use hackathon_radar::export::{ics as ics_export, sheet};
use hackathon_radar::ics;
use hackathon_radar::profile::Profile;
use hackathon_radar::report::{self, Listed, Report, Scope};
use hackathon_radar::run::{self, RunDeps};
use hackathon_radar::sheet_layout::{self, Column};
use hackathon_radar::sources::Source;
use hackathon_radar::store::Store;
use support::*;

/// Monday 9 November 2026, the day of the Run and of every report here.
fn today() -> NaiveDate {
    date(2026, 11, 9)
}

/// 07:00 UTC that Monday: the Run's start and end, and the calendar's stamp.
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-09T07:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

/// A clock pinned to [`today`] and [`now`], so the Run history is fixed too.
struct PinnedClock;

impl Clock for PinnedClock {
    fn today(&self) -> NaiveDate {
        today()
    }

    fn now(&self) -> DateTime<Utc> {
        now()
    }
}

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reports")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    assert!(
        expected == actual,
        "{name} differs from its golden copy; rerun with UPDATE_GOLDEN=1 if the change is \
         deliberate.\n--- expected\n{expected}\n--- actual\n{actual}"
    );
}

fn listing(
    source: &str,
    name: &str,
    city: &str,
    country: &str,
    start: NaiveDate,
    end: NaiveDate,
    link: Option<&str>,
) -> Candidate {
    Candidate {
        source: source.into(),
        source_key: name.to_lowercase().replace(' ', "-"),
        name: name.into(),
        start,
        end,
        city: Some(city.into()),
        location: None,
        country: Some(country.into()),
        link: link.map(str::to_owned),
    }
}

const PARIS: &str = "Paris Climate & Energy Hackathon 2026: Building Tools for the Grid";
const TOKYO: &str = "AI Hack Tokyo: \"Build, Ship\"";

/// Every kind of Hackathon a report shows: by train, flights under and over
/// the Flight budget, waiting for a Fare, beyond the Fare horizon,
/// Travel-funded far away, a Devpost submission period, closed, ineligible,
/// and a listing that isn't a Hackathon at all.
fn listings() -> Vec<Candidate> {
    vec![
        listing(
            "fake",
            PARIS,
            "Paris",
            "France",
            date(2026, 11, 28),
            date(2026, 11, 29),
            Some("https://example.org/paris"),
        ),
        listing(
            "fake",
            "Munich Hack",
            "Munich",
            "Germany",
            date(2026, 12, 5),
            date(2026, 12, 6),
            Some("https://example.org/munich?ref=radar,2026"),
        ),
        listing(
            "fake",
            "Lisbon Hack",
            "Lisbon",
            "Portugal",
            date(2026, 12, 12),
            date(2026, 12, 13),
            Some("https://example.org/lisbon"),
        ),
        listing(
            "fake",
            "Hamburg Hack",
            "Hamburg",
            "Germany",
            date(2026, 12, 1),
            date(2026, 12, 2),
            None,
        ),
        listing(
            "fake",
            "Lyon Hack",
            "Lyon",
            "France",
            date(2027, 3, 12),
            date(2027, 3, 14),
            Some("https://example.org/lyon"),
        ),
        listing(
            "fake",
            TOKYO,
            "Tokyo",
            "Japan",
            date(2026, 12, 19),
            date(2026, 12, 20),
            Some("https://example.org/tokyo"),
        ),
        listing(
            "devpost",
            "Berlin Devpost Hack",
            "Berlin",
            "Germany",
            date(2026, 11, 1),
            date(2026, 12, 6),
            Some("https://berlin-hack.devpost.com/"),
        ),
        listing(
            "fake",
            "Ghent Hack",
            "Ghent",
            "Belgium",
            date(2026, 11, 20),
            date(2026, 11, 21),
            Some("https://example.org/ghent"),
        ),
        listing(
            "fake",
            "Amsterdam Students Hack",
            "Amsterdam",
            "Netherlands",
            date(2026, 11, 14),
            date(2026, 11, 15),
            Some("https://example.org/amsterdam"),
        ),
        listing(
            "fake",
            "Brussels Pitch Night",
            "Brussels",
            "Belgium",
            date(2026, 11, 18),
            date(2026, 11, 18),
            None,
        ),
    ]
}

fn judged(deadline: Option<NaiveDate>, status: ApplicationStatus) -> Judgement {
    Judgement {
        deadline,
        status,
        ..hackathon()
    }
}

fn profile(extra: &str) -> Profile {
    travel_profile(
        &["Belgium", "France", "Germany", "Portugal"],
        &format!("[travel]\nflight_budget = 200\ntravel_funded_anywhere = true\n{extra}"),
    )
}

/// A store filled by one live Run on [`today`], with Agent-estimated
/// (Unverified) fares and no SerpApi key.
fn filled_store(profile: Profile) -> Harness {
    let mut harness = Harness::new(profile);
    harness.agent = FakeAgent::approving()
        .with_estimate("Paris", Some(1.4), Some("CDG"))
        .with_estimate("Munich", Some(6.4), Some("MUC"))
        .with_estimate("Lisbon", None, Some("LIS"))
        .with_estimate("Hamburg", Some(5.5), Some("HAM"))
        .with_estimate("Lyon", Some(4.5), Some("LYS"))
        .with_estimate("Tokyo", None, Some("NRT"))
        .with_estimate("Berlin", Some(6.0), Some("BER"))
        .with_estimate("Ghent", Some(0.5), None)
        .with_fare_estimate("MUC", Some(96), Some("CRL"))
        .with_fare_estimate("LIS", Some(340), Some("BRU"))
        .with_verdict(PARIS, open_until(date(2026, 11, 12)))
        .with_verdict(
            "Munich Hack",
            judged(Some(date(2026, 11, 28)), ApplicationStatus::OpenUntilFull),
        )
        .with_verdict("Lisbon Hack", open_until(date(2026, 11, 20)))
        .with_verdict(
            "Hamburg Hack",
            judged(Some(date(2026, 11, 30)), ApplicationStatus::NotOpenYet),
        )
        .with_verdict(
            TOKYO,
            Judgement {
                eligibility_restriction: Some("Students and recent graduates".into()),
                ..travel_funded(
                    TravelFunded::Full,
                    "Flights and two hotel nights, for selected teams",
                )
            },
        )
        .with_verdict("Berlin Devpost Hack", open_until(date(2026, 12, 6)))
        .with_verdict("Ghent Hack", open_until(date(2026, 11, 1)))
        .with_verdict(
            "Amsterdam Students Hack",
            ineligible("Students of Dutch universities only"),
        )
        .with_verdict("Brussels Pitch Night", not_a_hackathon());
    // Tokyo's deadline: the Travel-funded verdict above has none yet.
    let tokyo = harness
        .agent
        .verdicts
        .iter_mut()
        .find(|(name, _)| name == TOKYO)
        .unwrap();
    tokyo.1.deadline = Some(date(2026, 11, 15));
    tokyo.1.status = ApplicationStatus::Open;

    let sources: Vec<Box<dyn Source>> = vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(listings()),
    })];
    run::run(
        RunDeps {
            profile: &harness.profile,
            sources: &sources,
            clock: &PinnedClock,
            store: &mut harness.store,
            channel: &mut harness.channel,
            agent: &harness.agent,
            fares: None,
        },
        LIVE_RUN,
    )
    .expect("the Run failed");
    harness
}

fn collect(harness: &Harness, scope: Scope) -> Vec<Listed> {
    report::collect(&harness.store, &harness.profile, today(), scope).unwrap()
}

fn names(listed: &[Listed]) -> Vec<&str> {
    listed.iter().map(|l| l.hackathon.name.as_str()).collect()
}

#[test]
fn the_report_lists_what_fits_sorted_by_date_with_every_alert_field() {
    let harness = filled_store(profile(""));
    let report = Report::build(&harness.store, &harness.profile, today(), Scope::Fitting).unwrap();

    assert_eq!(
        names(&report.listed),
        [
            "Berlin Devpost Hack",
            PARIS,
            "Hamburg Hack",
            "Munich Hack",
            TOKYO,
            "Lyon Hack"
        ],
        "soonest first; over budget, ineligible, closed and not-a-Hackathon left out"
    );
    let soon: Vec<&str> = report
        .closing_soon()
        .iter()
        .map(|l| l.hackathon.name.as_str())
        .collect();
    assert_eq!(soon, [PARIS, TOKYO]);

    let markdown = report.markdown();
    for field in [
        "Munich, Germany",
        "5–6 Dec 2026",
        "Submission period 1 Nov – 6 Dec 2026",
        "Open until full · deadline 28 Nov 2026 (19 days left)",
        "fly BRU→MUC · ~€96 from CRL (unverified)",
        "fly BRU→LYS · fare checked closer to the date",
        "fly BRU→NRT · travel paid",
        "fly BRU→HAM · fare not looked up yet",
        "Travel-funded (full): Flights and two hotel nights, for selected teams",
        "<https://example.org/munich?ref=radar,2026>",
        "Last Run: 9 Nov 2026 at 07:00 UTC, succeeded.",
    ] {
        assert!(
            markdown.contains(field),
            "{field:?} is missing:\n{markdown}"
        );
    }
    golden("report.md", &markdown);
    golden("report.html", &report.html());
}

#[test]
fn all_includes_what_doesnt_fit_or_is_closed_and_says_why() {
    let harness = filled_store(profile(""));
    let report = Report::build(&harness.store, &harness.profile, today(), Scope::All).unwrap();

    assert_eq!(
        names(&report.listed),
        [
            "Berlin Devpost Hack",
            "Amsterdam Students Hack",
            "Ghent Hack",
            PARIS,
            "Hamburg Hack",
            "Munich Hack",
            "Lisbon Hack",
            TOKYO,
            "Lyon Hack"
        ],
        "a listing that isn't a Hackathon stays out even with --all"
    );
    let why = |name: &str| {
        report
            .listed
            .iter()
            .find(|l| l.hackathon.name == name)
            .unwrap()
            .not_fitting
            .clone()
    };
    assert_eq!(
        why("Lisbon Hack").as_deref(),
        Some("over the Flight budget (€200)")
    );
    assert_eq!(
        why("Amsterdam Students Hack").as_deref(),
        Some("not eligible: Students of Dutch universities only")
    );
    assert_eq!(why("Ghent Hack"), None, "closed, but it still Fits");
    golden("report-all.md", &report.markdown());
}

#[test]
fn the_calendar_has_one_all_day_event_per_open_deadline_and_reads_back() {
    let harness = filled_store(profile(""));
    let listed = collect(&harness, Scope::Fitting);
    let calendar = ics_export::calendar(&listed, &harness.profile, now());

    golden("deadlines.ics", &calendar.text);
    assert_eq!(calendar.events, 5);
    assert_eq!(calendar.summary(), "5 deadlines in the calendar.");

    let events = ics::parse_events(&calendar.text);
    let read: Vec<(String, NaiveDate, NaiveDate)> = events
        .iter()
        .map(|e| {
            (
                e.text("SUMMARY").unwrap(),
                e.start_date().unwrap(),
                e.end_date().unwrap(),
            )
        })
        .collect();
    let day = |d| date(2026, 11, d);
    assert_eq!(
        read,
        [
            (
                "Apply: Berlin Devpost Hack".to_owned(),
                date(2026, 12, 6),
                date(2026, 12, 6)
            ),
            (format!("Apply: {PARIS}"), day(12), day(12)),
            ("Apply: Hamburg Hack".to_owned(), day(30), day(30)),
            ("Apply: Munich Hack".to_owned(), day(28), day(28)),
            (format!("Apply: {TOKYO}"), day(15), day(15)),
        ],
        "all-day events on each deadline, in the report's order"
    );
    for event in &events {
        assert_eq!(
            event.property("DTSTART").unwrap().param("VALUE"),
            Some("DATE")
        );
    }
    let munich = &events[3];
    assert_eq!(
        munich.text("DESCRIPTION").unwrap(),
        "Munich, Germany\n5–6 Dec 2026\nhttps://example.org/munich?ref=radar,2026"
    );
    assert_eq!(
        munich.text("URL").unwrap(),
        "https://example.org/munich?ref=radar,2026"
    );
    assert_eq!(
        events[0].text("DESCRIPTION").unwrap(),
        "Berlin, Germany\nSubmission period 1 Nov – 6 Dec 2026\nhttps://berlin-hack.devpost.com/"
    );
    let mut uids: Vec<String> = events.iter().map(|e| e.text("UID").unwrap()).collect();
    uids.sort();
    uids.dedup();
    assert_eq!(uids.len(), 5, "one UID per Hackathon and deadline");
    assert_eq!(
        calendar.text.matches("TRIGGER:-P3D\r\n").count(),
        5,
        "an alarm 3 days before each deadline"
    );
}

#[test]
fn the_calendar_is_valid_icalendar() {
    let harness = filled_store(profile(""));
    let listed = collect(&harness, Scope::All);
    let text = ics_export::calendar(&listed, &harness.profile, now()).text;

    assert!(text.starts_with("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:"));
    assert!(text.ends_with("END:VCALENDAR\r\n"));
    assert!(
        !text.replace("\r\n", "").contains('\n'),
        "only CRLF line endings"
    );
    assert!(
        !text.replace("\r\n", "").contains('\r'),
        "only CRLF line endings"
    );
    for line in text.trim_end_matches("\r\n").split("\r\n") {
        assert!(line.len() <= 75, "{line:?} is {} octets", line.len());
    }
    assert!(
        text.contains("\r\n "),
        "the long Paris summary is folded onto a continuation line"
    );
    // Unfolded, every event has the properties RFC 5545 requires, and its
    // TEXT values escape commas and semicolons.
    let unfolded = text.replace("\r\n ", "");
    assert_eq!(
        unfolded.matches("BEGIN:VEVENT").count(),
        unfolded.matches("END:VEVENT").count()
    );
    for event in unfolded.split("BEGIN:VEVENT").skip(1) {
        for required in [
            "\r\nUID:",
            "\r\nDTSTAMP:20261109T070000Z\r\n",
            "\r\nDTSTART;VALUE=DATE:",
        ] {
            assert!(event.contains(required), "{required:?} missing in {event}");
        }
        assert_eq!(event.matches("BEGIN:VALARM").count(), 1);
    }
    assert!(unfolded.contains("SUMMARY:Apply: AI Hack Tokyo: \"Build\\, Ship\"\r\n"));
    assert!(
        unfolded.contains("DESCRIPTION:Munich\\, Germany\\n5–6 Dec 2026\\nhttps://example.org/munich?ref=radar\\,2026\r\n")
    );
    // With --all, Ghent's passed deadline and Lisbon's (over budget) are in too.
    assert_eq!(unfolded.matches("BEGIN:VEVENT").count(), 7);
}

#[test]
fn uids_stay_the_same_across_exports_and_the_alarm_follows_reminder_days() {
    let harness = filled_store(profile("\n[deadlines]\nreminder_days = 5\n"));
    let listed = collect(&harness, Scope::Fitting);
    let earlier = ics_export::calendar(&listed, &harness.profile, now());
    let later = ics_export::calendar(&listed, &harness.profile, now() + chrono::Duration::days(3));
    let uids = |text: &str| -> Vec<String> {
        ics::parse_events(text)
            .iter()
            .map(|e| e.text("UID").unwrap())
            .collect()
    };
    assert_eq!(uids(&earlier.text), uids(&later.text));
    assert_ne!(earlier.text, later.text, "only the stamp differs");
    assert_eq!(earlier.text.matches("TRIGGER:-P5D\r\n").count(), 5);

    // A moved deadline gets its own event.
    let mut moved = listed.clone();
    moved[1].judgement.deadline = Some(date(2026, 11, 13));
    let moved_uids = uids(&ics_export::calendar(&moved, &harness.profile, now()).text);
    assert_ne!(moved_uids[1], uids(&earlier.text)[1]);
    assert_eq!(moved_uids[0], uids(&earlier.text)[0]);
}

#[test]
fn the_sheet_has_the_trackers_columns_and_parses_back_into_the_same_rows() {
    let harness = filled_store(profile(""));
    let listed = collect(&harness, Scope::Fitting);
    let exported = sheet::sheet(&listed, &harness.profile, today()).unwrap();

    golden("sheet.csv", &exported.text);
    assert_eq!(exported.rows, 6);
    assert!(exported.text.ends_with("\r\n"));

    let mut reader = csv::Reader::from_reader(exported.text.as_bytes());
    let header: Vec<String> = reader
        .headers()
        .unwrap()
        .iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        header,
        [
            "Hackathon",
            "Start",
            "End",
            "City",
            "Country",
            "Format",
            "Apply by",
            "Days left to apply",
            "Application status",
            "Travel covered",
            "Accommodation",
            "Getting there",
            "Est. return fare",
            "Fits budget?",
            "Prize",
            "Theme",
            "Eligibility",
            "Link",
            "Flights (Google)",
            "Found on",
            "First seen",
            "Last checked",
            "Notes",
        ]
    );
    assert_eq!(header, sheet_layout::headers());
    let parsed: Vec<Vec<String>> = reader
        .records()
        .map(|r| r.unwrap().iter().map(str::to_owned).collect())
        .collect();
    let expected: Vec<Vec<String>> = listed
        .iter()
        .map(|l| sheet::row(l, &harness.profile, today()))
        .collect();
    assert_eq!(parsed, expected);

    let row = |name: &str| {
        parsed
            .iter()
            .find(|r| r[Column::Hackathon.index()] == name)
            .unwrap()
    };
    let munich = row("Munich Hack");
    assert_eq!(munich[Column::Start.index()], "2026-12-05");
    assert_eq!(munich[Column::ApplyBy.index()], "2026-11-28");
    assert_eq!(munich[Column::DaysLeftToApply.index()], "19");
    assert_eq!(munich[Column::GettingThere.index()], "fly BRU→MUC");
    assert_eq!(
        munich[Column::EstReturnFare.index()],
        "~€96 from CRL (unverified)"
    );
    assert_eq!(munich[Column::FitsBudget.index()], "Yes");
    assert_eq!(
        munich[Column::FlightsGoogle.index()],
        "https://www.google.com/travel/flights?q=Flights%20to%20MUC%20from%20BRU%20on%202026-12-04%20through%202026-12-07"
    );
    let tokyo = row(TOKYO);
    assert_eq!(
        tokyo[Column::TravelCovered.index()],
        "Full: Flights and two hotel nights, for selected teams"
    );
    assert_eq!(tokyo[Column::EstReturnFare.index()], "travel paid");
    assert_eq!(
        tokyo[Column::Eligibility.index()],
        "Students and recent graduates"
    );
    for empty in [
        Column::Format,
        Column::Accommodation,
        Column::Prize,
        Column::Theme,
    ] {
        assert!(parsed.iter().all(|r| r[empty.index()].is_empty()));
    }
}

#[test]
fn the_sheet_with_all_notes_why_a_hackathon_doesnt_fit() {
    let harness = filled_store(profile(""));
    let listed = collect(&harness, Scope::All);
    let exported = sheet::sheet(&listed, &harness.profile, today()).unwrap();

    golden("sheet-all.csv", &exported.text);
    let mut reader = csv::Reader::from_reader(exported.text.as_bytes());
    let lisbon = reader
        .records()
        .map(Result::unwrap)
        .find(|r| &r[Column::Hackathon.index()] == "Lisbon Hack")
        .unwrap();
    assert_eq!(&lisbon[Column::FitsBudget.index()], "No");
    assert_eq!(
        &lisbon[Column::Notes.index()],
        "Doesn't Fit: over the Flight budget (€200)"
    );
}

#[test]
fn an_empty_store_says_nothing_yet() {
    let store = Store::open_in_memory().unwrap();
    let profile = profile("");

    let report = Report::build(&store, &profile, today(), Scope::Fitting).unwrap();
    let markdown = report.markdown();
    assert!(markdown.contains("Nothing yet: no Hackathon fits you as of 9 Nov 2026."));
    assert!(markdown.contains("No Run yet."));
    assert!(!markdown.contains("Closing soon"));
    assert!(report.html().contains("Nothing yet"));
    assert!(report.summary_line().starts_with("Nothing yet"));
    golden("report-empty.md", &markdown);

    let calendar = ics_export::calendar(&[], &profile, now());
    assert_eq!(calendar.events, 0);
    assert!(calendar.summary().starts_with("Nothing yet"));
    assert_eq!(ics::parse_events(&calendar.text).len(), 0);
    assert!(calendar.text.starts_with("BEGIN:VCALENDAR\r\n"));
    assert!(calendar.text.ends_with("END:VCALENDAR\r\n"));

    let exported = sheet::sheet(&[], &profile, today()).unwrap();
    assert_eq!(exported.rows, 0);
    assert!(exported.summary().starts_with("Nothing yet"));
    assert_eq!(
        exported.text,
        format!("{}\r\n", sheet_layout::headers().join(","))
    );
}

#[test]
fn a_failed_last_run_shows_in_the_footer() {
    let mut harness = filled_store(profile(""));
    harness
        .store
        .record_run(&hackathon_radar::store::RunRecord {
            day: today(),
            started_at: now(),
            finished_at: now() + chrono::Duration::minutes(4),
            failure: Some("the Agent timed out".into()),
            counts: Default::default(),
            attention_sent: false,
        })
        .unwrap();
    let report = Report::build(&harness.store, &harness.profile, today(), Scope::Fitting).unwrap();
    assert!(
        report
            .markdown()
            .contains("Last Run: 9 Nov 2026 at 07:04 UTC, failed: the Agent timed out")
    );
}
