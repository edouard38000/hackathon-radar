//! Alerts: composing the text a person receives, and the channel it goes out on.

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

/// Composes the Alert for these Hackathons, soonest first, or `None` when there are none.
pub fn compose(fitting: &[Candidate]) -> Option<Alert> {
    if fitting.is_empty() {
        return None;
    }
    let mut hackathons: Vec<&Candidate> = fitting.iter().collect();
    hackathons.sort_by(|a, b| (a.start, &a.name).cmp(&(b.start, &b.name)));

    let heading = match hackathons.len() {
        1 => "hackathon-radar: 1 Hackathon fits you".to_owned(),
        n => format!("hackathon-radar: {n} Hackathons fit you"),
    };
    let entries = hackathons.iter().map(|h| entry(h));
    let text = std::iter::once(heading)
        .chain(entries)
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
        assert_eq!(compose(&[]), None);
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
            compose(&[hackathon]).unwrap().text,
            "hackathon-radar: 1 Hackathon fits you\n\nMystery Hack\n1 Dec 2026"
        );
    }
}
