//! The Sheet export: a CSV in the personal tracker's Hackathons tab layout
//! ([`crate::sheet_layout`]), to import into Google Sheets. Dates are
//! `YYYY-MM-DD`; quoting and line endings follow RFC 4180.
//!
//! The engine doesn't track every column the personal tracker had: Format,
//! Accommodation, Prize and Theme are left empty, and "Fits budget?" is only
//! filled for a flight the Flight budget decides.

use chrono::NaiveDate;

use crate::agent::TravelFunded;
use crate::candidate::Dates;
use crate::fares::FareCheck;
use crate::profile::Profile;
use crate::report::{self, Listed};
use crate::sheet_layout::{Column, DATE_FORMAT, headers};

/// The CSV and how many Hackathons it lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub text: String,
    pub rows: usize,
}

impl Sheet {
    /// One line for the person: what was written, or that there is nothing yet.
    pub fn summary(&self) -> String {
        match self.rows {
            0 => {
                "Nothing yet: no Hackathon fits you, so the CSV has only its header row.".to_owned()
            }
            1 => "1 Hackathon in the CSV.".to_owned(),
            n => format!("{n} Hackathons in the CSV."),
        }
    }
}

/// The CSV of `listed`, one row per Hackathon, under the header row.
pub fn sheet(listed: &[Listed], profile: &Profile, today: NaiveDate) -> anyhow::Result<Sheet> {
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_writer(Vec::new());
    writer.write_record(headers())?;
    for l in listed {
        writer.write_record(row(l, profile, today))?;
    }
    let text = String::from_utf8(writer.into_inner().map_err(|e| e.into_error())?)?;
    Ok(Sheet {
        text,
        rows: listed.len(),
    })
}

/// One Hackathon's row, in [`Column::ALL`] order.
pub fn row(listed: &Listed, profile: &Profile, today: NaiveDate) -> Vec<String> {
    Column::ALL
        .into_iter()
        .map(|column| as_text(cell(column, listed, profile, today).unwrap_or_default()))
        .collect()
}

/// Keeps a cell that a spreadsheet would read as a formula (it starts with
/// `=`, `+`, `-` or `@`) as text, with a leading apostrophe, which Sheets
/// hides. Names and details come from listings on the web, so they are never
/// run as formulas.
fn as_text(cell: String) -> String {
    if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{cell}")
    } else {
        cell
    }
}

fn cell(column: Column, l: &Listed, profile: &Profile, today: NaiveDate) -> Option<String> {
    let date = |d: NaiveDate| d.format(DATE_FORMAT).to_string();
    let h = &l.hackathon;
    match column {
        Column::Hackathon => Some(h.name.clone()),
        Column::Start => Some(date(h.start)),
        Column::End => Some(date(h.end)),
        Column::City => h.city.clone(),
        Column::Country => h.country.clone(),
        Column::ApplyBy => l.judgement.deadline.map(date),
        Column::DaysLeftToApply => l.days_left(today).map(|d| d.to_string()),
        Column::ApplicationStatus => Some(l.status.label().to_owned()),
        Column::TravelCovered => {
            let covered = match l.judgement.travel_funded {
                TravelFunded::None => "No",
                TravelFunded::Partial => "Partial",
                TravelFunded::Full => "Full",
            };
            Some(match &l.judgement.travel_detail {
                Some(detail) if l.judgement.travel_funded != TravelFunded::None => {
                    format!("{covered}: {detail}")
                }
                _ => covered.to_owned(),
            })
        }
        Column::GettingThere => l.trip.as_ref().map(|t| t.label.clone()),
        Column::EstReturnFare => l.fare.clone(),
        Column::FitsBudget => match &l.fare_check {
            Some(FareCheck::Priced { within_budget, .. }) => {
                Some(if *within_budget { "Yes" } else { "No" }.to_owned())
            }
            _ => None,
        },
        Column::Eligibility => l.judgement.eligibility_restriction.clone(),
        Column::Link => h.link.clone(),
        Column::FlightsGoogle => report::google_flights(profile, l),
        Column::FoundOn => Some(l.sources.join(", ")),
        Column::FirstSeen => l.first_seen.map(date),
        Column::LastChecked => Some(date(
            l.last_seen
                .map_or(l.judged_on, |seen| seen.max(l.judged_on)),
        )),
        Column::Notes => {
            let mut notes = Vec::new();
            if h.dates() == Dates::SubmissionPeriod {
                notes.push("Start and End are the submission period".to_owned());
            }
            if let Some(why) = &l.not_fitting {
                notes.push(format!("Doesn't Fit: {why}"));
            }
            (!notes.is_empty()).then(|| notes.join("; "))
        }
        Column::Format | Column::Accommodation | Column::Prize | Column::Theme => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_a_spreadsheet_would_run_as_formulas_stay_text() {
        assert_eq!(as_text("=HYPERLINK(\"x\")".into()), "'=HYPERLINK(\"x\")");
        assert_eq!(as_text("+32 Hack".into()), "'+32 Hack");
        assert_eq!(as_text("@home".into()), "'@home");
        assert_eq!(as_text("-1".into()), "'-1");
        assert_eq!(as_text("~€96".into()), "~€96");
        assert_eq!(as_text(String::new()), "");
    }
}
