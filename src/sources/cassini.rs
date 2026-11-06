//! The CASSINI Hackathons, the EU Space Programme's hackathons, through their
//! page. Each edition takes place on the same weekend in about ten cities, so
//! each city is its own Candidate, linked to its local organiser's page. The
//! page has no structured data: the edition (`12th CASSINI Hackathon`) and its
//! dates (`27-29 November 2026`) are read from its text, and each city from its
//! location block.

use chrono::NaiveDate;

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::sources::{Source, places};

pub const NAME: &str = "cassini";
pub const PAGE_URL: &str = "https://www.cassini.eu/hackathons/";

pub struct Cassini<H> {
    http: H,
}

impl<H: HttpGet> Cassini<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }
}

impl<H: HttpGet> Source for Cassini<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        Ok(parse(&self.http.get_text(PAGE_URL)?))
    }
}

/// One Candidate per city of the edition the page announces; none when the
/// page names no edition or dates.
pub fn parse(html: &str) -> Vec<Candidate> {
    let text = strip_tags(html);
    let Some(edition) = edition(&text) else {
        return Vec::new();
    };
    let Some((start, end)) = find_dates(&text) else {
        return Vec::new();
    };
    let mut candidates: Vec<Candidate> = Vec::new();
    // Location blocks follow `paragraph--type--hackathon-location ` (the
    // `-info` blocks inside them hold the organiser and the city).
    for block in html.split("paragraph--type--hackathon-location ").skip(1) {
        let Some(link) = organiser_page(block) else {
            continue;
        };
        if candidates
            .iter()
            .any(|c| c.link.as_deref() == Some(link.as_str()))
        {
            continue;
        }
        let Some(place) = city_line(block) else {
            continue;
        };
        let (city, country) = places::from_address(&place);
        let Some(city) = city else { continue };
        candidates.push(Candidate {
            source: NAME.to_owned(),
            source_key: format!(
                "{}/{}",
                places::fold(&edition).replace(' ', "-"),
                places::fold(&city).replace(' ', "-")
            ),
            name: format!("{edition} – {city}"),
            start,
            end,
            city: Some(city),
            location: Some(place),
            country,
            link: Some(link),
        });
    }
    candidates
}

/// `12th CASSINI Hackathon`, from the page's first mention of an edition.
fn edition(text: &str) -> Option<String> {
    text.match_indices("CASSINI Hackathon").find_map(|(at, _)| {
        let ordinal = text[..at].split_whitespace().last()?;
        let digits = ordinal.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
            .then(|| format!("{ordinal} CASSINI Hackathon"))
    })
}

/// The first date range in the text: `27-29 November 2026`, `27 - 29 November
/// 2026` or `30 November - 2 December 2026`.
fn find_dates(text: &str) -> Option<(NaiveDate, NaiveDate)> {
    let mut words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '-' || c == '–')
        .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation()))
        .filter(|w| !w.is_empty())
        .collect();
    // So the last words still fill a window.
    words.extend(["", "", "", ""]);
    let day = |w: &str| w.parse::<u32>().ok().filter(|d| (1..=31).contains(d));
    let month = |w: &str| {
        NaiveDate::parse_from_str(&format!("1 {w} 2000"), "%d %B %Y")
            .ok()
            .map(|d| chrono::Datelike::month(&d))
    };
    let year = |w: &str| w.parse::<i32>().ok().filter(|y| (2000..2100).contains(y));
    let date = |y: i32, m: u32, d: u32| NaiveDate::from_ymd_opt(y, m, d);
    for window in words.windows(5) {
        // 27 29 November 2026 …
        if let (Some(d1), Some(d2), Some(m), Some(y)) = (
            day(window[0]),
            day(window[1]),
            month(window[2]),
            year(window[3]),
        ) {
            return Some((date(y, m, d1)?, date(y, m, d2)?));
        }
        // 30 November 2 December 2026
        if let (Some(d1), Some(m1), Some(d2), Some(m2), Some(y)) = (
            day(window[0]),
            month(window[1]),
            day(window[2]),
            month(window[3]),
            year(window[4]),
        ) {
            let start_year = if m1 > m2 { y - 1 } else { y };
            return Some((date(start_year, m1, d1)?, date(y, m2, d2)?));
        }
    }
    None
}

/// The organiser page a location block links to.
fn organiser_page(block: &str) -> Option<String> {
    let (_, after) = block.split_once("field--name-field-organiser-page")?;
    let (_, after) = after.split_once("href=\"")?;
    let (link, _) = after.split_once('"')?;
    link.starts_with("http").then(|| link.to_owned())
}

/// The last paragraph of a location's description: `Sofia, Bulgaria`.
fn city_line(block: &str) -> Option<String> {
    let (_, after) = block.split_once("field--name-field-description")?;
    let (description, _) = after.split_once("</div>")?;
    description
        .split("<p>")
        .map(|p| strip_tags(p).trim().to_owned())
        .filter(|p| !p.is_empty())
        .last()
}

/// The text of some HTML: styles and scripts are dropped, tags become spaces,
/// and the few entities the page uses are decoded.
fn strip_tags(html: &str) -> String {
    let mut visible = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let next = ["style", "script"]
            .into_iter()
            .filter_map(|tag| rest.find(&format!("<{tag}")).map(|at| (at, tag)))
            .min();
        let Some((at, tag)) = next else {
            visible.push_str(rest);
            break;
        };
        visible.push_str(&rest[..at]);
        let close = format!("</{tag}>");
        match rest[at..].find(&close) {
            Some(end) => rest = &rest[at + end + close.len()..],
            None => break,
        }
    }

    let mut text = String::with_capacity(visible.len());
    let mut in_tag = false;
    for c in visible.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#039;", "'");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn reads_each_date_range_shape() {
        assert_eq!(
            find_dates("Open now! 27-29 November 2026 more"),
            Some((date(2026, 11, 27), date(2026, 11, 29)))
        );
        assert_eq!(
            find_dates("from 27 - 29 November 2026."),
            Some((date(2026, 11, 27), date(2026, 11, 29)))
        );
        assert_eq!(
            find_dates("from 30 November - 2 December 2026"),
            Some((date(2026, 11, 30), date(2026, 12, 2)))
        );
        assert_eq!(find_dates("soon, in November"), None);
    }

    #[test]
    fn the_edition_is_its_ordinal() {
        assert_eq!(
            edition("Registrations are open for the 12th CASSINI Hackathon!"),
            Some("12th CASSINI Hackathon".into())
        );
        assert_eq!(edition("About the CASSINI Hackathon"), None);
    }
}
