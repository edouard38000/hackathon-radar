//! Alerts: composing the text a person receives, and the channel it goes out on.

pub mod attention;
pub mod ntfy;
pub mod photon;

use chrono::{Datelike, NaiveDate};

use crate::agent::{ApplicationStatus, TravelFunded};
use crate::candidate::Candidate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub text: String,
}

/// Where Alerts go. Each Alert channel (ntfy, Photon) implements this.
pub trait AlertChannel {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()>;

    /// Whether a sent Alert reaches the person. Only then do its Hackathons become Announced.
    fn reaches_the_person(&self) -> bool {
        true
    }
}

/// Stands in for an Alert channel when the Profile has none. It prints the
/// Alert, which reaches no one on a schedule, so nothing it prints is Announced.
pub struct PrintChannel;

impl AlertChannel for PrintChannel {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()> {
        println!(
            "\nThe Profile has no Alert channel, so the Alert is printed here and not sent:\n\n{}\n\n\
             To receive Alerts, add an [alert_channel] section to the Profile (see examples/profile.toml).",
            alert.text
        );
        Ok(())
    }

    fn reaches_the_person(&self) -> bool {
        false
    }
}

/// An Alert together with what it tells the person, recorded once it reaches them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composed {
    pub alert: Alert,
    /// A Welcome Alert announces everything that Fits, so later Alerts only
    /// cover what's new. A later Alert announces only the entries it lists;
    /// those counted in "+ X more" wait for the next Alert. Listing an entry
    /// for an already Announced Hackathon changes nothing.
    pub announces: Vec<Candidate>,
    /// The Deadline reminders it lists, with their deadlines; those counted in
    /// "+ X more" stay due for the next Alert.
    pub reminds: Vec<(Candidate, NaiveDate)>,
    /// The listed Hackathons it announces as newly Travel-funded (in a Welcome
    /// Alert, also those it announces); the rest wait for the next Alert.
    pub announces_funding: Vec<Candidate>,
}

/// One Hackathon an Alert can list, why it is there, and what its entry shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub hackathon: Candidate,
    /// It newly Fits: no Alert has announced it yet.
    pub new: bool,
    /// A known Hackathon that became Travel-funded since it was judged.
    pub newly_funded: bool,
    /// When a Deadline reminder is due: the days left until the deadline.
    pub reminder_days_left: Option<i64>,
    /// The application status as of today (a passed deadline means closed).
    pub status: ApplicationStatus,
    pub deadline: Option<NaiveDate>,
    pub travel_funded: TravelFunded,
    /// What the organiser covers, and for whom.
    pub travel_detail: Option<String>,
    /// The way of getting there, such as `train ~2h` or `fly BRU→MUC`.
    pub trip: Option<String>,
    /// The Fare, shown after the trip; set once Fares exist (ticket 08).
    pub fare: Option<String>,
}

impl Entry {
    /// An entry for a Hackathon that newly Fits, before anything else is known about it.
    pub fn new_fit(hackathon: Candidate) -> Self {
        Entry {
            hackathon,
            new: true,
            newly_funded: false,
            reminder_days_left: None,
            status: ApplicationStatus::Unknown,
            deadline: None,
            travel_funded: TravelFunded::None,
            travel_detail: None,
            trip: None,
            fare: None,
        }
    }

    fn is_funded(&self) -> bool {
        self.travel_funded != TravelFunded::None
    }
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

/// Composes the Alert for Hackathons that newly Fit, about which nothing else
/// is known, or `None` when there are none.
pub fn compose(hackathons: &[Candidate], kind: AlertKind) -> Option<Composed> {
    compose_entries(
        hackathons.iter().cloned().map(Entry::new_fit).collect(),
        kind,
    )
}

/// Composes the Alert for these entries, or `None` when there are none.
/// Travel-funded Hackathons come first, then the rest soonest first; at most
/// [`MAX_ENTRIES`] are listed and the rest counted in "+ X more".
pub fn compose_entries(mut entries: Vec<Entry>, kind: AlertKind) -> Option<Composed> {
    if entries.is_empty() {
        return None;
    }
    entries.sort_by(|a, b| {
        (!a.is_funded(), a.hackathon.start, &a.hackathon.name).cmp(&(
            !b.is_funded(),
            b.hackathon.start,
            &b.hackathon.name,
        ))
    });
    let shown = entries.len().min(MAX_ENTRIES);
    let listed = &entries[..shown];
    // What an Alert tells: what it lists, and in a Welcome Alert also every
    // Hackathon it announces, listed or counted.
    let tells = |i: usize, e: &Entry| i < shown || (kind == AlertKind::Welcome && e.new);
    let told = |keep: fn(&Entry) -> bool| -> Vec<Candidate> {
        entries
            .iter()
            .enumerate()
            .filter(|&(i, e)| keep(e) && tells(i, e))
            .map(|(_, e)| e.hackathon.clone())
            .collect()
    };
    // Every Hackathon an Alert tells the person about is Announced, including a
    // reminder or newly Travel-funded entry for one no Alert had announced yet
    // (such as one outside Countries that only Fits now it is Travel-funded).
    let announces = told(|_| true);
    let announces_funding = told(|e| e.newly_funded);
    let reminds = listed
        .iter()
        .filter(|e| e.reminder_days_left.is_some())
        .filter_map(|e| Some((e.hackathon.clone(), e.deadline?)))
        .collect();

    let heading = format!("hackathon-radar: {}", heading(&entries, kind));
    let lines = listed.iter().map(entry);
    let waiting = entries.len() - listed.len();
    let more = (waiting > 0).then(|| format!("+ {waiting} more"));
    let text = std::iter::once(heading)
        .chain(lines)
        .chain(more)
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(Composed {
        alert: Alert { text },
        announces,
        reminds,
        announces_funding,
    })
}

/// Counts each Hackathon once, by the first reason it is in the Alert: newly
/// Fits, newly Travel-funded, or a Deadline reminder.
fn heading(entries: &[Entry], kind: AlertKind) -> String {
    let new = entries.iter().filter(|e| e.new).count();
    let funded = entries.iter().filter(|e| !e.new && e.newly_funded).count();
    let reminders = entries
        .iter()
        .filter(|e| !e.new && !e.newly_funded && e.reminder_days_left.is_some())
        .count();
    let hackathons = |n: usize| match n {
        1 => "1 Hackathon".to_owned(),
        n => format!("{n} Hackathons"),
    };
    let fit = if new == 1 { "fits" } else { "fit" };
    let mut parts = Vec::new();
    if new > 0 {
        parts.push(match kind {
            AlertKind::Welcome => format!("welcome! {} already {fit} you", hackathons(new)),
            AlertKind::New => format!("{} newly {fit} you", hackathons(new)),
        });
    }
    if funded > 0 {
        parts.push(format!("{} newly Travel-funded", hackathons(funded)));
    }
    match reminders {
        0 => {}
        1 => parts.push("1 deadline reminder".to_owned()),
        n => parts.push(format!("{n} deadline reminders")),
    }
    parts.join(", ")
}

/// The entry's lines: name; city, dates and travel (funding, or the trip and
/// Fare); status and deadline; link.
fn entry(entry: &Entry) -> String {
    let hackathon = &entry.hackathon;
    let dates = format_dates(hackathon.start, hackathon.end);
    let mut when_where = match &hackathon.city {
        Some(city) => format!("{city}, {dates}"),
        None => dates,
    };
    if let Some(travel) = travel(entry) {
        when_where.push_str(&format!(" · {travel}"));
    }
    let mut application = format!("{} · ", entry.status.label());
    match entry.deadline {
        Some(deadline) => application.push_str(&format!("deadline {}", format_date(deadline))),
        None => application.push_str("deadline unknown"),
    }
    match entry.reminder_days_left {
        Some(0) => application.push_str(" (closes today)"),
        Some(1) => application.push_str(" (1 day left)"),
        Some(days) => application.push_str(&format!(" ({days} days left)")),
        None => {}
    }
    let mut lines = vec![hackathon.name.clone(), when_where, application];
    lines.extend(hackathon.link.clone());
    lines.join("\n")
}

/// The travel funding when there is some, such as `Travel-funded (partial):
/// lodging only`; otherwise the trip and the Fare.
fn travel(entry: &Entry) -> Option<String> {
    if entry.is_funded() {
        let mut funded = format!(
            "{}Travel-funded ({})",
            if entry.newly_funded { "newly " } else { "" },
            entry.travel_funded.as_str()
        );
        if let Some(detail) = &entry.travel_detail {
            funded.push_str(&format!(": {detail}"));
        }
        return Some(funded);
    }
    let parts: Vec<&str> = entry
        .trip
        .iter()
        .chain(entry.fare.iter())
        .map(String::as_str)
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// `28 Nov 2026`.
fn format_date(day: NaiveDate) -> String {
    format_dates(day, day)
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
            compose(&[hackathon], AlertKind::New).unwrap().alert.text,
            "hackathon-radar: 1 Hackathon newly fits you\n\n\
             Mystery Hack\n1 Dec 2026\nStatus unknown · deadline unknown"
        );
    }

    fn munich() -> Entry {
        Entry::new_fit(Candidate {
            source: "test".into(),
            source_key: "1".into(),
            name: "Munich Hack".into(),
            start: date(2026, 12, 5),
            end: date(2026, 12, 6),
            city: Some("Munich".into()),
            location: None,
            country: Some("Germany".into()),
            link: Some("https://example.org/munich".into()),
        })
    }

    fn text(entries: Vec<Entry>) -> String {
        compose_entries(entries, AlertKind::New).unwrap().alert.text
    }

    #[test]
    fn an_entry_shows_its_trip_status_and_deadline() {
        let entry = Entry {
            trip: Some("fly BRU→MUC".into()),
            status: ApplicationStatus::OpenUntilFull,
            deadline: Some(date(2026, 11, 28)),
            ..munich()
        };
        assert_eq!(
            text(vec![entry]),
            "hackathon-radar: 1 Hackathon newly fits you\n\n\
             Munich Hack\nMunich, 5–6 Dec 2026 · fly BRU→MUC\n\
             Open until full · deadline 28 Nov 2026\nhttps://example.org/munich"
        );
    }

    #[test]
    fn the_fare_slot_follows_the_trip() {
        let entry = Entry {
            trip: Some("fly BRU→MUC".into()),
            fare: Some("€84".into()),
            ..munich()
        };
        assert!(
            text(vec![entry]).contains("\nMunich, 5–6 Dec 2026 · fly BRU→MUC · €84\n"),
            "the Fare goes right after the trip"
        );
    }

    #[test]
    fn travel_funding_shows_instead_of_the_trip() {
        let entry = Entry {
            trip: Some("fly BRU→MUC".into()),
            travel_funded: TravelFunded::Full,
            travel_detail: Some("Flights and two hotel nights".into()),
            ..munich()
        };
        assert!(text(vec![entry]).contains(
            "\nMunich, 5–6 Dec 2026 · Travel-funded (full): Flights and two hotel nights\n"
        ));
    }

    #[test]
    fn reminders_and_newly_funded_entries_are_marked_and_counted_once() {
        let reminder = Entry {
            new: false,
            reminder_days_left: Some(3),
            status: ApplicationStatus::Open,
            deadline: Some(date(2026, 11, 28)),
            ..munich()
        };
        let funded = Entry {
            new: false,
            newly_funded: true,
            travel_funded: TravelFunded::Partial,
            hackathon: Candidate {
                name: "Liège Hack".into(),
                source_key: "2".into(),
                ..munich().hackathon
            },
            ..munich()
        };
        let composed = compose_entries(vec![reminder, funded], AlertKind::New).unwrap();
        assert_eq!(
            composed.alert.text,
            "hackathon-radar: 1 Hackathon newly Travel-funded, 1 deadline reminder\n\n\
             Liège Hack\nMunich, 5–6 Dec 2026 · newly Travel-funded (partial)\n\
             Status unknown · deadline unknown\nhttps://example.org/munich\n\n\
             Munich Hack\nMunich, 5–6 Dec 2026\n\
             Open · deadline 28 Nov 2026 (3 days left)\nhttps://example.org/munich"
        );
        let announced: Vec<&str> = composed.announces.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(announced, ["Liège Hack", "Munich Hack"]);
        assert_eq!(composed.reminds.len(), 1);
        assert_eq!(composed.reminds[0].1, date(2026, 11, 28));
        assert_eq!(composed.announces_funding[0].name, "Liège Hack");
    }

    #[test]
    fn days_left_read_naturally() {
        let with = |days| Entry {
            new: false,
            reminder_days_left: Some(days),
            status: ApplicationStatus::Open,
            deadline: Some(date(2026, 11, 28)),
            ..munich()
        };
        assert!(text(vec![with(0)]).contains("deadline 28 Nov 2026 (closes today)"));
        assert!(text(vec![with(1)]).contains("deadline 28 Nov 2026 (1 day left)"));
        assert!(text(vec![with(2)]).starts_with("hackathon-radar: 1 deadline reminder\n"));
    }

    #[test]
    fn printing_reaches_no_one() {
        assert!(!PrintChannel.reaches_the_person());
    }
}
