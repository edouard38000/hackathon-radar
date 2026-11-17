//! Deadlines, status and Travel-funded news (ticket 09): the changes rechecks
//! make to judgements, the Deadline reminders sent, and the known Hackathons
//! that became Travel-funded. Reminders and newly Travel-funded are tracked
//! apart from the Announced flag, so a Hackathon already Announced can still get them.

use chrono::NaiveDate;
use rusqlite::{Transaction, params};

use super::{ListingKey, Store, upsert_judgements};
use crate::agent::{Judgement, TravelFunded};
use crate::candidate::Candidate;

/// One change a recheck, or a passed deadline, made to a stored judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgementChange {
    pub changed_on: NaiveDate,
    /// `application_deadline`, `application_status` or `travel_funded`.
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

/// A known Hackathon that became Travel-funded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewlyFunded {
    pub hackathon: ListingKey,
    /// The day a recheck found it Travel-funded.
    pub funded_on: NaiveDate,
}

impl Store {
    /// Stores the Agent's answers about known Hackathons it judged again,
    /// recording each change to the deadline, status or Travel-funded status.
    /// A Hackathon that goes from not Travel-funded to Travel-funded becomes
    /// newly Travel-funded until an Alert announces it; one that loses its
    /// funding before then stops being so.
    pub fn save_rechecks(
        &mut self,
        agent: &str,
        rechecked: &[(&Candidate, Judgement)],
        day: NaiveDate,
    ) -> anyhow::Result<()> {
        let mut before = Vec::new();
        for (c, _) in rechecked {
            before.push(self.judgement(&c.source, &c.source_key)?);
        }
        let tx = self.conn.transaction()?;
        for ((c, new), old) in rechecked.iter().zip(&before) {
            let Some(old) = old else { continue };
            let key = (c.source.clone(), c.source_key.clone());
            record_changes(&tx, &key, old, new, day)?;
            let was_funded = old.travel_funded != TravelFunded::None;
            let is_funded = new.travel_funded != TravelFunded::None;
            if !was_funded && is_funded {
                tx.execute(
                    "INSERT OR REPLACE INTO newly_funded (source, source_key, funded_on, announced_on)
                     VALUES (?1, ?2, ?3, NULL)",
                    params![key.0, key.1, day],
                )?;
            } else if was_funded && !is_funded {
                tx.execute(
                    "DELETE FROM newly_funded
                     WHERE source = ?1 AND source_key = ?2 AND announced_on IS NULL",
                    params![key.0, key.1],
                )?;
            }
        }
        upsert_judgements(&tx, agent, rechecked, day)?;
        tx.commit()?;
        Ok(())
    }

    /// Stores `closed` for every judgement whose deadline has passed by `today`,
    /// recording the change; a passed deadline means closed.
    pub fn close_passed_deadlines(&mut self, today: NaiveDate) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO judgement_changes
                (source, source_key, changed_on, field, old_value, new_value)
             SELECT source, source_key, ?1, 'application_status', application_status, 'closed'
             FROM judgements
             WHERE application_deadline < ?1 AND application_status != 'closed'",
            params![today],
        )?;
        tx.execute(
            "UPDATE judgements SET application_status = 'closed'
             WHERE application_deadline < ?1 AND application_status != 'closed'",
            params![today],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every recorded change to one listing's judgement, oldest first.
    pub fn judgement_changes(
        &self,
        source: &str,
        source_key: &str,
    ) -> anyhow::Result<Vec<JudgementChange>> {
        let mut query = self.conn.prepare(
            "SELECT changed_on, field, old_value, new_value FROM judgement_changes
             WHERE source = ?1 AND source_key = ?2
             ORDER BY rowid",
        )?;
        let rows = query.query_map(params![source, source_key], |row| {
            Ok(JudgementChange {
                changed_on: row.get(0)?,
                field: row.get(1)?,
                old_value: row.get(2)?,
                new_value: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The day the Deadline reminder for this Hackathon and deadline was sent, if it was.
    pub fn reminded_on(
        &self,
        source: &str,
        source_key: &str,
        deadline: NaiveDate,
    ) -> anyhow::Result<Option<NaiveDate>> {
        let mut query = self.conn.prepare_cached(
            "SELECT reminded_on FROM deadline_reminders
             WHERE source = ?1 AND source_key = ?2 AND deadline = ?3",
        )?;
        let mut rows = query.query_map(params![source, source_key, deadline], |row| row.get(0))?;
        Ok(rows.next().transpose()?)
    }

    /// Records that an Alert carried the Deadline reminder for each Hackathon and deadline.
    pub fn mark_reminded(
        &mut self,
        reminded: &[(Candidate, NaiveDate)],
        day: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut mark = tx.prepare(
                "INSERT OR IGNORE INTO deadline_reminders (source, source_key, deadline, reminded_on)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (c, deadline) in reminded {
                mark.execute(params![c.source, c.source_key, deadline, day])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The known Hackathons that became Travel-funded and no Alert has announced so yet.
    pub fn newly_funded(&self) -> anyhow::Result<Vec<NewlyFunded>> {
        let mut query = self.conn.prepare(
            "SELECT source, source_key, funded_on FROM newly_funded
             WHERE announced_on IS NULL
             ORDER BY source, source_key",
        )?;
        let rows = query.query_map([], |row| {
            Ok(NewlyFunded {
                hackathon: (row.get(0)?, row.get(1)?),
                funded_on: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Records that an Alert announced these Hackathons as newly Travel-funded.
    pub fn mark_funding_announced(
        &mut self,
        hackathons: &[Candidate],
        day: NaiveDate,
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut mark = tx.prepare(
                "UPDATE newly_funded SET announced_on = ?3
                 WHERE source = ?1 AND source_key = ?2 AND announced_on IS NULL",
            )?;
            for c in hackathons {
                mark.execute(params![c.source, c.source_key, day])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Whether a Hackathon is stored under this listing's identity.
    pub fn has_hackathon(&self, source: &str, source_key: &str) -> anyhow::Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM hackathons WHERE source = ?1 AND source_key = ?2)",
            params![source, source_key],
            |row| row.get(0),
        )?)
    }
}

fn record_changes(
    tx: &Transaction<'_>,
    key: &ListingKey,
    old: &Judgement,
    new: &Judgement,
    day: NaiveDate,
) -> anyhow::Result<()> {
    let date = |d: Option<NaiveDate>| d.map(|d| d.to_string());
    let changes = [
        (
            "application_deadline",
            date(old.deadline),
            date(new.deadline),
        ),
        (
            "application_status",
            Some(old.status.as_str().to_owned()),
            Some(new.status.as_str().to_owned()),
        ),
        (
            "travel_funded",
            Some(old.travel_funded.as_str().to_owned()),
            Some(new.travel_funded.as_str().to_owned()),
        ),
    ];
    let mut insert = tx.prepare(
        "INSERT INTO judgement_changes (source, source_key, changed_on, field, old_value, new_value)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (field, old_value, new_value) in changes {
        if old_value != new_value {
            insert.execute(params![key.0, key.1, day, field, old_value, new_value])?;
        }
    }
    Ok(())
}

/// Moves `from`'s reminders, Travel-funded news and recorded changes to
/// `into`, when one Hackathon is merged into another.
pub(super) fn merge(
    tx: &Transaction<'_>,
    from: &ListingKey,
    into: &ListingKey,
) -> anyhow::Result<()> {
    for table in ["deadline_reminders", "newly_funded"] {
        tx.execute(
            &format!(
                "UPDATE OR IGNORE {table} SET source = ?3, source_key = ?4
                 WHERE source = ?1 AND source_key = ?2"
            ),
            params![from.0, from.1, into.0, into.1],
        )?;
        tx.execute(
            &format!("DELETE FROM {table} WHERE source = ?1 AND source_key = ?2"),
            params![from.0, from.1],
        )?;
    }
    tx.execute(
        "UPDATE judgement_changes SET source = ?3, source_key = ?4
         WHERE source = ?1 AND source_key = ?2",
        params![from.0, from.1, into.0, into.1],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::MIGRATIONS;
    use super::*;
    use crate::agent::ApplicationStatus;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn a_database_at_schema_6_gains_deadlines_and_keeps_its_judgements() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("schema-6.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..6] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", 6).unwrap();
            conn.execute_batch(
                "INSERT INTO judgements (source, source_key, name, is_hackathon, eligible,
                                         travel_funded, reason, agent, instructions_version,
                                         judged_on)
                 VALUES ('s', 'k', 'Old Hack', 1, 1, 'partial', 'Builds.', 'fake', '0.1.0',
                         '2026-11-01');",
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        let judgement = store.judgement("s", "k").unwrap().unwrap();
        assert_eq!(judgement.deadline, None);
        assert_eq!(judgement.status, ApplicationStatus::Unknown);
        assert_eq!(judgement.travel_funded, TravelFunded::Partial);
        assert!(store.newly_funded().unwrap().is_empty());
        assert!(store.judgement_changes("s", "k").unwrap().is_empty());
    }

    #[test]
    fn a_passed_deadline_is_stored_as_closed_and_recorded() {
        let mut store = Store::open_in_memory().unwrap();
        let hack = Candidate {
            source: "s".into(),
            source_key: "k".into(),
            name: "Hack".into(),
            start: date(30),
            end: date(30),
            city: None,
            location: None,
            country: None,
            link: None,
        };
        let judgement = Judgement {
            is_hackathon: true,
            eligible: true,
            eligibility_restriction: None,
            travel_funded: TravelFunded::None,
            travel_detail: None,
            deadline: Some(date(20)),
            status: ApplicationStatus::Open,
            reason: "Builds.".into(),
            theme: None,
        };
        store
            .save_judgements("fake", &[(&hack, judgement)], date(1))
            .unwrap();

        store.close_passed_deadlines(date(20)).unwrap();
        assert_eq!(
            store.judgement("s", "k").unwrap().unwrap().status,
            ApplicationStatus::Open
        );
        store.close_passed_deadlines(date(21)).unwrap();
        store.close_passed_deadlines(date(22)).unwrap();

        assert_eq!(
            store.judgement("s", "k").unwrap().unwrap().status,
            ApplicationStatus::Closed
        );
        assert_eq!(
            store.judgement_changes("s", "k").unwrap(),
            [JudgementChange {
                changed_on: date(21),
                field: "application_status".into(),
                old_value: Some("open".into()),
                new_value: Some("closed".into()),
            }]
        );
    }
}
