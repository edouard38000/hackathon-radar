//! The Profile rules that can be checked from a listing's data alone.

use chrono::NaiveDate;

use crate::candidate::Candidate;
use crate::profile::Profile;

/// Keeps the Candidates that pass every data-only rule: not over yet, and in
/// one of the Profile's Countries. A Candidate whose country is unknown is left out.
pub fn apply(candidates: Vec<Candidate>, profile: &Profile, today: NaiveDate) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|c| c.end >= today)
        .filter(|c| {
            c.country
                .as_deref()
                .is_some_and(|country| profile.includes_country(country))
        })
        .collect()
}
