//! The local SQLite database, the source of truth for everything a Run learns (ADR 0003).

use std::path::Path;

use anyhow::Context;
use chrono::NaiveDate;
use rusqlite::{Connection, params};

use crate::agent::{ApplicationStatus, INSTRUCTIONS_VERSION, Judgement, TravelFunded};
use crate::candidate::Candidate;

mod deadlines;
mod fares;
mod imported;
mod runs;
mod travel;

pub use deadlines::{JudgementChange, NewlyFunded};
pub use imported::{ImportedFare, ImportedListing};
pub use runs::{FailedStreak, RunCounts, RunRecord};

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
    // Duplicate listings (ADR 0008): every listing a Run collected, and the
    // Hackathon it belongs to, named by the listing whose identity that
    // Hackathon carries (the listing itself unless it was merged into another).
    // Then the Agent's answers about pairs the rules couldn't settle, each pair
    // in sorted order. Hackathons stored before this migration become listings of themselves.
    "CREATE TABLE listings (
        source               TEXT NOT NULL,
        source_key           TEXT NOT NULL,
        name                 TEXT NOT NULL,
        start_date           TEXT NOT NULL,
        end_date             TEXT NOT NULL,
        city                 TEXT,
        location             TEXT,
        country              TEXT,
        link                 TEXT,
        hackathon_source     TEXT NOT NULL,
        hackathon_source_key TEXT NOT NULL,
        first_seen           TEXT NOT NULL,
        last_seen            TEXT NOT NULL,
        PRIMARY KEY (source, source_key)
    );
    CREATE INDEX listings_by_hackathon ON listings (hackathon_source, hackathon_source_key);
    CREATE TABLE same_hackathon_answers (
        first_source      TEXT NOT NULL,
        first_source_key  TEXT NOT NULL,
        second_source     TEXT NOT NULL,
        second_source_key TEXT NOT NULL,
        same              INTEGER NOT NULL,
        reason            TEXT NOT NULL,
        agent             TEXT NOT NULL,
        answered_on       TEXT NOT NULL,
        PRIMARY KEY (first_source, first_source_key, second_source, second_source_key)
    );
    INSERT INTO listings
        (source, source_key, name, start_date, end_date, city, location, country, link,
         hackathon_source, hackathon_source_key, first_seen, last_seen)
    SELECT source, source_key, name, start_date, end_date, city, location, country, link,
           source, source_key, first_seen, last_seen
    FROM hackathons;",
    // Run history (ticket 10): one row per Run that wasn't a dry run, with why
    // it failed (NULL when it succeeded), its counts, and whether its Alert
    // carried the needs-attention part to the person.
    "CREATE TABLE runs (
        id             INTEGER PRIMARY KEY,
        day            TEXT NOT NULL,
        started_at     TEXT NOT NULL,
        finished_at    TEXT NOT NULL,
        succeeded      INTEGER NOT NULL CHECK (succeeded IN (0, 1)),
        failure        TEXT,
        collected      INTEGER NOT NULL,
        judged         INTEGER NOT NULL,
        unjudged       INTEGER NOT NULL,
        fit            INTEGER NOT NULL,
        announced      INTEGER NOT NULL,
        sent           INTEGER NOT NULL CHECK (sent IN (0, 1)),
        failed_sources INTEGER NOT NULL,
        attention_sent INTEGER NOT NULL CHECK (attention_sent IN (0, 1)),
        CHECK ((succeeded = 1) = (failure IS NULL))
    );",
    // Deadlines and status (ticket 09): the Agent's latest application deadline
    // (NULL when none is given) and status, kept with its judgement; every change
    // a recheck or a passed deadline makes to them or to Travel-funded status;
    // the Deadline reminders sent, one per Hackathon and deadline; and the known
    // Hackathons that became Travel-funded, with the day an Alert announced it.
    // Reminders and newly Travel-funded are tracked apart from announced_on.
    "ALTER TABLE judgements ADD COLUMN application_deadline TEXT;
    ALTER TABLE judgements ADD COLUMN application_status TEXT NOT NULL DEFAULT 'unknown'
        CHECK (application_status IN
            ('open', 'open_until_full', 'not_open_yet', 'waitlist', 'closed', 'unknown'));
    CREATE TABLE judgement_changes (
        source      TEXT NOT NULL,
        source_key  TEXT NOT NULL,
        changed_on  TEXT NOT NULL,
        field       TEXT NOT NULL
            CHECK (field IN ('application_deadline', 'application_status', 'travel_funded')),
        old_value   TEXT,
        new_value   TEXT
    );
    CREATE INDEX judgement_changes_by_listing ON judgement_changes (source, source_key);
    CREATE TABLE deadline_reminders (
        source       TEXT NOT NULL,
        source_key   TEXT NOT NULL,
        deadline     TEXT NOT NULL,
        reminded_on  TEXT NOT NULL,
        PRIMARY KEY (source, source_key, deadline)
    );
    CREATE TABLE newly_funded (
        source        TEXT NOT NULL,
        source_key    TEXT NOT NULL,
        funded_on     TEXT NOT NULL,
        announced_on  TEXT,
        PRIMARY KEY (source, source_key)
    );",
    // Fares (ticket 08, ADR 0002): the cheapest return Fare per route, dates
    // and currency. from_airports is Home's airports joined by commas; a NULL
    // price means the lookup found no flight; verified is 0 for an Unverified
    // fare (estimated by the Agent) and 1 for one looked up through SerpApi.
    "CREATE TABLE fares (
        from_airports TEXT NOT NULL,
        to_airport    TEXT NOT NULL,
        outbound_date TEXT NOT NULL,
        return_date   TEXT NOT NULL,
        currency      TEXT NOT NULL,
        price         INTEGER CHECK (price > 0),
        from_airport  TEXT,
        verified      INTEGER NOT NULL CHECK (verified IN (0, 1)),
        source        TEXT NOT NULL,
        checked_on    TEXT NOT NULL,
        PRIMARY KEY (from_airports, to_airport, outbound_date, return_date, currency)
    );",
];

/// Names a listing: its Source and its key within that Source.
pub type ListingKey = (String, String);

/// A collected listing as stored, with the Hackathon it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub candidate: Candidate,
    /// The listing whose identity the Hackathon carries; the listing itself
    /// unless it is a Duplicate listing merged into another.
    pub hackathon: ListingKey,
    pub first_seen: NaiveDate,
    pub last_seen: NaiveDate,
}

impl Listing {
    pub fn key(&self) -> ListingKey {
        key_of(&self.candidate)
    }
}

pub fn key_of(candidate: &Candidate) -> ListingKey {
    (candidate.source.clone(), candidate.source_key.clone())
}

/// A pair of listings in the order they are stored in.
fn sorted_pair<'c>(a: &'c Candidate, b: &'c Candidate) -> (&'c Candidate, &'c Candidate) {
    if key_of(a) <= key_of(b) {
        (a, b)
    } else {
        (b, a)
    }
}

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

    /// Marks each stored Hackathon as Announced on `day`, keeping the day of an
    /// earlier Alert, and returns how many were newly Announced.
    pub fn mark_announced(
        &mut self,
        hackathons: &[Candidate],
        day: NaiveDate,
    ) -> anyhow::Result<usize> {
        let tx = self.conn.transaction()?;
        let mut newly = 0;
        {
            let mut mark = tx.prepare(
                "UPDATE hackathons SET announced_on = ?3
                 WHERE source = ?1 AND source_key = ?2 AND announced_on IS NULL",
            )?;
            for c in hackathons {
                newly += mark.execute(params![c.source, c.source_key, day])?;
            }
        }
        tx.commit()?;
        Ok(newly)
    }

    /// Stores the Agent's judgements, replacing any earlier one of the same listing.
    pub fn save_judgements(
        &mut self,
        agent: &str,
        judged: &[(&Candidate, Judgement)],
        judged_on: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        upsert_judgements(&tx, agent, judged, judged_on)?;
        tx.commit()?;
        Ok(())
    }

    /// The judgement of one listing, if the Agent has made one.
    pub fn judgement(&self, source: &str, source_key: &str) -> anyhow::Result<Option<Judgement>> {
        let mut query = self.conn.prepare_cached(
            "SELECT is_hackathon, eligible, eligibility_restriction, travel_funded, travel_detail,
                    reason, application_deadline, application_status
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
                    reason, application_deadline, application_status,
                    source, source_key, name, agent, instructions_version, judged_on
             FROM judgements
             ORDER BY name, source, source_key",
        )?;
        let rows = query.query_map([], |row| {
            Ok(StoredJudgement {
                judgement: judgement_from(row, 0)?,
                source: row.get(8)?,
                source_key: row.get(9)?,
                name: row.get(10)?,
                agent: row.get(11)?,
                instructions_version: row.get(12)?,
                judged_on: row.get(13)?,
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

impl Store {
    /// Every stored listing.
    pub fn listings(&self) -> anyhow::Result<Vec<Listing>> {
        let mut query = self.conn.prepare(
            "SELECT source, source_key, name, start_date, end_date, city, location, country, link,
                    hackathon_source, hackathon_source_key, first_seen, last_seen
             FROM listings
             ORDER BY start_date, source, source_key",
        )?;
        let rows = query.query_map([], |row| {
            Ok(Listing {
                candidate: candidate_from(row, 0)?,
                hackathon: (row.get(9)?, row.get(10)?),
                first_seen: row.get(11)?,
                last_seen: row.get(12)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The stored listings collected from one Source.
    pub fn listings_from(&self, source: &str) -> anyhow::Result<Vec<Candidate>> {
        Ok(self
            .listings()?
            .into_iter()
            .filter(|l| l.candidate.source == source)
            .map(|l| l.candidate)
            .collect())
    }

    /// The listings of one Hackathon, named by the listing whose identity it carries.
    pub fn listings_of(&self, source: &str, source_key: &str) -> anyhow::Result<Vec<Candidate>> {
        Ok(self
            .listings()?
            .into_iter()
            .filter(|l| l.hackathon.0 == source && l.hackathon.1 == source_key)
            .map(|l| l.candidate)
            .collect())
    }

    /// Stores each collected listing with the Hackathon it belongs to,
    /// updating the details of listings already stored.
    pub fn save_listings(
        &mut self,
        collected: &[(&Candidate, ListingKey)],
        seen_on: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT INTO listings
                    (source, source_key, name, start_date, end_date, city, location, country, link,
                     hackathon_source, hackathon_source_key, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
                 ON CONFLICT (source, source_key) DO UPDATE SET
                    name = excluded.name,
                    start_date = excluded.start_date,
                    end_date = excluded.end_date,
                    city = excluded.city,
                    location = excluded.location,
                    country = excluded.country,
                    link = excluded.link,
                    hackathon_source = excluded.hackathon_source,
                    hackathon_source_key = excluded.hackathon_source_key,
                    last_seen = excluded.last_seen",
            )?;
            for (c, (h_source, h_key)) in collected {
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
                    h_source,
                    h_key,
                    seen_on,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Merges the Hackathon `from` into the Hackathon `into`, once a Run finds
    /// they are the same: `from`'s listings move to `into`, and so do its stored
    /// row, Announced day, judgement and Trip where `into` has none. A Hackathon
    /// Announced as either stays Announced.
    pub fn merge_hackathon(&mut self, from: &ListingKey, into: &ListingKey) -> anyhow::Result<()> {
        if from == into {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE listings SET hackathon_source = ?3, hackathon_source_key = ?4
             WHERE hackathon_source = ?1 AND hackathon_source_key = ?2",
            params![from.0, from.1, into.0, into.1],
        )?;
        let into_exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM hackathons WHERE source = ?1 AND source_key = ?2)",
            params![into.0, into.1],
            |row| row.get(0),
        )?;
        if into_exists {
            tx.execute(
                "UPDATE hackathons SET announced_on = (
                    SELECT MIN(announced_on) FROM hackathons
                    WHERE (source = ?1 AND source_key = ?2) OR (source = ?3 AND source_key = ?4)
                 )
                 WHERE source = ?3 AND source_key = ?4",
                params![from.0, from.1, into.0, into.1],
            )?;
            tx.execute(
                "DELETE FROM hackathons WHERE source = ?1 AND source_key = ?2",
                params![from.0, from.1],
            )?;
        } else {
            tx.execute(
                "UPDATE hackathons SET source = ?3, source_key = ?4
                 WHERE source = ?1 AND source_key = ?2",
                params![from.0, from.1, into.0, into.1],
            )?;
        }
        tx.execute(
            "INSERT OR IGNORE INTO judgements
                (source, source_key, name, is_hackathon, eligible, eligibility_restriction,
                 travel_funded, travel_detail, reason, agent, instructions_version, judged_on,
                 application_deadline, application_status)
             SELECT ?3, ?4, name, is_hackathon, eligible, eligibility_restriction,
                    travel_funded, travel_detail, reason, agent, instructions_version, judged_on,
                    application_deadline, application_status
             FROM judgements WHERE source = ?1 AND source_key = ?2",
            params![from.0, from.1, into.0, into.1],
        )?;
        deadlines::merge(&tx, from, into)?;
        tx.execute(
            "INSERT OR IGNORE INTO trips
                (source, source_key, kind, train_minutes, airport, label)
             SELECT ?3, ?4, kind, train_minutes, airport, label
             FROM trips WHERE source = ?1 AND source_key = ?2",
            params![from.0, from.1, into.0, into.1],
        )?;
        tx.execute(
            "DELETE FROM trips WHERE source = ?1 AND source_key = ?2",
            params![from.0, from.1],
        )?;
        tx.execute(
            "DELETE FROM unjudged WHERE source = ?1 AND source_key = ?2",
            params![from.0, from.1],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The Agent's answer about whether two listings describe the same Hackathon, if it gave one.
    pub fn same_hackathon(&self, a: &Candidate, b: &Candidate) -> anyhow::Result<Option<bool>> {
        let (first, second) = sorted_pair(a, b);
        let mut query = self.conn.prepare_cached(
            "SELECT same FROM same_hackathon_answers
             WHERE first_source = ?1 AND first_source_key = ?2
               AND second_source = ?3 AND second_source_key = ?4",
        )?;
        let mut rows = query.query_map(
            params![
                first.source,
                first.source_key,
                second.source,
                second.source_key
            ],
            |row| row.get(0),
        )?;
        Ok(rows.next().transpose()?)
    }

    /// Stores the Agent's answers about pairs of listings, replacing earlier ones.
    pub fn save_same_hackathon_answers(
        &mut self,
        agent: &str,
        answers: &[(&Candidate, &Candidate, bool, &str)],
        answered_on: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT OR REPLACE INTO same_hackathon_answers
                    (first_source, first_source_key, second_source, second_source_key,
                     same, reason, agent, answered_on)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (a, b, same, reason) in answers {
                let (first, second) = sorted_pair(a, b);
                upsert.execute(params![
                    first.source,
                    first.source_key,
                    second.source,
                    second.source_key,
                    same,
                    reason,
                    agent,
                    answered_on,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

/// Reads the nine listing columns starting at `first`.
fn candidate_from(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Candidate> {
    Ok(Candidate {
        source: row.get(first)?,
        source_key: row.get(first + 1)?,
        name: row.get(first + 2)?,
        start: row.get(first + 3)?,
        end: row.get(first + 4)?,
        city: row.get(first + 5)?,
        location: row.get(first + 6)?,
        country: row.get(first + 7)?,
        link: row.get(first + 8)?,
    })
}

/// Stores judgements, replacing any earlier one of the same listing.
fn upsert_judgements(
    tx: &rusqlite::Transaction<'_>,
    agent: &str,
    judged: &[(&Candidate, Judgement)],
    judged_on: NaiveDate,
) -> anyhow::Result<()> {
    let mut upsert = tx.prepare(
        "INSERT OR REPLACE INTO judgements
            (source, source_key, name, is_hackathon, eligible, eligibility_restriction,
             travel_funded, travel_detail, reason, agent, instructions_version, judged_on,
             application_deadline, application_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
            j.deadline,
            j.status.as_str(),
        ])?;
    }
    Ok(())
}

/// Reads the eight judgement columns starting at `first`: is_hackathon,
/// eligible, eligibility_restriction, travel_funded, travel_detail, reason,
/// application_deadline and application_status.
fn judgement_from(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Judgement> {
    let unknown = |column: usize, what: &str, value: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            format!("unknown {what} value {value:?}").into(),
        )
    };
    let travel: String = row.get(first + 3)?;
    let travel_funded =
        TravelFunded::parse(&travel).ok_or_else(|| unknown(first + 3, "travel_funded", &travel))?;
    let status: String = row.get(first + 7)?;
    let status = ApplicationStatus::parse(&status)
        .ok_or_else(|| unknown(first + 7, "application_status", &status))?;
    Ok(Judgement {
        is_hackathon: row.get(first)?,
        eligible: row.get(first + 1)?,
        eligibility_restriction: row.get(first + 2)?,
        travel_funded,
        travel_detail: row.get(first + 4)?,
        reason: row.get(first + 5)?,
        deadline: row.get(first + 6)?,
        status,
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

    #[test]
    fn a_database_at_schema_3_keeps_its_hackathons_as_their_own_listings() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("schema-3.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..3] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", 3).unwrap();
            conn.execute(
                "INSERT INTO hackathons (source, source_key, name, start_date, end_date,
                                         first_seen, last_seen, announced_on)
                 VALUES ('s', 'k', 'Old Hack', '2026-12-01', '2026-12-02',
                         '2026-11-01', '2026-11-03', '2026-11-01')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        let listings = store.listings().unwrap();
        assert_eq!(listings.len(), 1);
        assert_eq!(listings[0].candidate.name, "Old Hack");
        assert_eq!(listings[0].hackathon, ("s".to_owned(), "k".to_owned()));
        assert_eq!(
            (listings[0].first_seen, listings[0].last_seen),
            (
                NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
            )
        );
        assert_eq!(store.listings_of("s", "k").unwrap().len(), 1);
    }

    #[test]
    fn merging_a_hackathon_keeps_it_announced_and_judged() {
        let mut store = Store::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let listing = |source: &str| Candidate {
            source: source.into(),
            source_key: "k".into(),
            name: "Hack".into(),
            start: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 12, 2).unwrap(),
            city: None,
            location: None,
            country: None,
            link: None,
        };
        let (a, b) = (listing("a"), listing("b"));
        store
            .save_listings(&[(&a, key_of(&a)), (&b, key_of(&b))], day)
            .unwrap();
        store
            .save_hackathons(std::slice::from_ref(&a), day)
            .unwrap();
        store.mark_announced(std::slice::from_ref(&a), day).unwrap();
        let judgement = Judgement {
            is_hackathon: true,
            eligible: true,
            eligibility_restriction: None,
            travel_funded: TravelFunded::None,
            travel_detail: None,
            deadline: NaiveDate::from_ymd_opt(2026, 11, 20),
            status: ApplicationStatus::Open,
            reason: "Builds.".into(),
        };
        store
            .save_judgements("fake", &[(&a, judgement.clone())], day)
            .unwrap();
        let trip = crate::travel::Trip {
            kind: crate::travel::TripKind::Train,
            train_minutes: Some(120),
            airport: None,
            label: "train ~2h".into(),
        };
        store.save_trips(&[(&a, Some(trip.clone()))]).unwrap();

        store.merge_hackathon(&key_of(&a), &key_of(&b)).unwrap();

        assert_eq!(store.trip("b", "k").unwrap(), Some(trip));
        assert_eq!(store.trip("a", "k").unwrap(), None);

        let hackathons = store.hackathons().unwrap();
        assert_eq!(hackathons.len(), 1);
        assert_eq!(
            (hackathons[0].source.as_str(), hackathons[0].announced_on),
            ("b", Some(day))
        );
        assert_eq!(store.judgement("b", "k").unwrap(), Some(judgement));
        assert_eq!(store.listings_of("b", "k").unwrap().len(), 2);
        assert!(
            store
                .not_announced(std::slice::from_ref(&b))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_database_at_schema_4_gains_listings_and_keeps_its_travel_rows() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("schema-4.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..4] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", 4).unwrap();
            conn.execute_batch(
                "INSERT INTO hackathons (source, source_key, name, start_date, end_date,
                                         city, country, first_seen, last_seen, announced_on)
                 VALUES ('s', 'k', 'Munich Hack', '2026-12-01', '2026-12-02',
                         'Munich', 'Germany', '2026-11-01', '2026-11-03', '2026-11-01');
                 INSERT INTO travel_estimates (home_city, city, country, train_minutes, airport,
                                               agent, instructions_version, estimated_on)
                 VALUES ('brussels', 'munich', 'germany', 384, 'MUC', 'fake', '0.1.0', '2026-11-01');
                 INSERT INTO trips (source, source_key, kind, train_minutes, airport, label)
                 VALUES ('s', 'k', 'flight', 384, 'MUC', 'fly BRU→MUC');",
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
            store.trip("s", "k").unwrap().map(|t| t.label),
            Some("fly BRU→MUC".to_owned())
        );
        let munich = crate::agent::travel::Destination {
            city: "Munich".into(),
            country: "Germany".into(),
        };
        let estimate = store.travel_estimate("Brussels", &munich).unwrap().unwrap();
        assert_eq!(
            (estimate.train_minutes, estimate.airport.as_deref()),
            (Some(384), Some("MUC"))
        );
        let listings = store.listings().unwrap();
        assert_eq!(listings.len(), 1);
        assert_eq!(listings[0].hackathon, ("s".to_owned(), "k".to_owned()));
        assert!(store.hackathons().unwrap()[0].announced_on.is_some());
    }

    #[test]
    fn a_database_at_schema_5_gains_an_empty_run_history() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("schema-5.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..5] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", 5).unwrap();
        }

        let mut store = Store::open(&path).unwrap();

        assert!(store.runs().unwrap().is_empty());
        assert_eq!(store.failed_streak().unwrap(), FailedStreak::default());

        let at = chrono::DateTime::from_timestamp(1_794_200_000, 0).unwrap();
        let run = |failure: Option<&str>, attention_sent| RunRecord {
            day: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            started_at: at,
            finished_at: at,
            failure: failure.map(str::to_owned),
            counts: RunCounts::default(),
            attention_sent,
        };
        for record in [
            run(Some("old"), true),
            run(None, false),
            run(Some("timed out"), false),
            run(Some("timed out"), true),
        ] {
            store.record_run(&record).unwrap();
        }
        assert_eq!(store.runs().unwrap()[3], run(Some("timed out"), true));
        assert_eq!(
            store.failed_streak().unwrap(),
            FailedStreak {
                runs: 2,
                attention_sent: true
            },
            "only the failures since the last success count"
        );
    }
}
