//! Deadlines, application status and Travel-funded news (ticket 09).
//!
//! The Agent's judgement carries each Hackathon's application deadline and
//! status. On each Run, [`recheck`] closes every Hackathon whose deadline has
//! passed and asks the Agent again about a few known Hackathons, through the
//! same read-only request as judging ([`JudgeRequest::recheck`]), so the
//! deadline, status and Travel-funded status stay current. Each change is stored.
//!
//! [`due`] decides what an Alert owes the person from what is stored alone:
//! Deadline reminders and known Hackathons that newly became Travel-funded.
//! It takes the store and today's date, never this Run's judgements, so
//! reminders still go out on a Run whose Agent failed. No Alert lists a
//! closed Hackathon ([`entries`]).
//!
//! The recheck policy, set in the Profile's `[deadlines]` section:
//! - a Hackathon is rechecked while it is known (judged a Hackathon the person
//!   is eligible for), passes the data-only rules, Fits or would Fit if
//!   Travel-funded (outside Countries under the Travel-funded rule), Fits the
//!   Themes (so one judged before the Themes were set is rechecked, and gets
//!   its Theme answer, while an off-Theme one isn't), and starts within
//!   `recheck_horizon_days`;
//! - never one judged or rechecked fewer than `recheck_every_days` ago;
//! - nearest deadline first (unknown deadlines last), then nearest start date;
//! - a closed one only while it could still reopen (a waitlist, a second
//!   round, or a mistaken "closed"): its deadline is unknown or hasn't passed
//!   and it hasn't started. It waits twice `recheck_every_days`, and closed ones
//!   only fill the batch after every due open one, soonest start first, at most
//!   a quarter of `rechecks_per_run` (rounded up). One found open again is
//!   newly Fitting, so the Alert announces it ([`entries`]);
//! - at most `rechecks_per_run` per Run, in one request held to `agent.max_turns`;
//! - not at all on a Run whose Agent already failed to judge.

use std::collections::HashMap;

use chrono::{Days, NaiveDate};

use crate::agent::{self, Agent, ApplicationStatus, JudgeRequest, Judgement};
use crate::alert::Entry;
use crate::candidate::Candidate;
use crate::profile::Profile;
use crate::rules;
use crate::store::{ListingKey, Store, key_of};

/// A closed Hackathon waits this many times `recheck_every_days` between rechecks.
const CLOSED_RECHECK_SLOWDOWN: u64 = 2;

/// The most closed Hackathons one Run rechecks: a quarter of
/// `rechecks_per_run`, rounded up, so at least one when rechecks are on.
fn closed_share(rechecks_per_run: usize) -> usize {
    rechecks_per_run.div_ceil(4)
}

/// A known Hackathon: one the Agent has judged, with its latest judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub hackathon: Candidate,
    pub judgement: Judgement,
    /// The day of its latest judgement or recheck.
    pub judged_on: NaiveDate,
}

/// What this Run's rechecks did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rechecked {
    /// How many known Hackathons the Agent rechecked.
    pub rechecked: usize,
    /// How many are due for a recheck but wait for a later Run.
    pub waiting: usize,
    /// Why the Agent's recheck failed, if it did; those Hackathons are due again next Run.
    pub failure: Option<String>,
    /// How many of the recheck's Theme answers were normalised rather than
    /// taken as given ([`agent::ReadAnswer`]).
    pub themes_normalised: usize,
}

/// Stores `closed` for every passed deadline, then rechecks the known
/// Hackathons that are due, unless `ask_agent` is false.
pub fn recheck(
    agent: &dyn Agent,
    profile: &Profile,
    store: &mut Store,
    today: NaiveDate,
    ask_agent: bool,
) -> anyhow::Result<Rechecked> {
    store.close_passed_deadlines(today)?;
    let settings = &profile.deadlines;
    if !ask_agent || settings.rechecks_per_run == 0 {
        return Ok(Rechecked::default());
    }

    let horizon = today + Days::new(settings.recheck_horizon_days.into());
    let every = u64::from(settings.recheck_every_days);
    let checked_before = today - Days::new(every);
    let closed_checked_before = today - Days::new(every * CLOSED_RECHECK_SLOWDOWN);
    let (mut closed, mut open): (Vec<Known>, Vec<Known>) = known(store)?
        .into_iter()
        .filter(|k| {
            k.judgement.fits()
                && rules::fits_themes(&k.judgement, profile)
                && !rules::apply(vec![k.hackathon.clone()], profile, today).is_empty()
                && k.hackathon.start <= horizon
        })
        .partition(|k| k.judgement.status_on(today) == ApplicationStatus::Closed);
    open.retain(|k| k.judged_on <= checked_before);
    open.sort_by_key(|k| {
        (
            k.judgement.deadline.is_none(),
            k.judgement.deadline,
            k.hackathon.start,
            k.hackathon.name.clone(),
        )
    });
    // A closed Hackathon may reopen only while its deadline hasn't passed and
    // it hasn't started.
    closed.retain(|k| {
        k.judgement.deadline.is_none_or(|d| d >= today)
            && k.hackathon.start > today
            && k.judged_on <= closed_checked_before
    });
    closed.sort_by_key(|k| (k.hackathon.start, k.hackathon.name.clone()));

    // Open ones first; closed ones only take what's left, up to their share.
    let closed_room = settings
        .rechecks_per_run
        .saturating_sub(open.len())
        .min(closed_share(settings.rechecks_per_run));
    let batch: Vec<Candidate> = open
        .iter()
        .take(settings.rechecks_per_run)
        .chain(closed.iter().take(closed_room))
        .map(|k| k.hackathon.clone())
        .collect();
    let due = open.len() + closed.len();
    if batch.is_empty() {
        return Ok(Rechecked::default());
    }

    let ids = agent::numbered_ids(batch.len());
    let request = JudgeRequest::recheck(&batch, &ids, profile);
    let answer = agent
        .judge(&request)
        .map_err(|e| format!("{e:#}"))
        .and_then(|answer| request.read_answer(&answer).map_err(|e| e.to_string()));
    let (judged, themes_normalised) = match answer {
        Ok(read) => (read.judgements, read.themes_normalised),
        Err(error) => {
            return Ok(Rechecked {
                rechecked: 0,
                waiting: due,
                failure: Some(error),
                themes_normalised: 0,
            });
        }
    };
    store.save_rechecks(agent.name(), &judged, today)?;

    // A Hackathon outside Countries that now Fits, being Travel-funded, is stored as one.
    let now_fitting: Vec<Candidate> = judged
        .iter()
        .filter(|(c, j)| rules::fits(c, j, profile, today))
        .map(|(c, _)| (*c).clone())
        .collect();
    let mut unstored = Vec::new();
    for c in now_fitting {
        if !store.has_hackathon(&c.source, &c.source_key)? {
            unstored.push(c);
        }
    }
    store.save_hackathons(&unstored, today)?;

    Ok(Rechecked {
        rechecked: judged.len(),
        waiting: due - judged.len(),
        failure: None,
        themes_normalised,
    })
}

/// Every known Hackathon, by the details stored for it as a Hackathon, or else
/// by the listing that carries its identity.
pub fn known(store: &Store) -> anyhow::Result<Vec<Known>> {
    let mut details: HashMap<ListingKey, Candidate> = store
        .listings()?
        .into_iter()
        .filter(|l| l.hackathon == l.key())
        .map(|l| (l.key(), l.candidate))
        .collect();
    for h in store.hackathons()? {
        let candidate = Candidate {
            source: h.source,
            source_key: h.source_key,
            name: h.name,
            start: h.start,
            end: h.end,
            city: h.city,
            location: h.location,
            country: h.country,
            link: h.link,
        };
        details.insert(key_of(&candidate), candidate);
    }
    Ok(store
        .judgements()?
        .into_iter()
        .filter_map(|j| {
            let hackathon = details.remove(&(j.source, j.source_key))?;
            Some(Known {
                hackathon,
                judgement: j.judgement,
                judged_on: j.judged_on,
            })
        })
        .collect())
}

/// What an Alert owes the person, from what is stored.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Due {
    /// Open Hackathons that Fit whose deadline is at most the Profile's
    /// `reminder_days` away and whose reminder for that deadline hasn't gone out.
    pub reminders: Vec<Known>,
    /// Known Hackathons that became Travel-funded, Fit, and aren't closed,
    /// that no Alert has announced as such yet.
    pub newly_funded: Vec<Known>,
}

/// Decides what is due from the store and today's date alone, whatever this
/// Run's Agent did. A Deadline reminder is due once per Hackathon and deadline,
/// on the first Run within `reminder_days` of it; a deadline that moves gets its
/// own reminder.
pub fn due(store: &Store, profile: &Profile, today: NaiveDate) -> anyhow::Result<Due> {
    let reminder_days = i64::from(profile.deadlines.reminder_days);
    let funded: Vec<ListingKey> = store
        .newly_funded()?
        .into_iter()
        .map(|n| n.hackathon)
        .collect();
    let mut due = Due::default();
    for k in known(store)? {
        let status = k.judgement.status_on(today);
        // Nothing about a closed Hackathon is due; see [`entries`].
        if status == ApplicationStatus::Closed
            || !rules::fits(&k.hackathon, &k.judgement, profile, today)
        {
            continue;
        }
        if let Some(deadline) = k.judgement.deadline {
            let days_left = (deadline - today).num_days();
            if status.is_open()
                && (0..=reminder_days).contains(&days_left)
                && store
                    .reminded_on(&k.hackathon.source, &k.hackathon.source_key, deadline)?
                    .is_none()
            {
                due.reminders.push(k.clone());
            }
        }
        if funded.contains(&key_of(&k.hackathon)) {
            due.newly_funded.push(k);
        }
    }
    Ok(due)
}

/// The Alert's entries: `new`, the Hackathons that newly Fit, and what is
/// [`due`]; one entry per Hackathon, whatever brings it into the Alert.
///
/// A closed Hackathon gets no entry: the person can't apply, so it isn't
/// listed, counted or Announced. It stays stored, and once a recheck finds it
/// open again it is newly Fitting like any other Hackathon no Alert has
/// announced.
pub fn entries(
    store: &Store,
    profile: &Profile,
    new: &[Candidate],
    today: NaiveDate,
) -> anyhow::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = Vec::new();
    for c in new {
        let mut entry = Entry::new_fit(c.clone());
        if let Some(judgement) = store.judgement(&c.source, &c.source_key)? {
            if judgement.status_on(today) == ApplicationStatus::Closed {
                continue;
            }
            describe(&mut entry, &judgement, profile, today);
        }
        entry.trip = store.trip(&c.source, &c.source_key)?.map(|t| t.label);
        entries.push(entry);
    }
    let due = due(store, profile, today)?;
    let mut entry_for = |k: &Known| -> anyhow::Result<usize> {
        let key = key_of(&k.hackathon);
        if let Some(at) = entries.iter().position(|e| key_of(&e.hackathon) == key) {
            return Ok(at);
        }
        let mut entry = Entry {
            new: false,
            ..Entry::new_fit(k.hackathon.clone())
        };
        describe(&mut entry, &k.judgement, profile, today);
        entry.trip = store.trip(&key.0, &key.1)?.map(|t| t.label);
        entries.push(entry);
        Ok(entries.len() - 1)
    };
    let mut reminded = Vec::new();
    for k in &due.reminders {
        reminded.push(entry_for(k)?);
    }
    let mut funded = Vec::new();
    for k in &due.newly_funded {
        funded.push(entry_for(k)?);
    }
    for (at, k) in reminded.into_iter().zip(&due.reminders) {
        let deadline = k.judgement.deadline.expect("a due reminder has a deadline");
        entries[at].reminder_days_left = Some((deadline - today).num_days());
    }
    for at in funded {
        entries[at].newly_funded = true;
    }
    Ok(entries)
}

fn describe(entry: &mut Entry, judgement: &Judgement, profile: &Profile, today: NaiveDate) {
    entry.status = judgement.status_on(today);
    entry.deadline = judgement.deadline;
    entry.travel_funded = judgement.travel_funded;
    entry.travel_detail = judgement.travel_detail.clone();
    entry.theme = rules::matched_theme(judgement, profile).map(str::to_owned);
}

/// Keeps those of `fitting` that still Fit by their stored judgement, which a
/// recheck may have changed.
pub fn still_fitting(
    store: &Store,
    profile: &Profile,
    fitting: Vec<Candidate>,
    today: NaiveDate,
) -> anyhow::Result<Vec<Candidate>> {
    let mut kept = Vec::new();
    for c in fitting {
        if store
            .judgement(&c.source, &c.source_key)?
            .is_some_and(|j| rules::fits(&c, &j, profile, today))
        {
            kept.push(c);
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::TravelFunded;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn ghent(name: &str) -> Candidate {
        Candidate {
            source: "s".into(),
            source_key: name.into(),
            name: name.into(),
            start: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            city: Some("Ghent".into()),
            location: None,
            country: Some("Belgium".into()),
            link: None,
        }
    }

    fn judged(status: ApplicationStatus, deadline: Option<NaiveDate>) -> Judgement {
        Judgement {
            is_hackathon: true,
            eligible: true,
            eligibility_restriction: None,
            travel_funded: TravelFunded::None,
            travel_detail: None,
            deadline,
            status,
            reason: "Teams build software.".into(),
            theme: None,
        }
    }

    #[test]
    fn a_closed_hackathon_gets_no_entry_whatever_closed_it() {
        let profile = Profile::from_toml(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n",
            std::path::Path::new("test.toml"),
        )
        .unwrap();
        let (open, closed, passed) = (ghent("Open"), ghent("Closed"), ghent("Passed"));
        let mut store = Store::open_in_memory().unwrap();
        store
            .save_judgements(
                "fake",
                &[
                    (&open, judged(ApplicationStatus::Open, Some(date(30)))),
                    (&closed, judged(ApplicationStatus::Closed, None)),
                    // Open as judged, but its deadline has passed by today.
                    (&passed, judged(ApplicationStatus::Open, Some(date(10)))),
                ],
                date(9),
            )
            .unwrap();

        let listed = entries(&store, &profile, &[open, closed, passed], date(12)).unwrap();

        let names: Vec<&str> = listed.iter().map(|e| e.hackathon.name.as_str()).collect();
        assert_eq!(names, ["Open"]);
    }
}
