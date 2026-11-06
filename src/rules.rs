//! The Profile rules that can be checked from a listing's data alone, and Fit.
//!
//! Countries and the Train limit (ticket 07): a Hackathon in one of the
//! Profile's Countries is in scope when its trip is local or by train. Trips
//! over the Train limit are labelled as flights ([`crate::travel`]) and, for
//! now, stay in scope too: the Flight budget, which decides them, comes with
//! Fares. So the Train limit labels trips but doesn't yet filter any out, and
//! Countries is still the only place-based filter.
//!
//! The Travel-funded rule (ticket 09): with `travel.travel_funded_anywhere`, a
//! Hackathon outside Countries passes the data-only rules too, because only the
//! Agent can tell whether it is Travel-funded; it Fits only once judged so.

use chrono::NaiveDate;

use crate::agent::{Judgement, TravelFunded};
use crate::candidate::Candidate;
use crate::profile::Profile;

/// Keeps the Candidates that pass every data-only rule: not over yet, and in
/// one of the Profile's Countries, or anywhere when the Travel-funded rule
/// allows. A Candidate whose country is unknown is left out.
pub fn apply(candidates: Vec<Candidate>, profile: &Profile, today: NaiveDate) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|c| passes(c, profile, today))
        .collect()
}

fn passes(candidate: &Candidate, profile: &Profile, today: NaiveDate) -> bool {
    candidate.end >= today
        && candidate.country.is_some()
        && (in_countries(candidate, profile) || profile.travel.travel_funded_anywhere)
}

/// Whether the Candidate is in one of the Profile's Countries.
pub fn in_countries(candidate: &Candidate, profile: &Profile) -> bool {
    candidate
        .country
        .as_deref()
        .is_some_and(|country| profile.includes_country(country))
}

/// Whether a judged Hackathon Fits: it passes the data-only rules, the Agent
/// judged it a Hackathon the person is eligible for, and it is in Countries or
/// Travel-funded under the Travel-funded rule.
pub fn fits(
    candidate: &Candidate,
    judgement: &Judgement,
    profile: &Profile,
    today: NaiveDate,
) -> bool {
    passes(candidate, profile, today)
        && judgement.fits()
        && (in_countries(candidate, profile) || judgement.travel_funded != TravelFunded::None)
}
