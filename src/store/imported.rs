//! Storing what `import-sheet` read from the personal tracker (ticket 17).
//!
//! An imported row is a listing like any other, from the `sheet-import`
//! Source. Everything else it carries (judgement, Trip, Fare, Announced day,
//! newly Travel-funded already sent) is stored for its Hackathon only where
//! that Hackathon has none yet, so importing into a store a Run already
//! filled never overwrites what the Run found, and a Hackathon already
//! Announced stays Announced on its own day.

use chrono::NaiveDate;
use rusqlite::params;

use super::{ListingKey, Store};
use crate::agent::{INSTRUCTIONS_VERSION, Judgement};
use crate::candidate::Candidate;
use crate::fares::FareQuery;
use crate::travel::Trip;

/// One row of the Sheet, ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedListing {
    pub listing: Candidate,
    /// The Hackathon it belongs to: its own key when it is new, or the
    /// Hackathon it is a Duplicate listing of.
    pub hackathon: ListingKey,
    pub first_seen: NaiveDate,
    pub last_seen: NaiveDate,
    /// Whether the Hackathon is stored as one that Fits (not ruled out in the Sheet).
    pub fits: bool,
    pub judgement: Judgement,
    /// Who made the judgement, such as `sheet-import`.
    pub judged_by: String,
    pub judged_on: NaiveDate,
    pub trip: Option<Trip>,
    pub fare: Option<ImportedFare>,
    /// The day an Alert told the person about it, if one did.
    pub announced_on: Option<NaiveDate>,
    /// The day an Alert told the person it became Travel-funded, if one did.
    pub funding_announced_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedFare {
    pub query: FareQuery,
    pub price: u32,
    /// The Home airport the cheapest flight leaves from, when known.
    pub from_airport: Option<String>,
    /// Looked up from flight prices; `false` for an Unverified fare.
    pub verified: bool,
    pub source: String,
    pub checked_on: NaiveDate,
}

impl Store {
    /// Stores imported rows in one transaction, in order, so a row can belong
    /// to a Hackathon an earlier row created.
    pub fn save_imported(&mut self, imported: &[ImportedListing]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        for row in imported {
            let c = &row.listing;
            let (h_source, h_key) = &row.hackathon;
            tx.execute(
                "INSERT OR IGNORE INTO listings
                    (source, source_key, name, start_date, end_date, city, location, country, link,
                     hackathon_source, hackathon_source_key, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    c.source,
                    c.source_key,
                    c.name,
                    c.start,
                    c.end,
                    c.city,
                    c.location,
                    c.country,
                    c.link,
                    h_source,
                    h_key,
                    row.first_seen,
                    row.last_seen,
                ],
            )?;
            if row.fits {
                // The Hackathon as its own listing describes it.
                tx.execute(
                    "INSERT OR IGNORE INTO hackathons
                        (source, source_key, name, start_date, end_date, city, location, country,
                         link, first_seen, last_seen)
                     SELECT source, source_key, name, start_date, end_date, city, location,
                            country, link, first_seen, last_seen
                     FROM listings WHERE source = ?1 AND source_key = ?2",
                    params![h_source, h_key],
                )?;
            }
            if let Some(day) = row.announced_on {
                tx.execute(
                    "UPDATE hackathons SET announced_on = ?3
                     WHERE source = ?1 AND source_key = ?2 AND announced_on IS NULL",
                    params![h_source, h_key, day],
                )?;
            }
            let j = &row.judgement;
            tx.execute(
                "INSERT OR IGNORE INTO judgements
                    (source, source_key, name, is_hackathon, eligible, eligibility_restriction,
                     travel_funded, travel_detail, reason, agent, instructions_version, judged_on,
                     application_deadline, application_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    h_source,
                    h_key,
                    c.name,
                    j.is_hackathon,
                    j.eligible,
                    j.eligibility_restriction,
                    j.travel_funded.as_str(),
                    j.travel_detail,
                    j.reason,
                    row.judged_by,
                    INSTRUCTIONS_VERSION,
                    row.judged_on,
                    j.deadline,
                    j.status.as_str(),
                ],
            )?;
            // Judged now, so no longer waiting for the Agent.
            tx.execute(
                "DELETE FROM unjudged WHERE source = ?1 AND source_key = ?2",
                params![h_source, h_key],
            )?;
            if let Some(trip) = &row.trip {
                tx.execute(
                    "INSERT OR IGNORE INTO trips
                        (source, source_key, kind, train_minutes, airport, label)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        h_source,
                        h_key,
                        trip.kind.as_str(),
                        trip.train_minutes,
                        trip.airport,
                        trip.label,
                    ],
                )?;
            }
            if let Some(fare) = &row.fare {
                let q = &fare.query;
                tx.execute(
                    "INSERT OR IGNORE INTO fares
                        (from_airports, to_airport, outbound_date, return_date, currency,
                         price, from_airport, verified, source, checked_on)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        q.from.join(","),
                        q.to,
                        q.outbound,
                        q.inbound,
                        q.currency,
                        fare.price,
                        fare.from_airport,
                        fare.verified,
                        fare.source,
                        fare.checked_on,
                    ],
                )?;
            }
            if let Some(day) = row.funding_announced_on {
                tx.execute(
                    "INSERT INTO newly_funded (source, source_key, funded_on, announced_on)
                     VALUES (?1, ?2, ?3, ?3)
                     ON CONFLICT (source, source_key) DO UPDATE SET
                        announced_on = COALESCE(newly_funded.announced_on, excluded.announced_on)",
                    params![h_source, h_key, day],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
