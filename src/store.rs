//! The local SQLite database, the source of truth for everything a Run learns (ADR 0003).

use std::path::Path;

use anyhow::Context;
use chrono::NaiveDate;
use rusqlite::{Connection, params};

use crate::candidate::Candidate;

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
}
