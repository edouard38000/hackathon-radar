//! The Profile rules that can be checked from a listing's data alone, and Fit.
//!
//! Countries and the Train limit (ticket 07): a Hackathon in one of the
//! Profile's Countries is in scope when its trip is local or by train. Trips
//! over the Train limit are labelled as flights ([`crate::travel`]); the
//! Flight budget then decides them once their Fare is known
//! ([`crate::fares`], after the Agent has judged), so it isn't checked here.
//!
//! Countries can name regions ([`crate::countries`]): `Schengen` passes its
//! member countries, and `US West Coast` the Hackathons in California, Oregon
//! and Washington, by the US state their location or city gives.
//!
//! The Travel-funded rule (ticket 09): with `travel.travel_funded_anywhere`, a
//! Hackathon outside Countries passes the data-only rules too, because only the
//! Agent can tell whether it is Travel-funded; it Fits only once judged so.
//! The same rule lets a Travel-funded flight skip the Flight budget.
//!
//! Themes (ticket 24, ADR 0010) need judgement, so the Agent answers them as
//! part of judging; [`fits_themes`] applies its answer.

use chrono::NaiveDate;

use crate::agent::{Judgement, ThemeFit, TravelFunded};
use crate::candidate::Candidate;
use crate::profile::Profile;
use crate::sources::places;

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

/// Whether the Candidate is in one of the Profile's Countries, with regions
/// expanded. A US Candidate whose state is unknown is only in Countries when
/// they include the whole United States, not just some states.
pub fn in_countries(candidate: &Candidate, profile: &Profile) -> bool {
    candidate.country.as_deref().is_some_and(|country| {
        let state = places::us_state(candidate.city.as_deref(), candidate.location.as_deref());
        profile.countries().includes(country, state)
    })
}

/// Whether a judged Hackathon Fits: it passes the data-only rules, the Agent
/// judged it a Hackathon the person is eligible for, it is in Countries or
/// Travel-funded under the Travel-funded rule, and it Fits the Themes.
pub fn fits(
    candidate: &Candidate,
    judgement: &Judgement,
    profile: &Profile,
    today: NaiveDate,
) -> bool {
    passes(candidate, profile, today)
        && judgement.fits()
        && (in_countries(candidate, profile) || judgement.travel_funded != TravelFunded::None)
        && fits_themes(judgement, profile)
}

/// Whether a judged Hackathon Fits the Profile's Themes (ticket 24, ADR 0010):
/// with no Themes every subject does; with Themes, every Hackathon but one the
/// Agent judged off-Theme does, on-Theme and open-themed alike. A judgement
/// with no answer about the Profile's current Themes (made before they were
/// set, or about other Themes) Fits until a recheck answers about them.
pub fn fits_themes(judgement: &Judgement, profile: &Profile) -> bool {
    !matches!(theme_fit(judgement, profile), Some(ThemeFit::OffTheme))
}

/// The Theme a judged Hackathon is about, among the Profile's current Themes,
/// as an Alert entry shows it; `None` when it is open-themed or not judged on them.
pub fn matched_theme<'j>(judgement: &'j Judgement, profile: &Profile) -> Option<&'j str> {
    theme_fit(judgement, profile).and_then(ThemeFit::theme)
}

/// The judgement's answer about the Profile's current Themes, if it has one.
fn theme_fit<'j>(judgement: &'j Judgement, profile: &Profile) -> Option<&'j ThemeFit> {
    judgement
        .theme
        .as_ref()
        .filter(|answer| !profile.themes.is_empty() && answer.is_about(&profile.themes))
        .map(|answer| &answer.fit)
}
