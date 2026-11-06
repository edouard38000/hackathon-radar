use chrono::NaiveDate;

use crate::sources::devpost;

/// A listing collected from a Source, normalised so every Source looks the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Name of the Source it came from, such as `hackathon.be`.
    pub source: String,
    /// Identifies the listing within its Source and stays stable across Runs.
    pub source_key: String,
    pub name: String,
    pub start: NaiveDate,
    /// The last day of the Hackathon, inclusive.
    pub end: NaiveDate,
    pub city: Option<String>,
    /// The location as the Source wrote it, which may be a full address.
    pub location: Option<String>,
    pub country: Option<String>,
    pub link: Option<String>,
}

/// What a Candidate's start and end dates stand for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dates {
    /// The days the Hackathon takes place.
    EventDays,
    /// The period submissions are open. For an in-person Hackathon it spans
    /// the event's days but can open weeks before them, so these aren't the
    /// days someone has to be there.
    SubmissionPeriod,
}

impl Candidate {
    /// What [`Candidate::start`] and [`Candidate::end`] stand for. Devpost is
    /// the only Source that gives a submission period instead of the event's
    /// days, for every listing, so this follows from the Source.
    pub fn dates(&self) -> Dates {
        if self.source == devpost::NAME {
            Dates::SubmissionPeriod
        } else {
            Dates::EventDays
        }
    }
}
