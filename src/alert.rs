//! Alerts: composing the text a person receives, and the channel it goes out on.

pub mod ntfy;

use chrono::{Datelike, NaiveDate};

use crate::candidate::Candidate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub text: String,
}

/// Where Alerts go. Each Alert channel (ntfy, Photon) implements this.
pub trait AlertChannel {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// The first Alert, listing what already Fits when tracking starts.
    Welcome,
    /// Every later Alert, about Hackathons that newly Fit.
    New,
}

/// An Alert lists at most this many Hackathons and counts the rest, so it stays readable on a phone.
pub const MAX_ENTRIES: usize = 6;

/// Composes the Alert for these Hackathons, soonest first, or `None` when there are none.
pub fn compose(hackathons: &[Candidate], kind: AlertKind) -> Option<Alert> {
    if hackathons.is_empty() {
        return None;
    }
    let mut hackathons: Vec<&Candidate> = hackathons.iter().collect();
    hackathons.sort_by(|a, b| (a.start, &a.name).cmp(&(b.start, &b.name)));

    let count = match hackathons.len() {
        1 => "1 Hackathon".to_owned(),
        n => format!("{n} Hackathons"),
    };
    let fit = if hackathons.len() == 1 { "fits" } else { "fit" };
    let heading = match kind {
        AlertKind::Welcome => format!("hackathon-radar: welcome! {count} already {fit} you"),
        AlertKind::New => format!("hackathon-radar: {count} newly {fit} you"),
    };
    let entries = hackathons.iter().take(MAX_ENTRIES).map(|h| entry(h));
    let more = hackathons
        .len()
        .checked_sub(MAX_ENTRIES)
        .filter(|&n| n > 0)
        .map(|n| format!("+ {n} more"));
    let text = std::iter::once(heading)
        .chain(entries)
        .chain(more)
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(Alert { text })
}

fn entry(hackathon: &Candidate) -> String {
    let dates = format_dates(hackathon.start, hackathon.end);
    let when_where = match &hackathon.city {
        Some(city) => format!("{city}, {dates}"),
        None => dates,
    };
    let mut lines = vec![hackathon.name.clone(), when_where];
    lines.extend(hackathon.link.clone());
    lines.join("\n")
}

/// `12 Mar 2027`, `20–22 Nov 2026`, `30 Nov – 2 Dec 2026` or `30 Dec 2026 – 2 Jan 2027`.
fn format_dates(start: NaiveDate, end: NaiveDate) -> String {
    let day_month = |d: NaiveDate| format!("{} {}", d.day(), d.format("%b"));
    if start == end {
        format!("{} {}", day_month(start), start.year())
    } else if start.year() != end.year() {
        format!(
            "{} {} – {} {}",
            day_month(start),
            start.year(),
            day_month(end),
            end.year()
        )
    } else if start.month() != end.month() {
        format!("{} – {} {}", day_month(start), day_month(end), end.year())
    } else {
        format!("{}–{} {}", start.day(), day_month(end), end.year())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn formats_date_ranges() {
        assert_eq!(
            format_dates(date(2027, 3, 12), date(2027, 3, 12)),
            "12 Mar 2027"
        );
        assert_eq!(
            format_dates(date(2026, 11, 20), date(2026, 11, 22)),
            "20–22 Nov 2026"
        );
        assert_eq!(
            format_dates(date(2026, 11, 30), date(2026, 12, 2)),
            "30 Nov – 2 Dec 2026"
        );
        assert_eq!(
            format_dates(date(2026, 12, 30), date(2027, 1, 2)),
            "30 Dec 2026 – 2 Jan 2027"
        );
    }

    #[test]
    fn nothing_to_compose_means_no_alert() {
        assert_eq!(compose(&[], AlertKind::Welcome), None);
        assert_eq!(compose(&[], AlertKind::New), None);
    }

    #[test]
    fn an_entry_without_city_or_link_keeps_its_dates() {
        let hackathon = Candidate {
            source: "test".into(),
            source_key: "1".into(),
            name: "Mystery Hack".into(),
            start: date(2026, 12, 1),
            end: date(2026, 12, 1),
            city: None,
            location: None,
            country: Some("Belgium".into()),
            link: None,
        };
        assert_eq!(
            compose(&[hackathon], AlertKind::New).unwrap().text,
            "hackathon-radar: 1 Hackathon newly fits you\n\nMystery Hack\n1 Dec 2026"
        );
    }
}
