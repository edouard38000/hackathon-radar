use chrono::NaiveDate;

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
