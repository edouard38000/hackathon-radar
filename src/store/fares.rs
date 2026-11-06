//! The Fares cache (ticket 08, ADR 0002): one Fare per route, dates and
//! currency, so Hackathons sharing them share one lookup.

use chrono::NaiveDate;
use rusqlite::params;

use super::Store;
use crate::fares::{Fare, FareQuery, FoundFare};

impl Store {
    /// The stored Fare for `query`, fresh or not.
    pub fn fare(&self, query: &FareQuery) -> anyhow::Result<Option<Fare>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT price, from_airport, verified, checked_on FROM fares
             WHERE from_airports = ?1 AND to_airport = ?2 AND outbound_date = ?3
               AND return_date = ?4 AND currency = ?5",
        )?;
        let mut rows = statement.query_map(
            params![
                query.from.join(","),
                query.to,
                query.outbound,
                query.inbound,
                query.currency
            ],
            |row| {
                Ok(Fare {
                    price: row.get(0)?,
                    from_airport: row.get(1)?,
                    verified: row.get(2)?,
                    checked_on: row.get(3)?,
                })
            },
        )?;
        Ok(rows.next().transpose()?)
    }

    /// Stores what a lookup found for `query` (`None`: no flight), replacing
    /// the earlier Fare. `source` is `serpapi` or the Agent's name.
    pub fn save_fare(
        &mut self,
        query: &FareQuery,
        found: Option<&FoundFare>,
        verified: bool,
        source: &str,
        checked_on: NaiveDate,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO fares
                (from_airports, to_airport, outbound_date, return_date, currency,
                 price, from_airport, verified, source, checked_on)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                query.from.join(","),
                query.to,
                query.outbound,
                query.inbound,
                query.currency,
                found.map(|f| f.price),
                found.and_then(|f| f.from_airport.clone()),
                verified,
                source,
                checked_on,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::MIGRATIONS;
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn munich() -> FareQuery {
        FareQuery {
            from: vec!["BRU".into(), "CRL".into()],
            to: "MUC".into(),
            outbound: NaiveDate::from_ymd_opt(2026, 12, 4).unwrap(),
            inbound: NaiveDate::from_ymd_opt(2026, 12, 7).unwrap(),
            currency: "EUR".into(),
        }
    }

    #[test]
    fn a_fare_is_kept_per_route_dates_and_currency_and_replaced() {
        let mut store = Store::open_in_memory().unwrap();
        let found = FoundFare {
            price: 185,
            from_airport: Some("CRL".into()),
        };
        store
            .save_fare(&munich(), Some(&found), false, "fake", day(9))
            .unwrap();
        assert_eq!(
            store.fare(&munich()).unwrap(),
            Some(Fare {
                price: Some(185),
                from_airport: Some("CRL".into()),
                verified: false,
                checked_on: day(9),
            })
        );
        let in_dollars = FareQuery {
            currency: "USD".into(),
            ..munich()
        };
        assert_eq!(store.fare(&in_dollars).unwrap(), None);
        let from_bru_only = FareQuery {
            from: vec!["BRU".into()],
            ..munich()
        };
        assert_eq!(store.fare(&from_bru_only).unwrap(), None);

        store
            .save_fare(&munich(), None, true, "serpapi", day(12))
            .unwrap();
        assert_eq!(
            store.fare(&munich()).unwrap(),
            Some(Fare {
                price: None,
                from_airport: None,
                verified: true,
                checked_on: day(12),
            })
        );
    }

    #[test]
    fn a_database_from_before_fares_gains_the_fares_table() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("before-fares.sqlite");
        let before = MIGRATIONS.len() - 1;
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..before] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", i64::try_from(before).unwrap())
                .unwrap();
        }

        let store = Store::open(&path).unwrap();

        assert_eq!(store.fare(&munich()).unwrap(), None);
    }
}
