//! Reports and exports (ticket 15): what the store says about each Hackathon,
//! gathered once for the Markdown or HTML report ([`Report`]) and the exports
//! ([`crate::export`]).
//!
//! Everything here is generated from the local SQLite store and never read
//! back (ADR 0003). Fit is decided the way a Run decides it, from the stored
//! judgement, the Profile's rules, and the stored Trip and Fare for the Flight
//! budget; nothing is looked up. The wording reuses the Alert entry's
//! formatters ([`crate::alert`]), so a report says things the way an Alert does.

mod html;
mod markdown;

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::agent::{ApplicationStatus, Judgement, TravelFunded};
use crate::alert;
use crate::candidate::{Candidate, Dates};
use crate::deadlines;
use crate::fares::{self, FareCheck, Priced};
use crate::profile::Profile;
use crate::rules;
use crate::store::{ListingKey, RunRecord, Store, key_of};
use crate::travel::{Trip, TripKind};

/// Deadlines this many days away or fewer are listed under "Closing soon".
pub const CLOSING_SOON_DAYS: i64 = 7;

/// The Fare slot of a Travel-funded flight that skips the Flight budget.
pub const TRAVEL_PAID: &str = "travel paid";

/// The Fare slot of a flight still waiting for its Fare.
pub const FARE_PENDING: &str = "fare not looked up yet";

/// Which Hackathons a report or export lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    /// Those that Fit and aren't closed.
    #[default]
    Fitting,
    /// Every Hackathon the Agent has judged, including those that don't Fit,
    /// are closed or are over.
    All,
}

/// One Hackathon in a report or export, with everything the store says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub hackathon: Candidate,
    /// Its latest judgement or recheck.
    pub judgement: Judgement,
    /// The day of that judgement or recheck.
    pub judged_on: NaiveDate,
    /// The day a Run first collected it, when stored.
    pub first_seen: Option<NaiveDate>,
    /// The day of the latest Run that still found it, when stored.
    pub last_seen: Option<NaiveDate>,
    /// The Sources its listings came from, sorted.
    pub sources: Vec<String>,
    pub trip: Option<Trip>,
    /// Where it stands with the Flight budget, for a flight it decides.
    pub fare_check: Option<FareCheck>,
    /// The Fare slot, as an Alert entry shows it after the Trip, or
    /// [`TRAVEL_PAID`] or [`FARE_PENDING`].
    pub fare: Option<String>,
    /// The application status today: a passed deadline means closed.
    pub status: ApplicationStatus,
    /// Why it doesn't Fit; `None` when it Fits.
    pub not_fitting: Option<String>,
}

impl Listed {
    pub fn fits(&self) -> bool {
        self.not_fitting.is_none()
    }

    pub fn is_closed(&self) -> bool {
        self.status == ApplicationStatus::Closed
    }

    /// The days left to apply, while the deadline hasn't passed and applications aren't closed.
    pub fn days_left(&self, today: NaiveDate) -> Option<i64> {
        let deadline = self.judgement.deadline?;
        let days = (deadline - today).num_days();
        (days >= 0 && !self.is_closed()).then_some(days)
    }

    /// `Munich, Germany`, or whichever of the two is known.
    pub fn place(&self) -> Option<String> {
        let parts: Vec<&str> = [&self.hackathon.city, &self.hackathon.country]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    /// `5–6 Dec 2026`, or `Submission period 1 Nov – 6 Dec 2026` for a
    /// listing whose dates are a submission period (Devpost).
    pub fn dates(&self) -> String {
        let dates = alert::format_dates(self.hackathon.start, self.hackathon.end);
        match self.hackathon.dates() {
            Dates::EventDays => dates,
            Dates::SubmissionPeriod => format!("Submission period {dates}"),
        }
    }

    /// `Open · deadline 28 Nov 2026 (19 days left)`, as an Alert entry writes it.
    pub fn application(&self, today: NaiveDate) -> String {
        alert::application_line(self.status, self.judgement.deadline, self.days_left(today))
    }

    /// The Trip and the Fare, such as `fly BRU→MUC · ~€96 from CRL (unverified)`.
    pub fn getting_there(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .trip
            .iter()
            .map(|t| t.label.as_str())
            .chain(self.fare.as_deref())
            .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// `Travel-funded (full): Flights and two hotel nights`, when it is.
    pub fn funding(&self) -> Option<String> {
        (self.judgement.travel_funded != TravelFunded::None).then(|| {
            alert::travel_funding(
                self.judgement.travel_funded,
                self.judgement.travel_detail.as_deref(),
                false,
            )
        })
    }
}

/// The Hackathons in `scope`, soonest first, as the store has them on `today`.
pub fn collect(
    store: &Store,
    profile: &Profile,
    today: NaiveDate,
    scope: Scope,
) -> anyhow::Result<Vec<Listed>> {
    let known: Vec<deadlines::Known> = deadlines::known(store)?
        .into_iter()
        .filter(|k| k.judgement.is_hackathon)
        .collect();
    let hackathons: Vec<Candidate> = known.iter().map(|k| k.hackathon.clone()).collect();
    let priced = Priced::from_store(profile, store, &hackathons, today)?;

    let mut seen: HashMap<ListingKey, (NaiveDate, NaiveDate)> = HashMap::new();
    let mut sources: HashMap<ListingKey, Vec<String>> = HashMap::new();
    for listing in store.listings()? {
        if listing.key() == listing.hackathon {
            seen.insert(listing.key(), (listing.first_seen, listing.last_seen));
        }
        sources
            .entry(listing.hackathon.clone())
            .or_default()
            .push(listing.candidate.source);
    }
    for h in store.hackathons()? {
        seen.insert((h.source, h.source_key), (h.first_seen, h.last_seen));
    }

    let mut listed = Vec::new();
    for k in known {
        let key = key_of(&k.hackathon);
        let status = k.judgement.status_on(today);
        let not_fitting = why_not_fitting(&k.hackathon, &k.judgement, profile, &priced, today);
        if scope == Scope::Fitting && (not_fitting.is_some() || status == ApplicationStatus::Closed)
        {
            continue;
        }
        let mut found_on = sources.remove(&key).unwrap_or_default();
        if found_on.is_empty() {
            found_on.push(k.hackathon.source.clone());
        }
        found_on.sort();
        found_on.dedup();
        let (first_seen, last_seen) = seen.get(&key).copied().unzip();
        listed.push(Listed {
            trip: store.trip(&key.0, &key.1)?,
            fare_check: priced.check(&k.hackathon).cloned(),
            fare: fare_slot(&priced, &k.hackathon),
            judged_on: k.judged_on,
            first_seen,
            last_seen,
            sources: found_on,
            status,
            not_fitting,
            judgement: k.judgement,
            hackathon: k.hackathon,
        });
    }
    listed.sort_by(|a, b| {
        let order = |l: &Listed| {
            (
                l.hackathon.start,
                l.hackathon.name.clone(),
                key_of(&l.hackathon),
            )
        };
        order(a).cmp(&order(b))
    });
    Ok(listed)
}

/// The Fare slot: the Alert's Fare text, or what stands in for it.
fn fare_slot(priced: &Priced, hackathon: &Candidate) -> Option<String> {
    match priced.check(hackathon)? {
        FareCheck::TravelPaid(_) => Some(TRAVEL_PAID.to_owned()),
        FareCheck::Waiting => Some(FARE_PENDING.to_owned()),
        FareCheck::Priced { .. } | FareCheck::Later => priced.fare_text(hackathon),
    }
}

/// Why a judged Hackathon doesn't Fit, in a few words, or `None` when it Fits.
fn why_not_fitting(
    hackathon: &Candidate,
    judgement: &Judgement,
    profile: &Profile,
    priced: &Priced,
    today: NaiveDate,
) -> Option<String> {
    if rules::fits(hackathon, judgement, profile, today) {
        return match priced.check(hackathon) {
            Some(FareCheck::Priced {
                within_budget: false,
                ..
            }) => Some(match profile.travel.flight_budget {
                Some(budget) => format!(
                    "over the Flight budget ({})",
                    profile.currency.format(u64::from(budget))
                ),
                None => "over the Flight budget".to_owned(),
            }),
            _ => None,
        };
    }
    Some(if hackathon.end < today {
        "already over".to_owned()
    } else if !judgement.eligible {
        match &judgement.eligibility_restriction {
            Some(restriction) => format!("not eligible: {restriction}"),
            None => "not eligible".to_owned(),
        }
    } else if hackathon.country.is_none() {
        "country unknown".to_owned()
    } else {
        "outside your Countries".to_owned()
    })
}

/// The report: the Hackathons in scope, the deadlines closing soon and the last Run.
#[derive(Debug, Clone)]
pub struct Report {
    pub today: NaiveDate,
    pub scope: Scope,
    pub listed: Vec<Listed>,
    /// The latest Run in the history, if any.
    pub last_run: Option<RunRecord>,
}

impl Report {
    /// Gathers the report from the store as of `today`.
    pub fn build(
        store: &Store,
        profile: &Profile,
        today: NaiveDate,
        scope: Scope,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            today,
            scope,
            listed: collect(store, profile, today, scope)?,
            last_run: store.runs()?.pop(),
        })
    }

    /// The listed Hackathons whose deadline is at most [`CLOSING_SOON_DAYS`]
    /// away and still open to applications, nearest deadline first.
    pub fn closing_soon(&self) -> Vec<&Listed> {
        let mut soon: Vec<&Listed> = self
            .listed
            .iter()
            .filter(|l| {
                l.days_left(self.today)
                    .is_some_and(|days| days <= CLOSING_SOON_DAYS)
            })
            .collect();
        soon.sort_by_key(|l| l.judgement.deadline);
        soon
    }

    /// The report in Markdown.
    pub fn markdown(&self) -> String {
        markdown::render(self)
    }

    /// The report as a standalone HTML page.
    pub fn html(&self) -> String {
        html::render(self)
    }

    /// One line for the person: how many Hackathons the report lists, or that there is nothing yet.
    pub fn summary_line(&self) -> String {
        match self.listed.len() {
            0 => "Nothing yet: the report lists no Hackathon.".to_owned(),
            1 => "The report lists 1 Hackathon.".to_owned(),
            n => format!("The report lists {n} Hackathons."),
        }
    }

    /// The sentence under the title: how many Hackathons are listed, as of when.
    fn summary(&self) -> String {
        let today = alert::format_date(self.today);
        let count = match self.listed.len() {
            1 => "1 Hackathon".to_owned(),
            n => format!("{n} Hackathons"),
        };
        match (self.scope, self.listed.len()) {
            (Scope::Fitting, 0) => format!(
                "Nothing yet: no Hackathon fits you as of {today}. Hackathons that Fit appear \
                 here once a Run finds them."
            ),
            (Scope::All, 0) => format!(
                "Nothing yet: no Hackathon is stored as of {today}. They appear here once a Run \
                 finds them."
            ),
            (Scope::Fitting, 1) => format!("{count} fits you and is not closed, as of {today}."),
            (Scope::Fitting, _) => format!("{count} fit you and are not closed, as of {today}."),
            (Scope::All, _) => {
                format!("{count}, including those that don't Fit or are closed, as of {today}.")
            }
        }
    }

    /// The footer line about the latest Run.
    fn last_run_line(&self) -> String {
        match &self.last_run {
            None => "No Run yet.".to_owned(),
            Some(run) => {
                let at = format!(
                    "{} at {} UTC",
                    alert::format_date(run.finished_at.date_naive()),
                    run.finished_at.format("%H:%M")
                );
                match &run.failure {
                    None => format!("Last Run: {at}, succeeded."),
                    Some(why) => format!("Last Run: {at}, failed: {why}"),
                }
            }
        }
    }

    /// Each listed Hackathon's details, labelled, in the order a report shows them.
    fn details(&self, listed: &Listed) -> Vec<(&'static str, String)> {
        let mut details = Vec::new();
        if let Some(why) = &listed.not_fitting {
            details.push(("Doesn't Fit", why.clone()));
        }
        if let Some(place) = listed.place() {
            details.push(("Where", place));
        }
        details.push(("When", listed.dates()));
        details.push(("Application", listed.application(self.today)));
        if let Some(getting_there) = listed.getting_there() {
            details.push(("Getting there", getting_there));
        }
        if let Some(funding) = listed.funding() {
            details.push(("Funding", funding));
        }
        details
    }
}

/// The link when it is safe to write as one: http or https, with no space or
/// control character. Anything else is shown as text.
pub fn web_link(link: &str) -> Option<&str> {
    let lower = link.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://"))
        && !link.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then_some(link)
}

/// The Google Flights search for a flight Trip's Fare dates, from Home's first
/// airport, or `None` when the Trip isn't a flight with an airport or the
/// dates are a submission period.
pub fn google_flights(profile: &Profile, listed: &Listed) -> Option<String> {
    let trip = listed
        .trip
        .as_ref()
        .filter(|t| t.kind == TripKind::Flight)?;
    let airport = trip.airport.as_deref()?;
    if listed.hackathon.dates() == Dates::SubmissionPeriod {
        return None;
    }
    let query = fares::FareQuery::for_hackathon(profile, &listed.hackathon, airport);
    let words = format!(
        "Flights to {} from {} on {} through {}",
        query.to, profile.home.airports[0], query.outbound, query.inbound
    );
    Some(format!(
        "https://www.google.com/travel/flights?q={}",
        percent_encode(&words)
    ))
}

/// Percent-encodes everything but unreserved characters (RFC 3986).
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
