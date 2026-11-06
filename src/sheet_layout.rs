//! The layout of the personal tracker's Google Sheet, Hackathons tab
//! (docs/background.md): its columns, in order, with their headers.
//!
//! `export sheet` writes a CSV in this layout so it can be imported into
//! Sheets (ADR 0003: the Sheet is only an export). Reading a Sheet in the
//! other direction uses the same columns, so they live here, in one place,
//! with the parsers for each column's values ([`Row::parse`]). `import-sheet`
//! ([`crate::sheet_import`]) reads both the personal tracker's own forms and
//! the forms `export sheet` writes, so an exported CSV can be imported again.

use std::fmt;

use chrono::NaiveDate;

use crate::agent::{ApplicationStatus, TravelFunded};
use crate::sources::places;

/// One column of the Hackathons tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Hackathon,
    Start,
    End,
    City,
    Country,
    Format,
    ApplyBy,
    DaysLeftToApply,
    ApplicationStatus,
    TravelCovered,
    Accommodation,
    GettingThere,
    EstReturnFare,
    FitsBudget,
    Prize,
    Theme,
    Eligibility,
    Link,
    FlightsGoogle,
    FoundOn,
    FirstSeen,
    LastChecked,
    Notes,
}

impl Column {
    /// Every column, in the tab's order.
    pub const ALL: [Column; 23] = [
        Column::Hackathon,
        Column::Start,
        Column::End,
        Column::City,
        Column::Country,
        Column::Format,
        Column::ApplyBy,
        Column::DaysLeftToApply,
        Column::ApplicationStatus,
        Column::TravelCovered,
        Column::Accommodation,
        Column::GettingThere,
        Column::EstReturnFare,
        Column::FitsBudget,
        Column::Prize,
        Column::Theme,
        Column::Eligibility,
        Column::Link,
        Column::FlightsGoogle,
        Column::FoundOn,
        Column::FirstSeen,
        Column::LastChecked,
        Column::Notes,
    ];

    /// The header as the tab writes it. The personal tracker's
    /// "Getting there from Brussels" is "Getting there", since Home varies.
    pub fn header(self) -> &'static str {
        match self {
            Column::Hackathon => "Hackathon",
            Column::Start => "Start",
            Column::End => "End",
            Column::City => "City",
            Column::Country => "Country",
            Column::Format => "Format",
            Column::ApplyBy => "Apply by",
            Column::DaysLeftToApply => "Days left to apply",
            Column::ApplicationStatus => "Application status",
            Column::TravelCovered => "Travel covered",
            Column::Accommodation => "Accommodation",
            Column::GettingThere => "Getting there",
            Column::EstReturnFare => "Est. return fare",
            Column::FitsBudget => "Fits budget?",
            Column::Prize => "Prize",
            Column::Theme => "Theme",
            Column::Eligibility => "Eligibility",
            Column::Link => "Link",
            Column::FlightsGoogle => "Flights (Google)",
            Column::FoundOn => "Found on",
            Column::FirstSeen => "First seen",
            Column::LastChecked => "Last checked",
            Column::Notes => "Notes",
        }
    }

    /// The column with this header, matched without regard to case or
    /// surrounding spaces. The personal tracker's "Getting there from
    /// Brussels" (from any Home) reads as "Getting there", and its
    /// "Est. return fare (€)" (in any currency) as "Est. return fare".
    pub fn from_header(header: &str) -> Option<Column> {
        let header = header.trim();
        let lower = header.to_lowercase();
        Column::ALL
            .into_iter()
            .find(|c| c.header().eq_ignore_ascii_case(header))
            .or_else(|| {
                lower
                    .starts_with("getting there from ")
                    .then_some(Column::GettingThere)
            })
            .or_else(|| {
                (lower.starts_with("est. return fare (") && lower.ends_with(')'))
                    .then_some(Column::EstReturnFare)
            })
    }

    /// Where the column sits in the tab, from 0.
    pub fn index(self) -> usize {
        Column::ALL
            .iter()
            .position(|c| *c == self)
            .expect("every column is in ALL")
    }
}

/// The header row, in order.
pub fn headers() -> [&'static str; 23] {
    Column::ALL.map(Column::header)
}

/// Dates in the Sheet are written `YYYY-MM-DD`.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// Reads a CSV file into its records, header row included, dropping blank
/// lines and a leading byte-order mark.
pub fn read_csv(text: &str) -> anyhow::Result<Vec<Vec<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut records = Vec::new();
    for record in reader.records() {
        let record: Vec<String> = record?.iter().map(str::to_owned).collect();
        if record.iter().any(|cell| !cell.trim().is_empty()) {
            records.push(record);
        }
    }
    Ok(records)
}

/// Where each column is in a CSV, read from its header row, so a CSV with
/// the columns in another order (or with extra ones) still reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    at: [usize; 23],
}

impl Layout {
    /// The layout a header row gives; every column of the tab must be there.
    pub fn from_header(header: &[String]) -> Result<Layout, String> {
        let mut at = [usize::MAX; 23];
        for (position, cell) in header.iter().enumerate() {
            if let Some(column) = Column::from_header(cell)
                && at[column.index()] == usize::MAX
            {
                at[column.index()] = position;
            }
        }
        let missing: Vec<&str> = Column::ALL
            .into_iter()
            .filter(|c| at[c.index()] == usize::MAX)
            .map(Column::header)
            .collect();
        if missing.is_empty() {
            Ok(Layout { at })
        } else {
            Err(format!(
                "the CSV has no {} column; export the Hackathons tab of the personal tracker, \
                 or a CSV from `hackathon-radar export sheet`",
                missing
                    .iter()
                    .map(|h| format!("{h:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }

    /// One cell, trimmed, or `None` when empty. The apostrophe a spreadsheet
    /// export puts before a cell starting with `=`, `+`, `-` or `@` (so it
    /// isn't run as a formula) is dropped.
    pub fn cell<'c>(&self, cells: &'c [String], column: Column) -> Option<&'c str> {
        let cell = cells.get(self.at[column.index()])?;
        let cell = match cell.strip_prefix('\'') {
            Some(rest) if rest.starts_with(['=', '+', '-', '@', '\t', '\r']) => rest,
            _ => cell,
        };
        Some(cell.trim()).filter(|c| !c.is_empty())
    }
}

/// Reads a date in the Sheet's `YYYY-MM-DD`, or as the personal tracker
/// shows it (`Tue 3 Nov 2026`, its `ddd d mmm yyyy` format, with or without
/// the weekday). A weekday that doesn't match the date is refused.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    [DATE_FORMAT, "%a %d %b %Y", "%d %b %Y"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(text, format).ok())
}

/// The English name of the country a code or name stands for: `UK` (the
/// personal tracker's code, not `GB`), `BE` or `Belgium`.
pub fn parse_country(text: &str) -> Option<&'static str> {
    places::country_name(text)
}

/// Splits `Yes – detail` or `Full: detail` at its first separator (an en
/// dash, an em dash, a spaced hyphen or a colon) into the word before it and
/// the detail after it, if any.
fn split_detail(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    let at = ["–", "—", " - ", ":"]
        .iter()
        .filter_map(|sep| text.find(sep).map(|at| (at, sep.len())))
        .min();
    match at {
        Some((at, len)) => {
            let detail = text[at + len..].trim();
            (text[..at].trim(), (!detail.is_empty()).then_some(detail))
        }
        None => (text, None),
    }
}

/// Reads the Application status column: Open, `Open – until full`,
/// `Not open yet`, Waitlist, Closed or Unknown, in any case, or as the
/// export writes them (`Open until full`, `Status unknown`).
pub fn parse_status(text: &str) -> Option<ApplicationStatus> {
    let (head, detail) = split_detail(text);
    let head = head.to_lowercase();
    let detail = detail.map(str::to_lowercase);
    Some(match (head.as_str(), detail.as_deref()) {
        ("open", None) => ApplicationStatus::Open,
        ("open", Some("until full")) | ("open until full", None) => {
            ApplicationStatus::OpenUntilFull
        }
        ("not open yet", None) => ApplicationStatus::NotOpenYet,
        ("waitlist", None) => ApplicationStatus::Waitlist,
        ("closed", None) => ApplicationStatus::Closed,
        ("unknown" | "status unknown", None) => ApplicationStatus::Unknown,
        _ => return None,
    })
}

/// The Travel covered column (Accommodation uses the same words).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TravelCovered {
    /// `Yes – <detail>` (`Full: <detail>` in the export): covered.
    Yes(Option<String>),
    /// `Partial – <detail>`: partly covered.
    Partial(Option<String>),
    No,
    NotMentioned,
    Unknown,
}

impl TravelCovered {
    pub fn parse(text: &str) -> Option<Self> {
        let (head, detail) = split_detail(text);
        let detail = detail.map(str::to_owned);
        Some(match (head.to_lowercase().as_str(), detail) {
            ("yes" | "full", detail) => TravelCovered::Yes(detail),
            ("partial", detail) => TravelCovered::Partial(detail),
            ("no", None) => TravelCovered::No,
            ("not mentioned", None) => TravelCovered::NotMentioned,
            ("unknown", None) => TravelCovered::Unknown,
            _ => return None,
        })
    }

    /// What it says about Travel-funded.
    pub fn funded(&self) -> TravelFunded {
        match self {
            TravelCovered::Yes(_) => TravelFunded::Full,
            TravelCovered::Partial(_) => TravelFunded::Partial,
            TravelCovered::No | TravelCovered::NotMentioned | TravelCovered::Unknown => {
                TravelFunded::None
            }
        }
    }

    /// What is covered and for whom, when it says.
    pub fn detail(&self) -> Option<&str> {
        match self {
            TravelCovered::Yes(detail) | TravelCovered::Partial(detail) => detail.as_deref(),
            _ => None,
        }
    }
}

/// The Getting there column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GettingThere {
    /// `Local`, or `local, train ~30min` in the export.
    Local { train_minutes: Option<u32> },
    /// `Train ~2h`, `Train ~1h30` or `Train ~45min`.
    Train { minutes: u32 },
    /// `Fly → MUC` (`fly BRU→MUC` in the export), to an airport by its IATA code.
    Fly { airport: String },
    /// `no practical flight (train ~10h)` or `no practical train or flight`, in the export.
    NoPracticalFlight { train_minutes: Option<u32> },
}

impl GettingThere {
    pub fn parse(text: &str) -> Option<Self> {
        let lower = text.trim().to_lowercase();
        if let Some(rest) = lower.strip_prefix("local") {
            let train_minutes = match rest.trim().strip_prefix(',') {
                Some(train) => Some(parse_duration(train.trim().strip_prefix("train")?)?),
                None if rest.trim().is_empty() => None,
                None => return None,
            };
            return Some(GettingThere::Local { train_minutes });
        }
        if let Some(rest) = lower.strip_prefix("train") {
            return parse_duration(rest).map(|minutes| GettingThere::Train { minutes });
        }
        if lower == "no practical train or flight" {
            return Some(GettingThere::NoPracticalFlight {
                train_minutes: None,
            });
        }
        if let Some(rest) = lower.strip_prefix("no practical flight") {
            let train = rest.trim().strip_prefix("(train")?.strip_suffix(')')?;
            return Some(GettingThere::NoPracticalFlight {
                train_minutes: Some(parse_duration(train)?),
            });
        }
        let rest = lower.strip_prefix("fly")?.trim();
        // `→ MUC`, `BRU→MUC`, `to MUC` or `MUC`: the destination comes last.
        let airport = ["→", "->"]
            .iter()
            .find_map(|arrow| rest.split_once(arrow).map(|(_, to)| to))
            .or_else(|| rest.strip_prefix("to "))
            .unwrap_or(rest)
            .trim();
        (airport.len() == 3 && airport.bytes().all(|b| b.is_ascii_alphabetic())).then(|| {
            GettingThere::Fly {
                airport: airport.to_ascii_uppercase(),
            }
        })
    }
}

/// A train journey such as `~2h`, `~1h30`, `~1.5h` or `~45min`, in minutes.
fn parse_duration(text: &str) -> Option<u32> {
    let text: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '~')
        .collect();
    let number = |s: &str| s.replace(',', ".").parse::<f64>().ok();
    let minutes = if let Some(minutes) = text.strip_suffix("min") {
        number(minutes)?
    } else {
        let (hours, rest) = text.split_once('h')?;
        let rest = rest.strip_suffix("min").unwrap_or(rest);
        let rest = rest.strip_suffix('m').unwrap_or(rest);
        let extra = if rest.is_empty() { 0.0 } else { number(rest)? };
        number(hours)? * 60.0 + extra
    };
    (1.0..10_000.0)
        .contains(&minutes)
        .then(|| minutes.round() as u32)
}

/// `~45min`, `~2h` or `~1h30`.
pub fn duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("~{m}min"),
        (h, 0) => format!("~{h}h"),
        (h, m) => format!("~{h}h{m:02}"),
    }
}

/// The Est. return fare column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FareCell {
    /// A price: `185` in the personal tracker (in euros, as its header says),
    /// or `~€96 from CRL (unverified)` in the export.
    Price {
        amount: u32,
        /// What is written before the amount, such as `€` or `CHF`; empty for a bare number.
        currency: String,
        /// The Home airport the cheapest flight leaves from, when it says.
        from_airport: Option<String>,
        /// What the cell says about how the fare was found: `Some(false)`
        /// when marked `(unverified)`, `Some(true)` for an export's price
        /// without that mark, `None` when a bare price doesn't say.
        verified: Option<bool>,
    },
    /// An export's slot for a flight with no price: travel paid, a fare not
    /// looked up yet, or one checked closer to the date.
    NoPrice,
}

/// Reads the Est. return fare column. Zero isn't a price.
pub fn parse_fare(text: &str) -> Option<FareCell> {
    let text = text.trim();
    if [
        crate::report::TRAVEL_PAID,
        crate::report::FARE_PENDING,
        crate::fares::LATER,
    ]
    .iter()
    .any(|slot| text.eq_ignore_ascii_case(slot))
    {
        return Some(FareCell::NoPrice);
    }
    let (text, unverified) = match text.strip_suffix("(unverified)") {
        Some(rest) => (rest.trim(), true),
        None => (text, false),
    };
    let (text, from_airport) = match text.rsplit_once(" from ") {
        Some((rest, from)) if from.len() == 3 && from.bytes().all(|b| b.is_ascii_uppercase()) => {
            (rest.trim(), Some(from.to_owned()))
        }
        _ => (text, None),
    };
    let (text, approximate) = match text.strip_prefix('~') {
        Some(rest) => (rest.trim(), true),
        None => (text, false),
    };
    let digits_at = text.find(|c: char| c.is_ascii_digit())?;
    let currency = text[..digits_at].trim().to_owned();
    let amount = parse_amount(&text[digits_at..])?;
    let verified = if unverified {
        Some(false)
    } else {
        approximate.then_some(true)
    };
    Some(FareCell::Price {
        amount,
        currency,
        from_airport,
        verified,
    })
}

/// A whole amount: `185`, `185.40`, `1,240` or `99,50`.
fn parse_amount(text: &str) -> Option<u32> {
    let text = text.trim();
    // A comma followed by three digits groups thousands; otherwise it's a decimal point.
    let text = match text.split_once(',') {
        Some((whole, rest)) if rest.len() == 3 && !rest.contains(',') => format!("{whole}{rest}"),
        _ => text.replace(',', "."),
    };
    let amount: f64 = text.parse().ok()?;
    (0.5..f64::from(u32::MAX))
        .contains(&amount)
        .then(|| amount.round() as u32)
}

/// What the Notes column says that the tracker acted on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteMarks {
    /// `🔔 notified YYYY-MM-DD`: an Alert told the person about it. The
    /// earliest date when there are several; `Some(None)` when the mark has
    /// no readable date.
    pub notified: Option<Option<NaiveDate>>,
    /// `🔔 travel notified YYYY-MM-DD`: an Alert told the person it became
    /// Travel-funded. As for `notified`.
    pub travel_notified: Option<Option<NaiveDate>>,
    /// Why it was ruled out, possibly empty: the personal tracker's
    /// `OUT OF SCOPE – <why>`, or the export's `Doesn't Fit: <why>`.
    pub ruled_out: Option<String>,
    /// `Ended`: it's over.
    pub ended: bool,
    /// `fare checked … via Google Flights` (or Skyscanner, or Kayak): the
    /// fare was looked up from flight prices, so it isn't an Unverified fare.
    pub fare_verified: bool,
}

/// Reads the marks in a Notes cell.
pub fn read_notes(notes: &str) -> NoteMarks {
    let mut marks = NoteMarks::default();
    for (at, _) in notes.match_indices('🔔') {
        let rest = notes[at + '🔔'.len_utf8()..].trim_start();
        let (slot, rest) = if let Some(rest) = rest.strip_prefix("notified") {
            (&mut marks.notified, rest)
        } else if let Some(rest) = rest.strip_prefix("travel notified") {
            (&mut marks.travel_notified, rest)
        } else {
            continue;
        };
        let date = rest.trim_start().get(..10).and_then(parse_date);
        *slot = Some(match (*slot, date) {
            (Some(Some(earlier)), Some(date)) => Some(earlier.min(date)),
            (Some(Some(earlier)), None) => Some(earlier),
            (_, date) => date,
        });
    }
    for marker in ["OUT OF SCOPE", "Doesn't Fit"] {
        if let Some(at) = notes.find(marker) {
            let why = notes[at + marker.len()..]
                .split(['🔔', ';'])
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches(['–', '—', '-', ':'])
                .trim();
            marks.ruled_out = Some(why.to_owned());
            break;
        }
    }
    marks.ended = notes
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == "Ended");
    let lower = notes.to_lowercase();
    marks.fare_verified = lower.match_indices("fare checked").any(|(at, _)| {
        lower[at..].split_once("via").is_some_and(|(_, after)| {
            let after = after.trim_start();
            ["google flights", "skyscanner", "kayak"]
                .iter()
                .any(|site| after.starts_with(site))
        })
    });
    marks
}

/// Why a row can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowError {
    MissingName,
    /// A cell this column can't read, by the column's header.
    Unparseable {
        column: &'static str,
        value: String,
    },
    /// An End before the Start.
    EndsBeforeStart,
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RowError::MissingName => f.write_str("missing name"),
            RowError::Unparseable { column, value } => {
                write!(f, "unparseable {column} {value:?}")
            }
            RowError::EndsBeforeStart => f.write_str("ends before it starts"),
        }
    }
}

/// One row of the Hackathons tab, read. Free-text cells are `None` when empty.
/// Days left to apply and Flights (Google) are formula results and aren't read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub start: NaiveDate,
    /// The Start when the End is empty.
    pub end: NaiveDate,
    pub city: Option<String>,
    /// The country's English name, such as `United Kingdom` for `UK`.
    pub country: Option<&'static str>,
    pub format: Option<String>,
    pub apply_by: Option<NaiveDate>,
    /// Unknown when empty.
    pub status: ApplicationStatus,
    /// Unknown when empty.
    pub travel_covered: TravelCovered,
    /// As written: it may use the Travel covered words or be free text.
    pub accommodation: Option<String>,
    pub getting_there: Option<GettingThere>,
    pub fare: Option<FareCell>,
    /// As written: the tracker worked it out from the trip and fare.
    pub fits_budget: Option<String>,
    pub prize: Option<String>,
    pub theme: Option<String>,
    pub eligibility: Option<String>,
    pub link: Option<String>,
    pub found_on: Option<String>,
    pub first_seen: Option<NaiveDate>,
    pub last_checked: Option<NaiveDate>,
    pub notes: Option<String>,
}

impl Row {
    /// Reads a row's cells where `layout` says each column is; missing cells count as empty.
    pub fn parse(cells: &[String], layout: &Layout) -> Result<Row, RowError> {
        let text = |column: Column| layout.cell(cells, column).map(str::to_owned);
        fn read<T>(
            value: Option<String>,
            column: Column,
            parse: impl Fn(&str) -> Option<T>,
        ) -> Result<Option<T>, RowError> {
            match value {
                None => Ok(None),
                Some(value) => match parse(&value) {
                    Some(parsed) => Ok(Some(parsed)),
                    None => Err(RowError::Unparseable {
                        column: column.header(),
                        value,
                    }),
                },
            }
        }
        let date = |column: Column| read(text(column), column, parse_date);

        let name = text(Column::Hackathon).ok_or(RowError::MissingName)?;
        let start = date(Column::Start)?.ok_or(RowError::Unparseable {
            column: Column::Start.header(),
            value: String::new(),
        })?;
        let end = date(Column::End)?.unwrap_or(start);
        if end < start {
            return Err(RowError::EndsBeforeStart);
        }
        Ok(Row {
            name,
            start,
            end,
            city: text(Column::City),
            country: read(text(Column::Country), Column::Country, parse_country)?,
            format: text(Column::Format),
            apply_by: date(Column::ApplyBy)?,
            status: read(
                text(Column::ApplicationStatus),
                Column::ApplicationStatus,
                parse_status,
            )?
            .unwrap_or(ApplicationStatus::Unknown),
            travel_covered: read(
                text(Column::TravelCovered),
                Column::TravelCovered,
                TravelCovered::parse,
            )?
            .unwrap_or(TravelCovered::Unknown),
            accommodation: text(Column::Accommodation),
            getting_there: read(
                text(Column::GettingThere),
                Column::GettingThere,
                GettingThere::parse,
            )?,
            fare: read(
                text(Column::EstReturnFare),
                Column::EstReturnFare,
                parse_fare,
            )?,
            fits_budget: text(Column::FitsBudget),
            prize: text(Column::Prize),
            theme: text(Column::Theme),
            eligibility: text(Column::Eligibility),
            link: text(Column::Link),
            found_on: text(Column::FoundOn),
            first_seen: date(Column::FirstSeen)?,
            last_checked: date(Column::LastChecked)?,
            notes: text(Column::Notes),
        })
    }

    /// What its Notes say.
    pub fn marks(&self) -> NoteMarks {
        self.notes.as_deref().map(read_notes).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn headers_round_trip_and_are_unique() {
        for (i, column) in Column::ALL.into_iter().enumerate() {
            assert_eq!(column.index(), i);
            assert_eq!(Column::from_header(column.header()), Some(column));
        }
        assert_eq!(
            Column::from_header("  getting THERE "),
            Some(Column::GettingThere)
        );
        assert_eq!(
            Column::from_header("Getting there from Brussels"),
            Some(Column::GettingThere)
        );
        assert_eq!(Column::from_header("Getting"), None);
        assert_eq!(
            Column::from_header("Est. return fare (€)"),
            Some(Column::EstReturnFare)
        );
    }

    #[test]
    fn dates_are_read_in_iso_form_or_as_the_tracker_shows_them() {
        assert_eq!(parse_date("2026-11-03"), Some(day(2026, 11, 3)));
        assert_eq!(parse_date("Tue 3 Nov 2026"), Some(day(2026, 11, 3)));
        assert_eq!(parse_date(" Sat 14 Nov 2026 "), Some(day(2026, 11, 14)));
        assert_eq!(parse_date("3 Nov 2026"), Some(day(2026, 11, 3)));
        assert_eq!(parse_date("Wed 3 Nov 2026"), None, "the wrong weekday");
        assert_eq!(parse_date("sometime in Dec"), None);
    }

    #[test]
    fn countries_are_codes_with_uk_for_the_united_kingdom_or_names() {
        assert_eq!(parse_country("BE"), Some("Belgium"));
        assert_eq!(parse_country("UK"), Some("United Kingdom"));
        assert_eq!(parse_country("GB"), Some("United Kingdom"));
        assert_eq!(parse_country("Germany"), Some("Germany"));
        assert_eq!(parse_country("XX"), None);
    }

    #[test]
    fn statuses_are_read_in_the_trackers_words_or_the_exports() {
        for (text, status) in [
            ("Open", ApplicationStatus::Open),
            ("Open – until full", ApplicationStatus::OpenUntilFull),
            ("open - until full", ApplicationStatus::OpenUntilFull),
            ("Open until full", ApplicationStatus::OpenUntilFull),
            ("Not open yet", ApplicationStatus::NotOpenYet),
            ("Waitlist", ApplicationStatus::Waitlist),
            ("CLOSED", ApplicationStatus::Closed),
            ("Unknown", ApplicationStatus::Unknown),
            ("Status unknown", ApplicationStatus::Unknown),
        ] {
            assert_eq!(parse_status(text), Some(status), "{text}");
        }
        for status in ApplicationStatus::ALL {
            assert_eq!(parse_status(status.label()), Some(status));
        }
        assert_eq!(parse_status("Open – maybe"), None);
        assert_eq!(parse_status("Soon"), None);
    }

    #[test]
    fn travel_covered_gives_travel_funded_and_its_detail() {
        let yes = TravelCovered::parse("Yes – flights and hotel for finalists").unwrap();
        assert_eq!(yes.funded(), TravelFunded::Full);
        assert_eq!(yes.detail(), Some("flights and hotel for finalists"));
        let partial = TravelCovered::parse("Partial – €100 train refund").unwrap();
        assert_eq!(partial.funded(), TravelFunded::Partial);
        assert_eq!(partial.detail(), Some("€100 train refund"));
        assert_eq!(TravelCovered::parse("Yes"), Some(TravelCovered::Yes(None)));
        // As the export writes it.
        let full =
            TravelCovered::parse("Full: Flights and two hotel nights, for selected teams").unwrap();
        assert_eq!(full.funded(), TravelFunded::Full);
        assert_eq!(
            full.detail(),
            Some("Flights and two hotel nights, for selected teams")
        );
        assert_eq!(
            TravelCovered::parse("Partial"),
            Some(TravelCovered::Partial(None))
        );
        for (text, form) in [
            ("No", TravelCovered::No),
            ("Not mentioned", TravelCovered::NotMentioned),
            ("Unknown", TravelCovered::Unknown),
        ] {
            assert_eq!(TravelCovered::parse(text), Some(form.clone()));
            assert_eq!(form.funded(), TravelFunded::None);
        }
        assert_eq!(TravelCovered::parse("Maybe – ask"), None);
        assert_eq!(TravelCovered::parse("No – never"), None);
    }

    #[test]
    fn getting_there_is_local_a_train_or_a_flight() {
        let local = |train_minutes| Some(GettingThere::Local { train_minutes });
        let train = |minutes| Some(GettingThere::Train { minutes });
        let fly = |airport: &str| {
            Some(GettingThere::Fly {
                airport: airport.into(),
            })
        };
        assert_eq!(GettingThere::parse("Local"), local(None));
        assert_eq!(GettingThere::parse("local, train ~30min"), local(Some(30)));
        assert_eq!(GettingThere::parse("Train ~2h"), train(120));
        assert_eq!(GettingThere::parse("Train ~1h30"), train(90));
        assert_eq!(GettingThere::parse("Train ~1.5h"), train(90));
        assert_eq!(GettingThere::parse("train ~45min"), train(45));
        assert_eq!(GettingThere::parse("Train ~3h 15m"), train(195));
        assert_eq!(GettingThere::parse("Fly → MUC"), fly("MUC"));
        assert_eq!(GettingThere::parse("fly -> lis"), fly("LIS"));
        assert_eq!(GettingThere::parse("fly BRU→MUC"), fly("MUC"));
        assert_eq!(
            GettingThere::parse("no practical flight (train ~10h)"),
            Some(GettingThere::NoPracticalFlight {
                train_minutes: Some(600)
            })
        );
        assert_eq!(
            GettingThere::parse("no practical train or flight"),
            Some(GettingThere::NoPracticalFlight {
                train_minutes: None
            })
        );
        assert_eq!(GettingThere::parse("Train"), None);
        assert_eq!(GettingThere::parse("Local-ish"), None);
        assert_eq!(GettingThere::parse("Fly → Munich"), None);
        assert_eq!(GettingThere::parse("Bike"), None);
    }

    #[test]
    fn fares_are_bare_prices_or_as_the_export_writes_them() {
        let price = |amount, currency: &str, from: Option<&str>, verified| {
            Some(FareCell::Price {
                amount,
                currency: currency.into(),
                from_airport: from.map(str::to_owned),
                verified,
            })
        };
        assert_eq!(parse_fare("185"), price(185, "", None, None));
        assert_eq!(parse_fare("185.40"), price(185, "", None, None));
        assert_eq!(parse_fare("€ 96"), price(96, "€", None, None));
        assert_eq!(parse_fare("1,240"), price(1240, "", None, None));
        assert_eq!(parse_fare("99,50"), price(100, "", None, None));
        assert_eq!(parse_fare("~€185"), price(185, "€", None, Some(true)));
        assert_eq!(
            parse_fare("~€96 from CRL (unverified)"),
            price(96, "€", Some("CRL"), Some(false))
        );
        assert_eq!(
            parse_fare("~CHF 1,250"),
            price(1250, "CHF", None, Some(true))
        );
        for slot in [
            "travel paid",
            "fare not looked up yet",
            "fare checked closer to the date",
        ] {
            assert_eq!(parse_fare(slot), Some(FareCell::NoPrice));
        }
        assert_eq!(parse_fare("0"), None);
        assert_eq!(parse_fare("cheap"), None);
    }

    #[test]
    fn notes_give_the_notified_marks_and_scope() {
        let marks = read_notes("🔔 notified 2026-11-05 · re-sent 🔔 notified 2026-11-02");
        assert_eq!(marks.notified, Some(Some(day(2026, 11, 2))));
        assert_eq!(marks.travel_notified, None);

        let marks = read_notes("Travel grant added. 🔔 travel notified 2026-11-05");
        assert_eq!(marks.notified, None);
        assert_eq!(marks.travel_notified, Some(Some(day(2026, 11, 5))));

        assert_eq!(read_notes("🔔 notified").notified, Some(None));

        let marks = read_notes("OUT OF SCOPE – pitch competition, nothing is built");
        assert_eq!(
            marks.ruled_out.as_deref(),
            Some("pitch competition, nothing is built")
        );
        assert!(!marks.ended);
        let marks = read_notes(
            "Start and End are the submission period; Doesn't Fit: over the Flight budget (€200)",
        );
        assert_eq!(
            marks.ruled_out.as_deref(),
            Some("over the Flight budget (€200)")
        );

        assert!(read_notes("Ended. Winners announced.").ended);
        assert!(!read_notes("Extended deadline").ended);
        assert!(!read_notes("registration ended").ended);

        assert!(read_notes("fare checked 2026-11-03 via Google Flights").fare_verified);
        assert!(read_notes("Fare checked via Skyscanner, BRU").fare_verified);
        assert!(!read_notes("fare estimated from web search").fare_verified);
        assert!(!read_notes("fare checked via web search").fare_verified);
        assert_eq!(read_notes(""), NoteMarks::default());
    }

    #[test]
    fn csv_is_read_with_quotes_and_without_blank_lines() {
        let text = "\u{feff}a,b,c\r\n\"x, y\",\"say \"\"hi\"\"\",\"two\nlines\"\r\n,,\r\nlast,,\n";
        assert_eq!(
            read_csv(text).unwrap(),
            [
                vec!["a", "b", "c"],
                vec!["x, y", "say \"hi\"", "two\nlines"],
                vec!["last", "", ""],
            ]
        );
    }

    fn header() -> Vec<String> {
        headers().map(str::to_owned).to_vec()
    }

    #[test]
    fn the_layout_comes_from_the_header_row() {
        assert!(Layout::from_header(&header()).is_ok());
        let mut tracker = header();
        tracker[Column::GettingThere.index()] = "Getting there from Brussels".into();
        tracker[Column::EstReturnFare.index()] = "Est. return fare (€)".into();
        assert!(Layout::from_header(&tracker).is_ok());

        // Columns in another order still read.
        let mut shuffled = header();
        shuffled.swap(0, 22);
        let layout = Layout::from_header(&shuffled).unwrap();
        let mut cells = vec![String::new(); 23];
        cells[22] = "Hack".into();
        assert_eq!(layout.cell(&cells, Column::Hackathon), Some("Hack"));

        let error = Layout::from_header(&header()[..21]).unwrap_err();
        assert!(error.contains("\"Last checked\", \"Notes\""), "{error}");
    }

    #[test]
    fn a_cell_loses_the_apostrophe_that_kept_it_from_being_a_formula() {
        let layout = Layout::from_header(&header()).unwrap();
        let mut cells = vec![String::new(); 23];
        for (text, read) in [
            ("'=HYPERLINK(\"x\")", "=HYPERLINK(\"x\")"),
            ("'+32 Hack", "+32 Hack"),
            ("'-1", "-1"),
            ("'@home", "@home"),
            ("'Tis the season", "'Tis the season"),
            ("  ", ""),
        ] {
            cells[0] = text.into();
            assert_eq!(layout.cell(&cells, Column::Hackathon).unwrap_or(""), read);
        }
    }

    fn row(pairs: &[(Column, &str)]) -> Result<Row, RowError> {
        let mut cells = vec![String::new(); 23];
        for (column, value) in pairs {
            cells[column.index()] = (*value).to_owned();
        }
        Row::parse(&cells, &Layout::from_header(&header()).unwrap())
    }

    #[test]
    fn a_row_needs_a_name_and_a_start_and_readable_cells() {
        use Column::*;
        let hack = row(&[(Hackathon, "Hack"), (Start, "Tue 3 Nov 2026")]).unwrap();
        assert_eq!(hack.end, day(2026, 11, 3));
        assert_eq!(hack.status, super::ApplicationStatus::Unknown);
        assert_eq!(hack.travel_covered, super::TravelCovered::Unknown);
        assert_eq!(
            row(&[(Start, "Tue 3 Nov 2026")]),
            Err(RowError::MissingName)
        );
        assert_eq!(
            row(&[(Hackathon, "Hack"), (Start, "soon")])
                .unwrap_err()
                .to_string(),
            "unparseable Start \"soon\""
        );
        assert_eq!(
            row(&[
                (Hackathon, "Hack"),
                (Start, "2026-11-03"),
                (End, "2026-11-01")
            ]),
            Err(RowError::EndsBeforeStart)
        );
        assert_eq!(
            row(&[
                (Hackathon, "Hack"),
                (Start, "2026-11-03"),
                (GettingThere, "Bike")
            ])
            .unwrap_err()
            .to_string(),
            "unparseable Getting there \"Bike\""
        );
        // Formula results are never read.
        assert!(
            row(&[
                (Hackathon, "Hack"),
                (Start, "2026-11-03"),
                (DaysLeftToApply, "#REF!"),
                (FlightsGoogle, "Search flights")
            ])
            .is_ok()
        );
    }
}
