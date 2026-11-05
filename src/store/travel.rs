//! The travel estimates cache and the stored Trips (ticket 07, ADR 0002).

use chrono::NaiveDate;
use rusqlite::params;

use super::Store;
use crate::agent::INSTRUCTIONS_VERSION;
use crate::agent::travel::{Destination, Estimate};
use crate::candidate::Candidate;
use crate::travel::{Trip, TripKind, key};

impl Store {
    /// The cached estimate for getting from `home_city` to `destination`, if the Agent has made one.
    pub fn travel_estimate(
        &self,
        home_city: &str,
        destination: &Destination,
    ) -> anyhow::Result<Option<Estimate>> {
        let mut query = self.conn.prepare_cached(
            "SELECT train_minutes, airport FROM travel_estimates
             WHERE home_city = ?1 AND city = ?2 AND country = ?3",
        )?;
        let mut rows = query.query_map(
            params![
                key(home_city),
                key(&destination.city),
                key(&destination.country)
            ],
            |row| {
                Ok(Estimate {
                    train_minutes: row.get(0)?,
                    airport: row.get(1)?,
                })
            },
        )?;
        Ok(rows.next().transpose()?)
    }

    /// Caches the Agent's estimates from `home_city`, replacing any earlier one of the same city.
    pub fn save_travel_estimates(
        &mut self,
        home_city: &str,
        agent: &str,
        estimates: &[(&Destination, Estimate)],
        estimated_on: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT OR REPLACE INTO travel_estimates
                    (home_city, city, country, train_minutes, airport, agent,
                     instructions_version, estimated_on)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (destination, estimate) in estimates {
                upsert.execute(params![
                    key(home_city),
                    key(&destination.city),
                    key(&destination.country),
                    estimate.train_minutes,
                    estimate.airport,
                    agent,
                    INSTRUCTIONS_VERSION,
                    estimated_on,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Stores the Trip to each Hackathon, or forgets it when there is none yet.
    pub fn save_trips(&mut self, trips: &[(&Candidate, Option<Trip>)]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT OR REPLACE INTO trips
                    (source, source_key, kind, train_minutes, airport, label)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut delete =
                tx.prepare("DELETE FROM trips WHERE source = ?1 AND source_key = ?2")?;
            for (c, trip) in trips {
                match trip {
                    Some(t) => upsert.execute(params![
                        c.source,
                        c.source_key,
                        t.kind.as_str(),
                        t.train_minutes,
                        t.airport,
                        t.label,
                    ])?,
                    None => delete.execute(params![c.source, c.source_key])?,
                };
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The stored Trip to one listing's Hackathon.
    pub fn trip(&self, source: &str, source_key: &str) -> anyhow::Result<Option<Trip>> {
        let mut query = self.conn.prepare_cached(
            "SELECT kind, train_minutes, airport, label FROM trips
             WHERE source = ?1 AND source_key = ?2",
        )?;
        let mut rows = query.query_map(params![source, source_key], |row| {
            let kind: String = row.get(0)?;
            let kind = TripKind::parse(&kind).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    format!("unknown trip kind {kind:?}").into(),
                )
            })?;
            Ok(Trip {
                kind,
                train_minutes: row.get(1)?,
                airport: row.get(2)?,
                label: row.get(3)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::MIGRATIONS;
    use super::*;

    fn munich() -> Destination {
        Destination {
            city: "Munich".into(),
            country: "Germany".into(),
        }
    }

    #[test]
    fn estimates_are_cached_per_home_city_and_destination() {
        let mut store = Store::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let estimate = Estimate {
            train_minutes: Some(384),
            airport: Some("MUC".into()),
        };
        store
            .save_travel_estimates("Brussels", "fake", &[(&munich(), estimate.clone())], day)
            .unwrap();

        let as_written = Destination {
            city: " munich".into(),
            country: "GERMANY".into(),
        };
        assert_eq!(
            store.travel_estimate("brussels", &as_written).unwrap(),
            Some(estimate)
        );
        assert_eq!(store.travel_estimate("Lyon", &munich()).unwrap(), None);
        let other_country = Destination {
            city: "Munich".into(),
            country: "United States".into(),
        };
        assert_eq!(
            store.travel_estimate("Brussels", &other_country).unwrap(),
            None
        );
    }

    #[test]
    fn a_database_at_schema_3_gains_the_travel_tables() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("schema-3.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..3] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", 3).unwrap();
        }

        let store = Store::open(&path).unwrap();

        let version: i64 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::try_from(MIGRATIONS.len()).unwrap());
        assert_eq!(store.travel_estimate("Brussels", &munich()).unwrap(), None);
        assert_eq!(store.trip("s", "k").unwrap(), None);
    }
}
