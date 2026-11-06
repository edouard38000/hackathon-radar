//! `hackathon-radar import-sheet`: loads the personal tracker (a CSV export
//! of its Sheet's Hackathons tab, laid out as [`crate::sheet_layout`] says)
//! into the store once, at the switch-over, so no history is lost: deadlines,
//! status, travel, Fares, notes, and which Hackathons were already Announced.
//! After that the store is the source of truth (ADR 0003). A CSV written by
//! `export sheet` reads too, so an export can be imported again.
//!
//! Each row becomes a listing from the `sheet-import` Source, keyed by its
//! Link (or its name and start date without one), with a judgement made by
//! `sheet-import` rather than the Agent:
//! - it is a Hackathon (the person tracked it), eligible unless the Sheet
//!   ruled it out, with the Sheet's deadline, status and Travel-funded status;
//! - rows marked `OUT OF SCOPE` (`Doesn't Fit` in an export), or `Ended`
//!   before their dates, don't Fit, so they are never announced; those and
//!   rows that are Closed and over are stored as closed;
//! - a `🔔 notified` mark makes it Announced on that day (and so does
//!   `🔔 travel notified`, since that Alert told the person about it too),
//!   and `🔔 travel notified` records its newly Travel-funded news as sent;
//! - Getting there becomes its Trip, and the estimated return fare (for a
//!   flight) its Fare: an Unverified fare unless Notes say it was checked on
//!   flight prices (an export's fare says which it is). A bare price is in
//!   euros, as the tracker's header says; a price written in the Profile's
//!   currency, as the export writes it, is in that currency.
//!
//! Each row is matched against the listings already stored, and the rows
//! before it, by the Duplicate listing rules ([`compare`], ADR 0008). A row
//! that matches becomes another listing of that Hackathon instead of a new
//! one; whatever that Hackathon already has (judgement, Trip, Fare,
//! Announced day) is kept. Pairs the rules find unclear are only merged when
//! the Agent already said they are the same: the import never asks the Agent,
//! and a later Run settles them as usual. A row whose listing is already
//! stored is left alone, so importing the same file again changes nothing.

use std::collections::{HashMap, HashSet};

use anyhow::Context;
use chrono::{Days, NaiveDate};

use crate::agent::{ApplicationStatus, Judgement, TravelFunded};
use crate::candidate::Candidate;
use crate::fares::FareQuery;
use crate::matching::compare::{self, Verdict};
use crate::profile::Profile;
use crate::sheet_layout::{
    self, Column, FareCell, GettingThere, Layout, Row, RowError, TravelCovered,
};
use crate::sources::places;
use crate::store::{ImportedFare, ImportedListing, ListingKey, Store, key_of};
use crate::travel::{Trip, TripKind};

/// The Source imported rows are listed under, and the name their judgements are made by.
pub const SOURCE: &str = "sheet-import";

/// The currency of the personal tracker's Est. return fare (€) column.
const SHEET_CURRENCY: &str = "EUR";

/// What an import did, or would do on a dry run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Data rows read, blank ones aside.
    pub rows: usize,
    /// Rows stored as new Hackathons.
    pub imported: usize,
    /// Rows stored as Duplicate listings of a Hackathon already stored, or of an earlier row.
    pub merged: usize,
    /// Rows imported or merged that the Sheet marks as notified.
    pub announced: usize,
    pub skipped: Vec<Skipped>,
    /// Things read but not stored, by row.
    pub warnings: Vec<String>,
}

/// A row left out, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The row's number in the Sheet; the header is row 1.
    pub row: usize,
    pub name: Option<String>,
    pub why: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// A cell couldn't be read.
    Unreadable(RowError),
    /// Its listing is already stored, from an earlier import.
    AlreadyImported,
    /// It has the same Link and start date as an earlier row.
    SameAsRow(usize),
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkipReason::Unreadable(error) => error.fmt(f),
            SkipReason::AlreadyImported => f.write_str("already imported"),
            SkipReason::SameAsRow(row) => write!(f, "same Link and Start as row {row}"),
        }
    }
}

/// Imports the CSV export `csv` into `store`, or with `dry_run` only works
/// out what it would do. `today` decides which Closed rows are over.
pub fn import(
    store: &mut Store,
    profile: &Profile,
    csv: &str,
    today: NaiveDate,
    dry_run: bool,
) -> anyhow::Result<Report> {
    let records = sheet_layout::read_csv(csv).context("reading the CSV")?;
    let (header, records) = records.split_first().context("the CSV file is empty")?;
    let layout = Layout::from_header(header).map_err(anyhow::Error::msg)?;

    let mut report = Report {
        rows: records.len(),
        ..Report::default()
    };
    let rows: Vec<(usize, Result<Row, RowError>)> = records
        .iter()
        .enumerate()
        .map(|(n, cells)| (n + 2, Row::parse(cells, &layout)))
        .collect();
    // A Link shared by several rows (a yearly event's website) keys each by its start date too.
    let mut links: HashMap<String, usize> = HashMap::new();
    for (_, row) in &rows {
        if let Ok(Row {
            link: Some(link), ..
        }) = row
        {
            *links.entry(link.clone()).or_default() += 1;
        }
    }

    // What matching compares against: the stored listings, oldest first, then each row added.
    let mut stored = store.listings()?;
    stored.sort_by_key(|l| (l.first_seen, l.key()));
    let already: HashSet<ListingKey> = stored.iter().map(|l| l.key()).collect();
    let mut pool: Vec<(Candidate, ListingKey)> = stored
        .into_iter()
        .map(|l| (l.candidate, l.hackathon))
        .collect();
    let mut keyed_rows: HashMap<ListingKey, usize> = HashMap::new();
    let mut to_save = Vec::new();

    for (number, row) in rows {
        let row = match row {
            Ok(row) => row,
            Err(error) => {
                let name = layout.cell(&records[number - 2], Column::Hackathon);
                report.skipped.push(Skipped {
                    row: number,
                    name: name.map(str::to_owned),
                    why: SkipReason::Unreadable(error),
                });
                continue;
            }
        };
        let listing = listing(&row, &links);
        let key = key_of(&listing);
        let skip = |why| Skipped {
            row: number,
            name: Some(row.name.clone()),
            why,
        };
        if let Some(&first) = keyed_rows.get(&key) {
            report.skipped.push(skip(SkipReason::SameAsRow(first)));
            continue;
        }
        keyed_rows.insert(key.clone(), number);
        if already.contains(&key) {
            report.skipped.push(skip(SkipReason::AlreadyImported));
            continue;
        }

        let hackathon = match same_hackathon(store, &listing, &pool)? {
            Some(existing) => {
                report.merged += 1;
                existing
            }
            None => {
                report.imported += 1;
                key.clone()
            }
        };
        // The Fare is looked up for the dates of the listing the Hackathon is named by.
        let named_by = pool
            .iter()
            .find(|(c, _)| key_of(c) == hackathon)
            .map_or(&listing, |(c, _)| c);
        let (imported, warning) = imported(
            &row,
            listing.clone(),
            hackathon.clone(),
            named_by,
            profile,
            today,
        );
        if let Some(warning) = warning {
            report
                .warnings
                .push(format!("row {number} ({}): {warning}", row.name));
        }
        if imported.announced_on.is_some() {
            report.announced += 1;
        }
        pool.push((listing, hackathon));
        to_save.push(imported);
    }

    if !dry_run {
        store.save_imported(&to_save)?;
    }
    Ok(report)
}

/// The row as a listing from the `sheet-import` Source.
fn listing(row: &Row, links: &HashMap<String, usize>) -> Candidate {
    let source_key = match &row.link {
        Some(link) if links.get(link) == Some(&1) => link.clone(),
        Some(link) => format!("{link} {}", row.start),
        None => format!("{} {}", row.name, row.start),
    };
    // A known city by its usual name (Bruxelles is Brussels), as the Sources write it.
    let city = row.city.as_ref().map(|city| {
        row.country
            .and_then(|country| places::known_city_in(city, country, None))
            .map_or_else(|| city.clone(), str::to_owned)
    });
    Candidate {
        source: SOURCE.to_owned(),
        source_key,
        name: row.name.clone(),
        start: row.start,
        end: row.end,
        city,
        location: None,
        country: row.country.map(str::to_owned),
        link: row.link.clone(),
    }
}

/// The Hackathon a listing is a Duplicate listing of, among those in `pool`:
/// the first the rules say is the same, or that the Agent already said is.
fn same_hackathon(
    store: &Store,
    listing: &Candidate,
    pool: &[(Candidate, ListingKey)],
) -> anyhow::Result<Option<ListingKey>> {
    for (other, hackathon) in pool {
        let same = match compare::compare(listing, other) {
            Verdict::Same => true,
            Verdict::Unclear(_) => store.same_hackathon(listing, other)? == Some(true),
            Verdict::Different => false,
        };
        if same {
            return Ok(Some(hackathon.clone()));
        }
    }
    Ok(None)
}

/// Everything the row says, ready to store with the Hackathon it belongs to,
/// and what it says that can't be stored.
fn imported(
    row: &Row,
    listing: Candidate,
    hackathon: ListingKey,
    named_by: &Candidate,
    profile: &Profile,
    today: NaiveDate,
) -> (ImportedListing, Option<String>) {
    let marks = row.marks();
    let last_seen = row.last_checked.or(row.first_seen).unwrap_or(today);
    let undated = |mark: Option<Option<NaiveDate>>| mark.map(|day| day.unwrap_or(last_seen));
    let over = row.end < today;
    let ended_early = marks.ended && !over;
    let ruled_out = marks.ruled_out.is_some() || ended_early;
    let closed = ruled_out || marks.ended || (row.status == ApplicationStatus::Closed && over);

    let travel = row.travel_covered.funded();
    let lodging = row
        .accommodation
        .as_deref()
        .and_then(TravelCovered::parse)
        .map_or(TravelFunded::None, |a| a.funded());
    let travel_funded = match (travel, lodging) {
        (TravelFunded::None, TravelFunded::None) => TravelFunded::None,
        (TravelFunded::None, _) => TravelFunded::Partial,
        (travel, _) => travel,
    };
    let judgement = Judgement {
        is_hackathon: true,
        eligible: !ruled_out,
        eligibility_restriction: row.eligibility.clone(),
        travel_funded,
        travel_detail: travel_detail(row),
        deadline: row.apply_by,
        status: if closed {
            ApplicationStatus::Closed
        } else {
            row.status
        },
        reason: reason(row, marks.ruled_out.as_deref(), ended_early),
    };

    let trip = row.getting_there.as_ref().map(|there| trip(there, profile));
    let mut warning = None;
    let fare = match (&row.getting_there, &row.fare) {
        (
            Some(GettingThere::Fly { airport }),
            Some(FareCell::Price {
                amount,
                currency,
                from_airport,
                verified,
            }),
        ) => match fare_currency(currency, profile) {
            Some(currency) => Some(ImportedFare {
                query: FareQuery {
                    from: profile.home.airports.clone(),
                    to: airport.clone(),
                    outbound: named_by.start - Days::new(1),
                    inbound: named_by.end + Days::new(1),
                    currency,
                },
                price: *amount,
                from_airport: from_airport.clone(),
                verified: verified.unwrap_or(marks.fare_verified),
                source: SOURCE.to_owned(),
                checked_on: last_seen,
            }),
            None => {
                warning = Some(format!(
                    "its fare in {currency:?} isn't kept: only euros or the Profile's currency are"
                ));
                None
            }
        },
        (_, Some(FareCell::Price { .. })) => {
            warning = Some(
                "its fare isn't kept: a Fare is only kept for a flight to an airport".to_owned(),
            );
            None
        }
        _ => None,
    };
    let funding_announced_on = undated(marks.travel_notified);
    let announced_on = [undated(marks.notified), funding_announced_on]
        .into_iter()
        .flatten()
        .min();

    let imported = ImportedListing {
        listing,
        hackathon,
        first_seen: row.first_seen.unwrap_or(last_seen).min(last_seen),
        last_seen,
        fits: !ruled_out,
        judgement,
        judged_by: SOURCE.to_owned(),
        judged_on: last_seen,
        trip,
        fare,
        announced_on,
        funding_announced_on,
    };
    (imported, warning)
}

/// The currency code of a fare with `written` before its amount: a bare
/// amount is in euros (the personal tracker's column), and so is one in `€`;
/// otherwise it must be in the Profile's currency, as the export writes it.
fn fare_currency(written: &str, profile: &Profile) -> Option<String> {
    // How the Profile's currency writes an amount, without the amount: `€`, `$`, `CHF`.
    let own = profile.currency.format(0);
    let own = own.trim_end_matches('0').trim();
    if written == own || written.eq_ignore_ascii_case(profile.currency.code()) {
        Some(profile.currency.code().to_owned())
    } else if written.is_empty() || written == "€" {
        Some(SHEET_CURRENCY.to_owned())
    } else {
        None
    }
}

/// What travel and lodging are covered, as the Sheet says.
fn travel_detail(row: &Row) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(detail) = row.travel_covered.detail() {
        parts.push(detail.to_owned());
    }
    if let Some(accommodation) = &row.accommodation {
        let says_nothing = matches!(
            TravelCovered::parse(accommodation),
            Some(TravelCovered::No | TravelCovered::NotMentioned | TravelCovered::Unknown)
        );
        if !says_nothing {
            parts.push(format!("accommodation: {accommodation}"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The judgement's reason: where it came from, and the Sheet's free-text
/// columns that have no other place in the store.
fn reason(row: &Row, ruled_out: Option<&str>, ended_early: bool) -> String {
    let mut reason = String::from("Imported from the personal tracker's Sheet");
    if let Some(found_on) = &row.found_on {
        reason.push_str(&format!(" (found on {found_on})"));
    }
    reason.push('.');
    match ruled_out {
        Some("") => reason.push_str(" Marked out of scope there."),
        Some(why) => reason.push_str(&format!(" Marked out of scope there: {why}.")),
        None if ended_early => reason.push_str(" Marked as ended there."),
        None => {}
    }
    for (label, value) in [
        ("Format", &row.format),
        ("Theme", &row.theme),
        ("Prize", &row.prize),
        ("Fits budget?", &row.fits_budget),
        ("Notes", &row.notes),
    ] {
        if let Some(value) = value {
            reason.push_str(&format!(" {label}: {value}."));
        }
    }
    reason
}

/// The Trip the Getting there column describes, labelled as an Alert entry shows it.
fn trip(there: &GettingThere, profile: &Profile) -> Trip {
    match there {
        GettingThere::Local { train_minutes } => Trip {
            kind: TripKind::Local,
            train_minutes: *train_minutes,
            airport: None,
            label: match train_minutes {
                Some(minutes) => format!("local, train {}", sheet_layout::duration(*minutes)),
                None => "local".to_owned(),
            },
        },
        GettingThere::Train { minutes } => Trip {
            kind: TripKind::Train,
            train_minutes: Some(*minutes),
            airport: None,
            label: format!("train {}", sheet_layout::duration(*minutes)),
        },
        GettingThere::Fly { airport } => Trip {
            kind: TripKind::Flight,
            train_minutes: None,
            airport: Some(airport.clone()),
            label: format!("fly {}→{airport}", profile.home.airports[0]),
        },
        GettingThere::NoPracticalFlight { train_minutes } => Trip {
            kind: TripKind::Flight,
            train_minutes: *train_minutes,
            airport: None,
            label: match train_minutes {
                Some(minutes) => {
                    format!(
                        "no practical flight (train {})",
                        sheet_layout::duration(*minutes)
                    )
                }
                None => "no practical train or flight".to_owned(),
            },
        },
    }
}
