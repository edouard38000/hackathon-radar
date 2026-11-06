//! dev.events, through its RSS feed of upcoming developer events. The feed
//! mixes conferences, meetups and masterclasses with hackathons; only items
//! with the `hackathon` category are kept.
//!
//! Each item's description says when and where it happens: `X is happening on
//! November 12, 2026 in San Francisco, United States, North America.`, or
//! `…, Online.` for online events, which are left out. The end date only
//! appears in the text drawn on the item's preview image (`Nov 12-13 2026`),
//! so it is read from there, and is the start date when that doesn't read.

use chrono::{Datelike, NaiveDate};

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::sources::{Source, places};

pub const NAME: &str = "dev.events";
pub const FEED_URL: &str = "https://dev.events/rss.xml";

pub struct DevEvents<H> {
    http: H,
}

impl<H: HttpGet> DevEvents<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }
}

impl<H: HttpGet> Source for DevEvents<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        let feed = self.http.get_text(FEED_URL)?;
        if !feed.contains("<rss") {
            anyhow::bail!("{FEED_URL} is not an RSS feed");
        }
        Ok(parse(&feed))
    }
}

/// The feed's in-person hackathons, as Candidates.
pub fn parse(feed: &str) -> Vec<Candidate> {
    feed.split("<item>")
        .skip(1)
        .filter_map(|item| item.split("</item>").next())
        .filter(|item| categories(item).any(|c| c.eq_ignore_ascii_case("hackathon")))
        .filter_map(to_candidate)
        .collect()
}

fn to_candidate(item: &str) -> Option<Candidate> {
    let name = element(item, "title")?;
    let link = element(item, "link")?;
    let key = element(item, "guid").unwrap_or_else(|| link.clone());
    let description = element(item, "description")?;
    let (_, when_where) = description.split_once(" is happening on ")?;
    let (when_where, _) = when_where
        .split_once(". More information")
        .unwrap_or((when_where, ""));
    // `November 12, 2026 in San Francisco, United States, North America`
    let (when, place) = when_where.split_once(" in ")?;
    let start = NaiveDate::parse_from_str(when.trim(), "%B %d, %Y").ok()?;
    let end = image_dates(item)
        .filter(|(from, _)| *from == start)
        .map_or(start, |(_, to)| to);
    // The last part is the continent.
    let mut parts: Vec<&str> = place.split(',').map(str::trim).collect();
    if parts.len() > 2 {
        parts.pop();
    }
    let location = parts.join(", ");
    let (city, country) = places::from_address(&location);
    Some(Candidate {
        source: NAME.to_owned(),
        source_key: key,
        name,
        start,
        end: end.max(start),
        city,
        location: Some(location),
        country,
        link: Some(link),
    })
}

fn categories(item: &str) -> impl Iterator<Item = &str> {
    item.split("<category>")
        .skip(1)
        .filter_map(|c| c.split("</category>").next())
        .map(str::trim)
}

/// The text of the first `<name>…</name>` element, without its CDATA wrapper.
fn element(item: &str, name: &str) -> Option<String> {
    let (_, after) = item.split_once(&format!("<{name}>"))?;
    let (inner, _) = after.split_once(&format!("</{name}>"))?;
    let inner = inner.trim();
    let inner = inner
        .strip_prefix("<![CDATA[")
        .and_then(|i| i.strip_suffix("]]>"))
        .unwrap_or(inner);
    let text = inner
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The dates drawn on the preview image, whose URL holds them doubly
/// URL-encoded after `Lato_40:`: `Nov 12 2026`, `Nov 12-13 2026` or
/// `Nov 30 - Dec 2 2026`.
fn image_dates(item: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (_, after) = item.split_once("Lato_40:")?;
    let encoded = after.split([',', '/', '"']).next()?;
    let text = percent_decode(&percent_decode(encoded));
    read_short_range(&text)
}

fn read_short_range(text: &str) -> Option<(NaiveDate, NaiveDate)> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .collect();
    let date = |m: &str, d: &str, y: &str| {
        NaiveDate::parse_from_str(&format!("{m} {d} {y}"), "%b %d %Y").ok()
    };
    match words[..] {
        [m, d, y] => Some((date(m, d, y)?, date(m, d, y)?)),
        [m, d, e, y] => Some((date(m, d, y)?, date(m, e, y)?)),
        [m, d, em, e, y] => {
            let end = date(em, e, y)?;
            let start = date(m, d, y)?;
            // `Dec 30 - Jan 2 2027` starts the year before.
            let start = if start > end {
                start.with_year(end.year() - 1)?
            } else {
                start
            };
            Some((start, end))
        }
        _ => None,
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        if bytes[i] == b'%'
            && let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn reads_the_dates_on_the_preview_image() {
        assert_eq!(
            read_short_range("Nov 12 2026"),
            Some((date(2026, 11, 12), date(2026, 11, 12)))
        );
        assert_eq!(
            read_short_range("Dec 10-11 2026"),
            Some((date(2026, 12, 10), date(2026, 12, 11)))
        );
        assert_eq!(
            read_short_range("May 31 - Jun 4 2027"),
            Some((date(2027, 5, 31), date(2027, 6, 4)))
        );
        assert_eq!(
            read_short_range("Dec 30 - Jan 2 2027"),
            Some((date(2026, 12, 30), date(2027, 1, 2)))
        );
        assert_eq!(read_short_range("Soon"), None);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("Nov%252012"), "Nov%2012");
        assert_eq!(percent_decode(&percent_decode("Nov%252012")), "Nov 12");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
