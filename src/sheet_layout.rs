//! The layout of the personal tracker's Google Sheet, Hackathons tab
//! (docs/background.md): its columns, in order, with their headers.
//!
//! `export sheet` writes a CSV in this layout so it can be imported into
//! Sheets (ADR 0003: the Sheet is only an export). Reading a Sheet in the
//! other direction uses the same columns, so they live here, in one place.

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
    /// Brussels" (from any Home) reads as "Getting there".
    pub fn from_header(header: &str) -> Option<Column> {
        let header = header.trim();
        Column::ALL
            .into_iter()
            .find(|c| c.header().eq_ignore_ascii_case(header))
            .or_else(|| {
                header
                    .to_lowercase()
                    .starts_with("getting there from ")
                    .then_some(Column::GettingThere)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
