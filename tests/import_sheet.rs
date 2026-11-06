//! `import-sheet` (ticket 17): the personal tracker's Hackathons tab, exported
//! as CSV, imported into the store, then a whole Run over what it imported;
//! and a CSV from `export sheet` imported again.

mod support;

use std::path::Path;

use chrono::NaiveDate;
use hackathon_radar::agent::{ApplicationStatus, TravelFunded};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::export;
use hackathon_radar::fares::FareQuery;
use hackathon_radar::report::{self, Scope};
use hackathon_radar::sheet_import::{self, Report, SkipReason, Skipped};
use hackathon_radar::sheet_layout::RowError;
use hackathon_radar::store::{Store, key_of};
use hackathon_radar::travel::TripKind;
use support::*;

/// The day of the import.
fn today() -> NaiveDate {
    date(2026, 11, 6)
}

fn sheet() -> String {
    fixture("sheet/hackathons.csv")
}

/// The Profile the personal tracker stands for: Home in Brussels, flying from
/// BRU or CRL, the countries it allowed, and no rechecks so a Run keeps what
/// the Sheet said.
fn tracker_profile() -> hackathon_radar::profile::Profile {
    travel_profile(
        &[
            "Belgium",
            "France",
            "Netherlands",
            "Luxembourg",
            "Germany",
            "Switzerland",
            "United Kingdom",
        ],
        "[travel]\ntrain_limit_hours = 4\n\n[deadlines]\nrechecks_per_run = 0\n",
    )
}

/// Ghent Civic Hackathon, collected from hackathon.be and announced by an
/// earlier Run; the Sheet lists it too, with another spelling of its link.
fn ghent_civic() -> Candidate {
    Candidate {
        source: "hackathon.be".into(),
        source_key: "ghent-civic-2026".into(),
        name: "Ghent Civic Hackathon 2026".into(),
        start: date(2026, 11, 20),
        end: date(2026, 11, 22),
        city: Some("Ghent".into()),
        location: Some("De Krook, Ghent".into()),
        country: Some("Belgium".into()),
        link: Some("https://www.ghentcivic.example/2026/?utm_source=hackathon.be".into()),
    }
}

fn store_ghent_civic(store: &mut Store) {
    let ghent = ghent_civic();
    let day = date(2026, 11, 3);
    store
        .save_listings(&[(&ghent, key_of(&ghent))], day)
        .unwrap();
    store
        .save_hackathons(std::slice::from_ref(&ghent), day)
        .unwrap();
    store
        .mark_announced(std::slice::from_ref(&ghent), day)
        .unwrap();
}

fn sheet_key(key: &str) -> (String, String) {
    (sheet_import::SOURCE.to_owned(), key.to_owned())
}

/// Every row of every table, so two states of the database can be compared.
fn dump(path: &Path) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut out = String::new();
    for table in tables {
        out.push_str(&format!("## {table}\n"));
        let mut query = conn
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1, 2, 3"))
            .unwrap();
        let columns = query.column_count();
        let mut rows = query.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let cells: Vec<String> = (0..columns)
                .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                .collect();
            out.push_str(&cells.join(" | "));
            out.push('\n');
        }
    }
    out
}

fn expected_report() -> Report {
    Report {
        rows: 15,
        imported: 12,
        merged: 1,
        announced: 3,
        skipped: vec![
            Skipped {
                row: 15,
                name: None,
                why: SkipReason::Unreadable(RowError::MissingName),
            },
            Skipped {
                row: 16,
                name: Some("Mystery Hack".into()),
                why: SkipReason::Unreadable(RowError::Unparseable {
                    column: "Start",
                    value: "sometime in Dec".into(),
                }),
            },
        ],
        warnings: Vec::new(),
    }
}

#[test]
fn every_column_of_the_sheet_is_stored() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut store = Store::open(&dir.path().join("radar.sqlite")).unwrap();
    store_ghent_civic(&mut store);
    let profile = tracker_profile();

    let report = sheet_import::import(&mut store, &profile, &sheet(), today(), false).unwrap();

    assert_eq!(report, expected_report());

    // One Hackathon per row that Fits, plus Ghent Civic, which the Sheet's row merged into.
    let hackathons = store.hackathons().unwrap();
    let names: Vec<&str> = hackathons.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Lille Hack Days",
            "Rotterdam Port Datathon",
            "Brussels AI Hack",
            "Ghent Civic Hackathon 2026",
            "Paris Climate Build",
            "Euro Agents Hackathon",
            "London Fintech Hack",
            "Munich Agents Hack",
            "Zurich Robotics Hack",
            "Lisbon Web Hack",
            "Berlin Builders Weekend",
            "Amsterdam Open Data Day",
        ],
        "Ghent Pitch Night is out of scope, so it isn't stored as a Hackathon"
    );
    let announced: Vec<(&str, NaiveDate)> = hackathons
        .iter()
        .filter_map(|h| Some((h.name.as_str(), h.announced_on?)))
        .collect();
    assert_eq!(
        announced,
        [
            ("Brussels AI Hack", date(2026, 11, 2)),
            ("Ghent Civic Hackathon 2026", date(2026, 11, 3)),
            ("London Fintech Hack", date(2026, 11, 2)),
            ("Zurich Robotics Hack", date(2026, 11, 5)),
        ],
        "the notified rows are Announced; Ghent Civic keeps its own Announced day"
    );

    // The listing: Link as the key, country names from codes (UK too), known city names.
    let munich = hackathons
        .iter()
        .find(|h| h.name == "Munich Agents Hack")
        .unwrap();
    assert_eq!(
        (munich.source.as_str(), munich.source_key.as_str()),
        (sheet_import::SOURCE, "https://munichagents.de")
    );
    assert_eq!(munich.city.as_deref(), Some("Munich"));
    assert_eq!(munich.country.as_deref(), Some("Germany"));
    assert_eq!(
        (munich.start, munich.end),
        (date(2026, 12, 4), date(2026, 12, 6))
    );
    assert_eq!(
        (munich.first_seen, munich.last_seen),
        (date(2026, 11, 2), date(2026, 11, 5))
    );
    let london = hackathons
        .iter()
        .find(|h| h.name == "London Fintech Hack")
        .unwrap();
    assert_eq!(london.country.as_deref(), Some("United Kingdom"));
    let amsterdam = hackathons
        .iter()
        .find(|h| h.name == "Amsterdam Open Data Day")
        .unwrap();
    assert_eq!(
        amsterdam.source_key, "Amsterdam Open Data Day 2027-01-09",
        "without a Link, the name and start date are the key"
    );
    assert_eq!(amsterdam.end, amsterdam.start);
    assert!(
        hackathons
            .iter()
            .any(|h| h.source_key == "https://euro-agents.devpost.com/")
    );

    // The judgement: deadline, status, Travel-funded, eligibility and the free text.
    let paris = store
        .judgement(sheet_import::SOURCE, "https://parisclimate.build/2026")
        .unwrap()
        .unwrap();
    assert!(paris.is_hackathon && paris.eligible);
    assert_eq!(paris.deadline, Some(date(2026, 11, 15)));
    assert_eq!(paris.status, ApplicationStatus::OpenUntilFull);
    assert_eq!(paris.travel_funded, TravelFunded::Partial);
    assert_eq!(paris.travel_detail.as_deref(), Some("€100 train refund"));
    assert_eq!(
        paris.eligibility_restriction.as_deref(),
        Some("Students and young professionals")
    );
    assert_eq!(
        paris.reason,
        "Imported from the personal tracker's Sheet (found on Luma). Format: Hybrid. \
         Theme: Climate. Fits budget?: Yes – train."
    );
    let london_judgement = store
        .judgement(sheet_import::SOURCE, "https://londonfintech.example/hack")
        .unwrap()
        .unwrap();
    assert_eq!(london_judgement.travel_funded, TravelFunded::Full);
    assert_eq!(
        london_judgement.travel_detail.as_deref(),
        Some("flights and hotel for finalists; accommodation: Yes – hotel for finalists")
    );
    let euro = store
        .judgement(sheet_import::SOURCE, "https://euro-agents.devpost.com/")
        .unwrap()
        .unwrap();
    assert_eq!(
        euro.travel_funded,
        TravelFunded::Partial,
        "lodging covered counts as partly Travel-funded"
    );
    let status = |key: &str| {
        store
            .judgement(sheet_import::SOURCE, key)
            .unwrap()
            .unwrap()
            .status
    };
    assert_eq!(
        status("https://munichagents.de"),
        ApplicationStatus::NotOpenYet
    );
    assert_eq!(
        status("https://berlinbuilders.example/weekend"),
        ApplicationStatus::Waitlist
    );
    assert_eq!(
        status("https://lisbonhack.example"),
        ApplicationStatus::Unknown
    );

    // Closed: out of scope (never Fits), ended, or Closed and over.
    let pitch = store
        .judgement(sheet_import::SOURCE, "https://ghentpitch.example")
        .unwrap()
        .unwrap();
    assert_eq!(pitch.status, ApplicationStatus::Closed);
    assert!(!pitch.fits(), "an out-of-scope row can never be announced");
    assert!(pitch.reason.contains("pitch competition, nothing is built"));
    assert_eq!(
        status("https://lillehackdays.example"),
        ApplicationStatus::Closed
    );
    assert_eq!(
        status("https://portdatathon.example"),
        ApplicationStatus::Closed
    );

    // The Duplicate listing: the row is a listing of Ghent Civic, which takes its judgement and Trip.
    let listings = store.listings().unwrap();
    assert_eq!(listings.len(), 14);
    let civic_row = listings
        .iter()
        .find(|l| l.key() == sheet_key("https://ghentcivic.example/2026"))
        .unwrap();
    assert_eq!(civic_row.hackathon, key_of(&ghent_civic()));
    assert!(
        store
            .judgement(sheet_import::SOURCE, "https://ghentcivic.example/2026")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .judgement("hackathon.be", "ghent-civic-2026")
            .unwrap()
            .unwrap()
            .status,
        ApplicationStatus::Open
    );
    assert_eq!(
        store
            .trip("hackathon.be", "ghent-civic-2026")
            .unwrap()
            .map(|t| t.label),
        Some("local".to_owned())
    );

    // Trips from Getting there.
    let trip = |key: &str| store.trip(sheet_import::SOURCE, key).unwrap().unwrap();
    assert_eq!(trip("https://lu.ma/brussels-ai-hack").kind, TripKind::Local);
    let paris_trip = trip("https://parisclimate.build/2026");
    assert_eq!(
        (
            paris_trip.kind,
            paris_trip.train_minutes,
            paris_trip.label.as_str()
        ),
        (TripKind::Train, Some(90), "train ~1h30")
    );
    let munich_trip = trip("https://munichagents.de");
    assert_eq!(
        (
            munich_trip.kind,
            munich_trip.airport.as_deref(),
            munich_trip.label.as_str()
        ),
        (TripKind::Flight, Some("MUC"), "fly BRU→MUC")
    );

    // Fares: only with an estimate, verified only when checked on flight prices.
    let fare = |airport: &str, start: NaiveDate, end: NaiveDate| {
        store
            .fare(&FareQuery {
                from: vec!["BRU".into(), "CRL".into()],
                to: airport.into(),
                outbound: start.pred_opt().unwrap(),
                inbound: end.succ_opt().unwrap(),
                currency: "EUR".into(),
            })
            .unwrap()
    };
    let munich_fare = fare("MUC", date(2026, 12, 4), date(2026, 12, 6)).unwrap();
    assert_eq!(
        (
            munich_fare.price,
            munich_fare.verified,
            munich_fare.checked_on
        ),
        (Some(185), true, date(2026, 11, 5))
    );
    let berlin_fare = fare("BER", date(2026, 12, 12), date(2026, 12, 13)).unwrap();
    assert_eq!(
        (berlin_fare.price, berlin_fare.verified),
        (Some(240), false),
        "an Unverified fare"
    );
    let zurich_fare = fare("ZRH", date(2026, 12, 5), date(2026, 12, 6)).unwrap();
    assert_eq!(
        (zurich_fare.price, zurich_fare.verified),
        (Some(160), false)
    );
    assert_eq!(
        fare("LIS", date(2026, 12, 8), date(2026, 12, 9)),
        None,
        "an empty fare stores none"
    );

    // Newly Travel-funded news the Sheet already sent isn't sent again.
    assert!(store.newly_funded().unwrap().is_empty());
    let conn = rusqlite::Connection::open(dir.path().join("radar.sqlite")).unwrap();
    let sent: Vec<(String, String)> = conn
        .prepare("SELECT source_key, announced_on FROM newly_funded ORDER BY source_key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        sent,
        [
            (
                "https://londonfintech.example/hack".to_owned(),
                "2026-11-05".to_owned()
            ),
            (
                "https://zurichrobotics.example".to_owned(),
                "2026-11-05".to_owned()
            ),
        ]
    );
}

#[test]
fn importing_the_same_file_again_changes_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("radar.sqlite");
    let mut store = Store::open(&path).unwrap();
    store_ghent_civic(&mut store);
    let profile = tracker_profile();
    sheet_import::import(&mut store, &profile, &sheet(), today(), false).unwrap();
    let before = dump(&path);

    let again =
        sheet_import::import(&mut store, &profile, &sheet(), date(2026, 11, 7), false).unwrap();

    assert_eq!(dump(&path), before);
    assert_eq!((again.imported, again.merged, again.announced), (0, 0, 0));
    let already = again
        .skipped
        .iter()
        .filter(|s| s.why == SkipReason::AlreadyImported)
        .count();
    assert_eq!(already, 13);
    assert_eq!(again.skipped.len(), 15);
}

#[test]
fn a_dry_run_reports_the_same_counts_and_writes_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("radar.sqlite");
    let mut store = Store::open(&path).unwrap();
    store_ghent_civic(&mut store);
    let profile = tracker_profile();
    let before = dump(&path);

    let dry = sheet_import::import(&mut store, &profile, &sheet(), today(), true).unwrap();

    assert_eq!(dry, expected_report());
    assert_eq!(dump(&path), before);
    let real = sheet_import::import(&mut store, &profile, &sheet(), today(), false).unwrap();
    assert_eq!(real, dry);
    assert_ne!(dump(&path), before);
}

#[test]
fn rows_repeating_a_hackathon_in_the_file_are_merged_or_skipped() {
    let header = hackathon_radar::sheet_layout::headers().join(",");
    let row = |name: &str, start: &str, link: &str| {
        let mut cells = vec![""; 23];
        cells[0] = name;
        cells[1] = start;
        cells[3] = "Brussels";
        cells[4] = "BE";
        cells[17] = link;
        cells.join(",")
    };
    let csv = [
        header,
        row("Spring Hack", "2026-04-10", "https://springhack.example"),
        row("Spring Hack", "2027-04-09", "https://springhack.example"),
        row(
            "Spring Hack again",
            "2027-04-09",
            "https://springhack.example",
        ),
        row(
            "Brussels AI Hack",
            "2026-11-14",
            "https://lu.ma/brussels-ai-hack",
        ),
        row(
            "Brussels AI Hackathon",
            "2026-11-14",
            "https://luma.com/brussels-ai-hack?utm_source=x",
        ),
    ]
    .join("\n");
    let mut store = Store::open_in_memory().unwrap();

    let report =
        sheet_import::import(&mut store, &tracker_profile(), &csv, today(), false).unwrap();

    assert_eq!((report.imported, report.merged), (3, 1));
    assert_eq!(
        report.skipped,
        [Skipped {
            row: 4,
            name: Some("Spring Hack again".into()),
            why: SkipReason::SameAsRow(3),
        }]
    );
    let keys: Vec<String> = store
        .hackathons()
        .unwrap()
        .into_iter()
        .map(|h| h.source_key)
        .collect();
    assert_eq!(
        keys,
        [
            "https://springhack.example 2026-04-10",
            "https://lu.ma/brussels-ai-hack",
            "https://springhack.example 2027-04-09",
        ],
        "a yearly event's editions sharing a Link are different Hackathons"
    );
    assert_eq!(
        store
            .listings_of(sheet_import::SOURCE, "https://lu.ma/brussels-ai-hack")
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_file_that_isnt_the_hackathons_tab_is_refused() {
    let mut store = Store::open_in_memory().unwrap();
    let error = sheet_import::import(
        &mut store,
        &tracker_profile(),
        "Name,Date\nHack,2026-11-14\n",
        today(),
        false,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("no \"Hackathon\", \"Start\""),
        "{error}"
    );
    assert!(store.listings().unwrap().is_empty());
}

/// Everything stored about Hackathons, to tell whether an import changed anything.
fn state(store: &Store) -> String {
    format!(
        "{:#?}\n{:#?}\n{:#?}",
        store.hackathons().unwrap(),
        store.listings().unwrap(),
        store.judgements().unwrap()
    )
}

/// A store filled by a live Run, as `export sheet` would export it: a flight
/// with an Unverified fare from CRL, one over the Flight budget, a train, and
/// a name a spreadsheet would read as a formula.
fn run_then_export() -> (Harness, String) {
    let profile = travel_profile(
        &["Belgium", "France", "Germany", "Portugal"],
        "[travel]\nflight_budget = 200\n\n[deadlines]\nrechecks_per_run = 0\n",
    );
    let mut harness = Harness::new(profile);
    harness.agent = FakeAgent::approving()
        .with_estimate("Paris", Some(1.4), Some("CDG"))
        .with_estimate("Munich", Some(6.4), Some("MUC"))
        .with_estimate("Lisbon", None, Some("LIS"))
        .with_fare_estimate("MUC", Some(96), Some("CRL"))
        .with_fare_estimate("LIS", Some(340), Some("BRU"))
        .with_verdict("Paris Hack", open_until(date(2026, 11, 20)))
        .with_verdict(
            "Munich Hack",
            travel_funded(TravelFunded::Partial, "€150 towards travel"),
        );
    let run_day = date(2026, 11, 9);
    let sources: Vec<Box<dyn hackathon_radar::sources::Source>> = vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(vec![
            collected(
                "+32 Hack Brussels",
                date(2026, 11, 21),
                date(2026, 11, 22),
                "Brussels",
                "Belgium",
                "https://plus32.example",
            ),
            collected(
                "Paris Hack",
                date(2026, 11, 28),
                date(2026, 11, 29),
                "Paris",
                "France",
                "https://parishack.example",
            ),
            collected(
                "Munich Hack",
                date(2026, 12, 5),
                date(2026, 12, 6),
                "Munich",
                "Germany",
                "https://munichhack.example",
            ),
            collected(
                "Lisbon Hack",
                date(2026, 12, 12),
                date(2026, 12, 13),
                "Lisbon",
                "Portugal",
                "https://lisbonhack.example",
            ),
        ]),
    })];
    harness.run(&sources, run_day, LIVE_RUN);
    let listed = report::collect(&harness.store, &harness.profile, run_day, Scope::All).unwrap();
    let csv = export::sheet::sheet(&listed, &harness.profile, run_day)
        .unwrap()
        .text;
    assert!(csv.contains("'+32 Hack Brussels"), "{csv}");
    assert!(csv.contains("~€96 from CRL (unverified)"), "{csv}");
    (harness, csv)
}

#[test]
fn an_export_imported_into_the_store_it_came_from_changes_no_hackathon() {
    let (mut harness, csv) = run_then_export();
    let profile = harness.profile.clone();
    let day = date(2026, 11, 9);
    let hackathons_before = harness.store.hackathons().unwrap();
    let judgements_before = harness.store.judgements().unwrap();

    let first = sheet_import::import(&mut harness.store, &profile, &csv, day, false).unwrap();

    assert_eq!(
        (first.rows, first.imported, first.merged, first.announced),
        (4, 0, 4, 0)
    );
    assert!(first.skipped.is_empty(), "{:?}", first.skipped);
    assert!(first.warnings.is_empty(), "{:?}", first.warnings);
    assert_eq!(
        harness.store.hackathons().unwrap(),
        hackathons_before,
        "each row is a listing of the Hackathon it was exported from"
    );
    assert_eq!(harness.store.judgements().unwrap(), judgements_before);

    let after_first = state(&harness.store);
    let second = sheet_import::import(&mut harness.store, &profile, &csv, day, false).unwrap();
    assert_eq!(state(&harness.store), after_first);
    assert_eq!((second.imported, second.merged), (0, 0));
    assert!(
        second
            .skipped
            .iter()
            .all(|s| s.why == SkipReason::AlreadyImported)
    );
}

#[test]
fn an_export_imported_into_a_new_store_keeps_what_it_says() {
    let (harness, csv) = run_then_export();
    let mut store = Store::open_in_memory().unwrap();
    let day = date(2026, 11, 9);

    let report = sheet_import::import(&mut store, &harness.profile, &csv, day, false).unwrap();

    assert_eq!((report.imported, report.merged), (4, 0));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let names: Vec<String> = store
        .hackathons()
        .unwrap()
        .into_iter()
        .map(|h| h.name)
        .collect();
    assert_eq!(
        names,
        ["+32 Hack Brussels", "Paris Hack", "Munich Hack"],
        "the formula guard is dropped; Lisbon doesn't Fit (over the Flight budget)"
    );
    let lisbon = store
        .judgement(sheet_import::SOURCE, "https://lisbonhack.example")
        .unwrap()
        .unwrap();
    assert!(!lisbon.fits());
    assert!(lisbon.reason.contains("over the Flight budget"));
    let munich = store
        .judgement(sheet_import::SOURCE, "https://munichhack.example")
        .unwrap()
        .unwrap();
    assert_eq!(munich.travel_funded, TravelFunded::Partial);
    assert_eq!(munich.travel_detail.as_deref(), Some("€150 towards travel"));
    let trip = store
        .trip(sheet_import::SOURCE, "https://munichhack.example")
        .unwrap()
        .unwrap();
    assert_eq!(
        (trip.kind, trip.airport.as_deref(), trip.label.as_str()),
        (TripKind::Flight, Some("MUC"), "fly BRU→MUC")
    );
    let fare = store
        .fare(&FareQuery {
            from: vec!["BRU".into(), "CRL".into()],
            to: "MUC".into(),
            outbound: date(2026, 12, 4),
            inbound: date(2026, 12, 7),
            currency: "EUR".into(),
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        (fare.price, fare.from_airport.as_deref(), fare.verified),
        (Some(96), Some("CRL"), false)
    );
    let paris = store
        .judgement(sheet_import::SOURCE, "https://parishack.example")
        .unwrap()
        .unwrap();
    assert_eq!(
        (paris.deadline, paris.status),
        (Some(date(2026, 11, 20)), ApplicationStatus::Open)
    );

    let before = state(&store);
    let again = sheet_import::import(&mut store, &harness.profile, &csv, day, false).unwrap();
    assert_eq!(state(&store), before);
    assert_eq!((again.imported, again.merged), (0, 0));
}

#[test]
fn the_exports_fixture_reads_without_a_skipped_row() {
    let mut store = Store::open_in_memory().unwrap();
    let csv = fixture("reports/sheet-all.csv");

    let report = sheet_import::import(
        &mut store,
        &tracker_profile(),
        &csv,
        date(2026, 11, 9),
        false,
    )
    .unwrap();

    assert_eq!((report.rows, report.imported), (9, 9));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let judgement = |key: &str| store.judgement(sheet_import::SOURCE, key).unwrap().unwrap();
    let tokyo = judgement("https://example.org/tokyo");
    assert_eq!(tokyo.travel_funded, TravelFunded::Full);
    assert_eq!(
        tokyo.travel_detail.as_deref(),
        Some("Flights and two hotel nights, for selected teams")
    );
    assert!(!judgement("https://example.org/amsterdam").fits());
    assert_eq!(
        judgement("https://example.org/munich?ref=radar,2026").status,
        ApplicationStatus::OpenUntilFull
    );
    assert_eq!(
        store
            .trip(sheet_import::SOURCE, "https://example.org/ghent")
            .unwrap()
            .map(|t| (t.kind, t.train_minutes, t.label)),
        Some((TripKind::Local, Some(30), "local, train ~30min".to_owned()))
    );
}

/// A listing a Source collects for one of the Sheet's Hackathons.
fn collected(
    name: &str,
    start: NaiveDate,
    end: NaiveDate,
    city: &str,
    country: &str,
    link: &str,
) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start,
        end,
        city: Some(city.into()),
        location: None,
        country: Some(country.into()),
        link: Some(link.into()),
    }
}

#[test]
fn a_run_after_the_import_announces_only_what_the_sheet_never_notified() {
    let mut harness = Harness::new(tracker_profile());
    harness.agent = FakeAgent::approving()
        .with_estimate("Paris", Some(1.5), Some("CDG"))
        .with_estimate("London", Some(2.0), Some("LHR"));
    sheet_import::import(
        &mut harness.store,
        &harness.profile,
        &sheet(),
        today(),
        false,
    )
    .unwrap();
    let source = FakeSource {
        name: "fake".into(),
        result: Ok(vec![
            collected(
                "Brussels AI Hack 2026",
                date(2026, 11, 14),
                date(2026, 11, 15),
                "Brussels",
                "Belgium",
                "https://luma.com/brussels-ai-hack",
            ),
            collected(
                "Paris Climate Build 2026",
                date(2026, 11, 21),
                date(2026, 11, 22),
                "Paris",
                "France",
                "https://parisclimate.build/2026?utm_source=luma",
            ),
            collected(
                "London Fintech Hackathon",
                date(2026, 11, 28),
                date(2026, 11, 29),
                "London",
                "United Kingdom",
                "https://londonfintech.example/hack",
            ),
        ]),
    };
    let sources: Vec<Box<dyn hackathon_radar::sources::Source>> = vec![Box::new(source)];

    let outcome = harness.run(&sources, date(2026, 11, 9), LIVE_RUN);

    let fitting: Vec<&str> = outcome.fitting.iter().map(|h| h.source.as_str()).collect();
    assert_eq!(
        fitting,
        [sheet_import::SOURCE; 3],
        "each listing found belongs to the Sheet's Hackathon"
    );
    assert_eq!(
        harness.agent.requests(),
        0,
        "the Sheet's judgements stand, so the Agent judges nothing"
    );
    assert_eq!(outcome.announced, 1);
    let alert = &harness.channel.sent[0].text;
    assert!(!alert.contains("welcome"), "{alert}");
    assert!(alert.contains("Paris Climate Build"), "{alert}");
    assert!(!alert.contains("Brussels AI Hack"), "{alert}");
    assert!(!alert.contains("London Fintech"), "{alert}");
    assert_eq!(
        harness.announced_names(),
        [
            "Brussels AI Hack",
            "Paris Climate Build",
            "London Fintech Hack",
            "Zurich Robotics Hack",
        ]
    );
    let paris = harness
        .store
        .hackathons()
        .unwrap()
        .into_iter()
        .find(|h| h.name == "Paris Climate Build")
        .unwrap();
    assert_eq!(
        (paris.source.as_str(), paris.announced_on),
        (sheet_import::SOURCE, Some(date(2026, 11, 9)))
    );
}
