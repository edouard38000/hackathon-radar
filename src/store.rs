//! The local SQLite database, the source of truth for everything a Run learns (ADR 0003).

use std::path::Path;

use anyhow::Context;
use chrono::NaiveDate;
use rusqlite::{Connection, params};

use crate::agent::{INSTRUCTIONS_VERSION, Judgement, TravelFunded};
use crate::candidate::Candidate;

mod travel;

/// Applied in order; `PRAGMA user_version` records how many have run.
/// Later changes append a migration and never edit an earlier one.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE hackathons (
        id          INTEGER PRIMARY KEY,
        source      TEXT NOT NULL,
        source_key  TEXT NOT NULL,
        name        TEXT NOT NULL,
        start_date  TEXT NOT NULL,
        end_date    TEXT NOT NULL,
        city        TEXT,
        location    TEXT,
        country     TEXT,
        link        TEXT,
        first_seen  TEXT NOT NULL,
        last_seen   TEXT NOT NULL,
        UNIQUE (source, source_key)
    );",
    // The day an Alert first told the person about the Hackathon; NULL until then.
    "ALTER TABLE hackathons ADD COLUMN announced_on TEXT;",
    // The Agent's judgements, one per listing, and the Candidates still waiting for one.
    "CREATE TABLE judgements (
        source                  TEXT NOT NULL,
        source_key              TEXT NOT NULL,
        name                    TEXT NOT NULL,
        is_hackathon            INTEGER NOT NULL,
        eligible                INTEGER NOT NULL,
        eligibility_restriction TEXT,
        travel_funded           TEXT NOT NULL CHECK (travel_funded IN ('none', 'partial', 'full')),
        travel_detail           TEXT,
        reason                  TEXT NOT NULL,
        agent                   TEXT NOT NULL,
        instructions_version    TEXT NOT NULL,
        judged_on               TEXT NOT NULL,
        PRIMARY KEY (source, source_key)
    );
    CREATE TABLE unjudged (
        source       TEXT NOT NULL,
        source_key   TEXT NOT NULL,
        name         TEXT NOT NULL,
        start_date   TEXT NOT NULL,
        end_date     TEXT NOT NULL,
        city         TEXT,
        location     TEXT,
        country      TEXT,
        link         TEXT,
        first_queued TEXT NOT NULL,
        attempts     INTEGER NOT NULL DEFAULT 0,
        last_error   TEXT,
        PRIMARY KEY (source, source_key)
    );",
    // Getting there (ADR 0002): the Agent's travel estimates, cached per Home city
    // and destination city (names lower-cased; NULL train_minutes or airport means
    // no practical train or flight), and the Trip worked out for each Hackathon.
    "CREATE TABLE travel_estimates (
        home_city            TEXT NOT NULL,
        city                 TEXT NOT NULL,
        country              TEXT NOT NULL,
        train_minutes        INTEGER CHECK (train_minutes > 0),
        airport              TEXT,
        agent                TEXT NOT NULL,
        instructions_version TEXT NOT NULL,
        estimated_on         TEXT NOT NULL,
        PRIMARY KEY (home_city, city, country)
    );
    CREATE TABLE trips (
        source        TEXT NOT NULL,
        source_key    TEXT NOT NULL,
        kind          TEXT NOT NULL CHECK (kind IN ('local', 'train', 'flight')),
        train_minutes INTEGER,
        airport       TEXT,
        label         TEXT NOT NULL,
        PRIMARY KEY (source, source_key)
    );",
];

/// A Hackathon as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hackathon {
    pub source: String,
    pub source_key: String,
    pub name: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub city: Option<String>,
    pub location: Option<String>,
    pub country: Option<String>,
    pub link: Option<String>,
    /// The day of the first Run that stored it.
    pub first_seen: NaiveDate,
    /// The day of the latest Run that still found it.
    pub last_seen: NaiveDate,
    /// The day an Alert announced it, if one has.
    pub announced_on: Option<NaiveDate>,
}

/// A judgement as stored, with the listing it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredJudgement {
    pub source: String,
    pub source_key: String,
    pub name: String,
    pub judgement: Judgement,
    /// The Agent that judged it, such as `claude-code`.
    pub agent: String,
    /// The hackathon-radar version whose instructions the Agent ran with.
    pub instructions_version: String,
    pub judged_on: NaiveDate,
}

/// A Candidate that passed the data-only rules and is waiting for the Agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unjudged {
    pub candidate: Candidate,
    /// The day of the first Run that queued it.
    pub first_queued: NaiveDate,
    /// How many times the Agent was asked about it and failed.
    pub attempts: u32,
    pub last_error: Option<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens the database at `path`, creating it and its directory if needed.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating the directory {}", dir.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening the database at {}", path.display()))?;
        Self::migrated(conn)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::migrated(Connection::open_in_memory()?)
    }

    fn migrated(mut conn: Connection) -> anyhow::Result<Self> {
        let applied: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let applied = usize::try_from(applied)?;
        if applied > MIGRATIONS.len() {
            anyhow::bail!(
                "the database was written by a newer hackathon-radar (schema {applied}); upgrade to use it"
            );
        }
        let tx = conn.transaction()?;
        for migration in &MIGRATIONS[applied..] {
            tx.execute_batch(migration)?;
        }
        tx.pragma_update(None, "user_version", i64::try_from(MIGRATIONS.len())?)?;
        tx.commit()?;
        Ok(Self { conn })
    }

    /// Stores each Hackathon, updating the one already stored from the same listing.
    pub fn save_hackathons(
        &mut self,
        found: &[Candidate],
        seen_on: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT INTO hackathons
                    (source, source_key, name, start_date, end_date, city, location, country, link,
                     first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
                 ON CONFLICT (source, source_key) DO UPDATE SET
                    name = excluded.name,
                    start_date = excluded.start_date,
                    end_date = excluded.end_date,
                    city = excluded.city,
                    location = excluded.location,
                    country = excluded.country,
                    link = excluded.link,
                    last_seen = excluded.last_seen",
            )?;
            for c in found {
                upsert.execute(params![
                    c.source,
                    c.source_key,
                    c.name,
                    c.start,
                    c.end,
                    c.city,
                    c.location,
                    c.country,
                    c.link,
                    seen_on,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Every stored Hackathon, soonest first.
    pub fn hackathons(&self) -> anyhow::Result<Vec<Hackathon>> {
        let mut query = self.conn.prepare(
            "SELECT source, source_key, name, start_date, end_date, city, location, country, link,
                    first_seen, last_seen, announced_on
             FROM hackathons
             ORDER BY start_date, name, source, source_key",
        )?;
        let rows = query.query_map([], |row| {
            Ok(Hackathon {
                source: row.get(0)?,
                source_key: row.get(1)?,
                name: row.get(2)?,
                start: row.get(3)?,
                end: row.get(4)?,
                city: row.get(5)?,
                location: row.get(6)?,
                country: row.get(7)?,
                link: row.get(8)?,
                first_seen: row.get(9)?,
                last_seen: row.get(10)?,
                announced_on: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Whether any Alert has announced anything yet; until one has, the next Alert is the Welcome Alert.
    pub fn has_announced_anything(&self) -> anyhow::Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM hackathons WHERE announced_on IS NOT NULL)",
            [],
            |row| row.get(0),
        )?)
    }

    /// The ones among `found` that no Alert has announced, in the same order.
    /// A listing that isn't stored counts as not announced.
    pub fn not_announced(&self, found: &[Candidate]) -> anyhow::Result<Vec<Candidate>> {
        let mut announced = self.conn.prepare(
            "SELECT EXISTS (
                SELECT 1 FROM hackathons
                WHERE source = ?1 AND source_key = ?2 AND announced_on IS NOT NULL
             )",
        )?;
        let mut fresh = Vec::new();
        for c in found {
            let is_announced: bool =
                announced.query_row(params![c.source, c.source_key], |row| row.get(0))?;
            if !is_announced {
                fresh.push(c.clone());
            }
        }
        Ok(fresh)
    }

    /// Marks each stored Hackathon as Announced on `day`, keeping the day of an earlier Alert.
    pub fn mark_announced(
        &mut self,
        hackathons: &[Candidate],
        day: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut mark = tx.prepare(
                "UPDATE hackathons SET announced_on = ?3
                 WHERE source = ?1 AND source_key = ?2 AND announced_on IS NULL",
            )?;
            for c in hackathons {
                mark.execute(params![c.source, c.source_key, day])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Stores the Agent's judgements, replacing any earlier one of the same listing.
    pub fn save_judgements(
        &mut self,
        agent: &str,
        judged: &[(&Candidate, Judgement)],
        judged_on: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT OR REPLACE INTO judgements
                    (source, source_key, name, is_hackathon, eligible, eligibility_restriction,
                     travel_funded, travel_detail, reason, agent, instructions_version, judged_on)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for (c, j) in judged {
                upsert.execute(params![
                    c.source,
                    c.source_key,
                    c.name,
                    j.is_hackathon,
                    j.eligible,
                    j.eligibility_restriction,
                    j.travel_funded.as_str(),
                    j.travel_detail,
                    j.reason,
                    agent,
                    INSTRUCTIONS_VERSION,
                    judged_on,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The judgement of one listing, if the Agent has made one.
    pub fn judgement(&self, source: &str, source_key: &str) -> anyhow::Result<Option<Judgement>> {
        let mut query = self.conn.prepare_cached(
            "SELECT is_hackathon, eligible, eligibility_restriction, travel_funded, travel_detail,
                    reason
             FROM judgements WHERE source = ?1 AND source_key = ?2",
        )?;
        let mut rows =
            query.query_map(params![source, source_key], |row| judgement_from(row, 0))?;
        Ok(rows.next().transpose()?)
    }

    /// Every stored judgement, by listing name.
    pub fn judgements(&self) -> anyhow::Result<Vec<StoredJudgement>> {
        let mut query = self.conn.prepare(
            "SELECT is_hackathon, eligible, eligibility_restriction, travel_funded, travel_detail,
                    reason, source, source_key, name, agent, instructions_version, judged_on
             FROM judgements
             ORDER BY name, source, source_key",
        )?;
        let rows = query.query_map([], |row| {
            Ok(StoredJudgement {
                judgement: judgement_from(row, 0)?,
                source: row.get(6)?,
                source_key: row.get(7)?,
                name: row.get(8)?,
                agent: row.get(9)?,
                instructions_version: row.get(10)?,
                judged_on: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Queues Candidates for the Agent, refreshing the details of those already queued.
    pub fn queue_unjudged(
        &mut self,
        waiting: &[Candidate],
        today: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT INTO unjudged
                    (source, source_key, name, start_date, end_date, city, location, country, link,
                     first_queued)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (source, source_key) DO UPDATE SET
                    name = excluded.name,
                    start_date = excluded.start_date,
                    end_date = excluded.end_date,
                    city = excluded.city,
                    location = excluded.location,
                    country = excluded.country,
                    link = excluded.link",
            )?;
            for c in waiting {
                upsert.execute(params![
                    c.source,
                    c.source_key,
                    c.name,
                    c.start,
                    c.end,
                    c.city,
                    c.location,
                    c.country,
                    c.link,
                    today,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Records that the Agent was asked about these Candidates and failed.
    pub fn note_failed_judging(&mut self, asked: &[Candidate], error: &str) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut update = tx.prepare(
                "UPDATE unjudged SET attempts = attempts + 1, last_error = ?3
                 WHERE source = ?1 AND source_key = ?2",
            )?;
            for c in asked {
                update.execute(params![c.source, c.source_key, error])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Takes Candidates off the queue, once judged or no longer passing the rules.
    pub fn remove_unjudged<'c>(
        &mut self,
        done: impl IntoIterator<Item = &'c Candidate>,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut delete =
                tx.prepare("DELETE FROM unjudged WHERE source = ?1 AND source_key = ?2")?;
            for c in done {
                delete.execute(params![c.source, c.source_key])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The Candidates waiting for the Agent, soonest first.
    pub fn unjudged(&self) -> anyhow::Result<Vec<Unjudged>> {
        let mut query = self.conn.prepare(
            "SELECT source, source_key, name, start_date, end_date, city, location, country, link,
                    first_queued, attempts, last_error
             FROM unjudged
             ORDER BY start_date, name, source, source_key",
        )?;
        let rows = query.query_map([], |row| {
            Ok(Unjudged {
                candidate: Candidate {
                    source: row.get(0)?,
                    source_key: row.get(1)?,
                    name: row.get(2)?,
                    start: row.get(3)?,
                    end: row.get(4)?,
                    city: row.get(5)?,
                    location: row.get(6)?,
                    country: row.get(7)?,
                    link: row.get(8)?,
                },
                first_queued: row.get(9)?,
                attempts: row.get(10)?,
                last_error: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// Reads the six judgement columns starting at `first`.
fn judgement_from(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Judgement> {
    let travel: String = row.get(first + 3)?;
    let travel_funded = TravelFunded::parse(&travel).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            first + 3,
            rusqlite::types::Type::Text,
            format!("unknown travel_funded value {travel:?}").into(),
        )
    })?;
    Ok(Judgement {
        is_hackathon: row.get(first)?,
        eligible: row.get(first + 1)?,
        eligibility_restriction: row.get(first + 2)?,
        travel_funded,
        travel_detail: row.get(first + 4)?,
        reason: row.get(first + 5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_database_from_before_announced_flags_is_upgraded_in_place() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("old.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO hackathons (source, source_key, name, start_date, end_date,
                                         first_seen, last_seen)
                 VALUES ('s', 'k', 'Old Hack', '2026-12-01', '2026-12-02',
                         '2026-11-01', '2026-11-01')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        let stored = store.hackathons().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].announced_on, None);
        assert!(!store.has_announced_anything().unwrap());
    }

    #[test]
    fn a_database_at_schema_2_gains_the_judging_tables() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("schema-2.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.execute_batch(MIGRATIONS[1]).unwrap();
            conn.pragma_update(None, "user_version", 2).unwrap();
            conn.execute(
                "INSERT INTO hackathons (source, source_key, name, start_date, end_date,
                                         first_seen, last_seen, announced_on)
                 VALUES ('s', 'k', 'Old Hack', '2026-12-01', '2026-12-02',
                         '2026-11-01', '2026-11-01', '2026-11-01')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        let version: i64 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::try_from(MIGRATIONS.len()).unwrap());
        assert_eq!(
            store.hackathons().unwrap()[0].announced_on,
            Some(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap())
        );
        assert!(store.judgements().unwrap().is_empty());
        assert!(store.unjudged().unwrap().is_empty());
        assert!(store.judgement("s", "k").unwrap().is_none());
    }
}
