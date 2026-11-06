//! The deadline calendar: one all-day "Apply: <name>" event on each listed
//! Hackathon's application deadline, with an alarm `[deadlines]
//! reminder_days` before it, as the personal tracker kept in Google Calendar.
//!
//! The file follows RFC 5545: CRLF line endings, content lines folded at 75
//! octets (never inside a UTF-8 character), and TEXT values escaped. Each
//! event's UID is stable per Hackathon and deadline, so importing a newer
//! export updates events instead of duplicating them, and a moved deadline
//! gets an event of its own.

use chrono::{DateTime, Days, NaiveDate, Utc};

use crate::profile::Profile;
use crate::report::{self, Listed};

/// The calendar file and how many events it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub text: String,
    pub events: usize,
}

impl Calendar {
    /// One line for the person: what was written, or that there is nothing yet.
    pub fn summary(&self) -> String {
        match self.events {
            0 => "Nothing yet: no open deadline of a Hackathon that Fits, so the calendar has \
                  no events."
                .to_owned(),
            1 => "1 deadline in the calendar.".to_owned(),
            n => format!("{n} deadlines in the calendar."),
        }
    }
}

/// The calendar of the deadlines of `listed`, stamped `now`.
pub fn calendar(listed: &[Listed], profile: &Profile, now: DateTime<Utc>) -> Calendar {
    let stamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let mut lines: Vec<String> = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        format!(
            "PRODID:-//hackathon-radar//hackathon-radar {}//EN",
            env!("CARGO_PKG_VERSION")
        ),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
        "X-WR-CALNAME:hackathon-radar deadlines".into(),
    ];
    let mut events = 0;
    for l in listed {
        let Some(deadline) = l.judgement.deadline else {
            continue;
        };
        events += 1;
        let summary = format!("Apply: {}", l.hackathon.name);
        let mut description = Vec::new();
        description.extend(l.place());
        description.push(l.dates());
        description.extend(l.hackathon.link.clone());
        lines.extend([
            "BEGIN:VEVENT".into(),
            format!("UID:{}", escape(&uid(l, deadline))),
            format!("DTSTAMP:{stamp}"),
            format!("DTSTART;VALUE=DATE:{}", ics_date(deadline)),
            format!("DTEND;VALUE=DATE:{}", ics_date(deadline + Days::new(1))),
            format!("SUMMARY:{}", escape(&summary)),
            format!("DESCRIPTION:{}", escape(&description.join("\n"))),
        ]);
        if let Some(link) = l.hackathon.link.as_deref().and_then(report::web_link) {
            lines.push(format!("URL:{link}"));
        }
        lines.extend([
            "TRANSP:TRANSPARENT".into(),
            "BEGIN:VALARM".into(),
            "ACTION:DISPLAY".into(),
            format!("DESCRIPTION:{}", escape(&summary)),
            format!("TRIGGER:-P{}D", profile.deadlines.reminder_days),
            "END:VALARM".into(),
            "END:VEVENT".into(),
        ]);
    }
    lines.push("END:VCALENDAR".into());
    let text = lines.iter().map(|line| fold(line)).collect();
    Calendar { text, events }
}

/// `apply-<hash of the Hackathon>-<deadline>@hackathon-radar`. The hash is
/// FNV-1a, which never changes between builds, of the Hackathon's listing key.
fn uid(listed: &Listed, deadline: NaiveDate) -> String {
    let key = format!(
        "{}\u{1f}{}",
        listed.hackathon.source, listed.hackathon.source_key
    );
    format!(
        "apply-{:016x}-{}@hackathon-radar",
        fnv1a(key.as_bytes()),
        ics_date(deadline)
    )
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn ics_date(day: NaiveDate) -> String {
    day.format("%Y%m%d").to_string()
}

/// Escapes a TEXT value (RFC 5545 3.3.11): backslash, semicolon, comma and newline.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Folds one content line (RFC 5545 3.1): at most 75 octets per line, each
/// continuation starting with a space, never splitting a UTF-8 character;
/// every line ends with CRLF.
pub fn fold(line: &str) -> String {
    const LIMIT: usize = 75;
    let mut out = String::with_capacity(line.len() + line.len() / 70 * 3 + 2);
    let mut used = 0;
    for c in line.chars() {
        if used + c.len_utf8() > LIMIT {
            out.push_str("\r\n ");
            used = 1;
        }
        out.push(c);
        used += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped() {
        assert_eq!(
            escape("Hack, Build; Ship\\now\nnext"),
            "Hack\\, Build\\; Ship\\\\now\\nnext"
        );
    }

    #[test]
    fn long_lines_fold_at_75_octets_without_splitting_characters() {
        let line = format!("SUMMARY:{}", "é".repeat(60));
        let folded = fold(&line);
        assert!(folded.ends_with("\r\n"));
        for physical in folded.trim_end_matches("\r\n").split("\r\n") {
            assert!(
                physical.len() <= 75,
                "{physical:?} is {} octets",
                physical.len()
            );
        }
        let unfolded = folded.trim_end_matches("\r\n").replace("\r\n ", "");
        assert_eq!(unfolded, line);
        assert_eq!(fold("SHORT:1"), "SHORT:1\r\n");
    }

    #[test]
    fn the_hash_is_fnv1a() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
