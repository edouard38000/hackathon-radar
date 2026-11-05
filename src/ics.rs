//! A small reader for iCalendar (RFC 5545) feeds: just enough to read the
//! VEVENTs that calendar Sources publish.

use chrono::{Days, NaiveDate};

/// One VEVENT, with its properties in feed order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// Upper-cased, such as `DTSTART`.
    pub name: String,
    /// Parameter names upper-cased, values as written.
    pub params: Vec<(String, String)>,
    /// The raw value, still escaped.
    pub value: String,
}

impl Event {
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// A TEXT property, unescaped and trimmed; `None` when absent or blank.
    pub fn text(&self, name: &str) -> Option<String> {
        let text = unescape_text(&self.property(name)?.value);
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// The first day of the event.
    pub fn start_date(&self) -> Option<NaiveDate> {
        parse_date(&self.property("DTSTART")?.value)
    }

    /// The last day of the event, inclusive.
    ///
    /// An all-day DTEND is exclusive (RFC 5545 3.6.1), so it is moved back a day.
    /// Without a DTEND the event lasts its start day.
    pub fn end_date(&self) -> Option<NaiveDate> {
        let start = self.start_date()?;
        let Some(dtend) = self.property("DTEND") else {
            return Some(start);
        };
        let end = parse_date(&dtend.value)?;
        let end = if dtend.is_date_only() {
            end.checked_sub_days(Days::new(1))?
        } else {
            end
        };
        Some(end.max(start))
    }
}

impl Property {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn is_date_only(&self) -> bool {
        self.param("VALUE")
            .is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
            || !self.value.contains('T')
    }
}

/// Reads every VEVENT in a feed. Lines that can't be read are skipped, since
/// one broken listing shouldn't hide the rest.
pub fn parse_events(input: &str) -> Vec<Event> {
    let mut events = Vec::new();
    // Components nest (a VALARM inside a VEVENT); only the VEVENT's own lines count.
    let mut stack: Vec<String> = Vec::new();
    let mut current: Option<Event> = None;

    for line in unfold(input) {
        let Some(property) = parse_line(&line) else {
            continue;
        };
        match property.name.as_str() {
            "BEGIN" => {
                let component = property.value.trim().to_ascii_uppercase();
                if component == "VEVENT" && current.is_none() {
                    current = Some(Event {
                        properties: Vec::new(),
                    });
                }
                stack.push(component);
            }
            "END" => {
                let component = property.value.trim().to_ascii_uppercase();
                if stack.last() == Some(&component) {
                    stack.pop();
                }
                if component == "VEVENT" && !stack.iter().any(|c| c == "VEVENT") {
                    events.extend(current.take());
                }
            }
            _ if stack.last().map(String::as_str) == Some("VEVENT") => {
                if let Some(event) = current.as_mut() {
                    event.properties.push(property);
                }
            }
            _ => {}
        }
    }
    events
}

/// Joins folded lines: a line starting with a space or tab continues the previous one.
fn unfold(input: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in input.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match raw.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => lines.last_mut().unwrap().push_str(rest),
            _ if raw.is_empty() => {}
            _ => lines.push(raw.to_owned()),
        }
    }
    lines
}

fn parse_line(line: &str) -> Option<Property> {
    let colon = find_outside_quotes(line, ':')?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = split_outside_quotes(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((
                k.trim().to_ascii_uppercase(),
                v.trim_matches('"').to_owned(),
            ))
        })
        .collect();
    Some(Property {
        name,
        params,
        value: value.to_owned(),
    })
}

fn find_outside_quotes(s: &str, target: char) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            c if c == target && !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

fn split_outside_quotes(s: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = s;
    while let Some(i) = find_outside_quotes(rest, separator) {
        parts.push(&rest[..i]);
        rest = &rest[i + separator.len_utf8()..];
    }
    parts.push(rest);
    parts
}

/// Undoes TEXT escaping (RFC 5545 3.3.11).
pub fn unescape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Reads the date part of a DATE (`20261120`) or DATE-TIME (`20261120T180000Z`) value.
/// The calendar date as written is what a person sees on the listing, so time zones are ignored.
fn parse_date(value: &str) -> Option<NaiveDate> {
    let digits = value.trim().get(..8)?;
    NaiveDate::parse_from_str(digits, "%Y%m%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn reads_folded_escaped_lines_with_crlf() {
        let feed = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Hack\\, Build\\; Ship\r\nLOCATION:Rue \r\n de la Loi\\, Brussels\r\nDTSTART;VALUE=DATE:20261120\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse_events(feed);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text("summary").unwrap(), "Hack, Build; Ship");
        assert_eq!(
            events[0].text("LOCATION").unwrap(),
            "Rue de la Loi, Brussels"
        );
    }

    #[test]
    fn reads_lf_only_feeds_and_tab_folding() {
        let feed = "BEGIN:VEVENT\nSUMMARY:Long\n\tname\nDTSTART:20261120\nEND:VEVENT\n";
        let events = parse_events(feed);
        assert_eq!(events[0].text("SUMMARY").unwrap(), "Longname");
    }

    #[test]
    fn all_day_end_is_exclusive() {
        let feed =
            "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261120\nDTEND;VALUE=DATE:20261123\nEND:VEVENT\n";
        let event = &parse_events(feed)[0];
        assert_eq!(event.start_date(), Some(date(2026, 11, 20)));
        assert_eq!(event.end_date(), Some(date(2026, 11, 22)));
    }

    #[test]
    fn date_time_end_keeps_its_day() {
        let feed = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Brussels:20261120T090000\nDTEND;TZID=Europe/Brussels:20261121T180000\nEND:VEVENT\n";
        let event = &parse_events(feed)[0];
        assert_eq!(event.start_date(), Some(date(2026, 11, 20)));
        assert_eq!(event.end_date(), Some(date(2026, 11, 21)));
    }

    #[test]
    fn missing_end_means_a_single_day() {
        let feed = "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261120\nEND:VEVENT\n";
        let event = &parse_events(feed)[0];
        assert_eq!(event.end_date(), Some(date(2026, 11, 20)));
    }

    #[test]
    fn ignores_nested_alarms_and_quoted_colons() {
        let feed = "BEGIN:VEVENT\nSUMMARY;ALTREP=\"https://x.test/a:b\":Real\nBEGIN:VALARM\nSUMMARY:Alarm\nEND:VALARM\nURL:https://x.test/e\nEND:VEVENT\n";
        let events = parse_events(feed);
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.text("SUMMARY").unwrap(), "Real");
        assert_eq!(
            event.property("SUMMARY").unwrap().param("altrep"),
            Some("https://x.test/a:b")
        );
        assert_eq!(event.text("URL").unwrap(), "https://x.test/e");
        assert_eq!(
            event
                .properties
                .iter()
                .filter(|p| p.name == "SUMMARY")
                .count(),
            1
        );
    }

    #[test]
    fn unparseable_dates_are_none() {
        let feed = "BEGIN:VEVENT\nDTSTART:soon\nEND:VEVENT\n";
        assert_eq!(parse_events(feed)[0].start_date(), None);
    }
}
