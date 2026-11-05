//! hackathon.be, which lists Hackathons in Belgium as an ICS calendar.

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::ics;
use crate::sources::Source;

pub const NAME: &str = "hackathon.be";
pub const CALENDAR_URL: &str = "https://hackathon.be/calendar.ics";

/// Every listing is in Belgium unless its location ends with another country.
const DEFAULT_COUNTRY: &str = "Belgium";

/// Country names a location may end with, in the languages used around Belgium.
const COUNTRY_NAMES: &[(&str, &str)] = &[
    ("belgium", "Belgium"),
    ("belgië", "Belgium"),
    ("belgie", "Belgium"),
    ("belgique", "Belgium"),
    ("belgien", "Belgium"),
    ("france", "France"),
    ("netherlands", "Netherlands"),
    ("the netherlands", "Netherlands"),
    ("nederland", "Netherlands"),
    ("luxembourg", "Luxembourg"),
    ("germany", "Germany"),
    ("deutschland", "Germany"),
];

pub struct HackathonBe<H> {
    http: H,
}

impl<H: HttpGet> HackathonBe<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }
}

impl<H: HttpGet> Source for HackathonBe<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        Ok(parse(&self.http.get_text(CALENDAR_URL)?))
    }
}

/// Turns the calendar into Candidates. Events without a name or a start date are skipped.
pub fn parse(calendar: &str) -> Vec<Candidate> {
    ics::parse_events(calendar)
        .iter()
        .filter_map(to_candidate)
        .collect()
}

fn to_candidate(event: &ics::Event) -> Option<Candidate> {
    let name = event.text("SUMMARY")?;
    let start = event.start_date()?;
    let end = event.end_date()?;
    let link = event.text("URL");
    let location = event.text("LOCATION");
    let (city, country) = match &location {
        Some(location) => place(location),
        None => (None, DEFAULT_COUNTRY.to_owned()),
    };
    let source_key = event
        .text("UID")
        .or_else(|| link.clone())
        .unwrap_or_else(|| format!("{name}@{start}"));
    Some(Candidate {
        source: NAME.to_owned(),
        source_key,
        name,
        start,
        end,
        city,
        location,
        country: Some(country),
        link,
    })
}

/// Reads the city and country from a location such as `Howest, 8500 Kortrijk`:
/// the city is the last part, without its postcode.
fn place(location: &str) -> (Option<String>, String) {
    let mut parts: Vec<&str> = location
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let country = match parts.last().and_then(|last| country_named(last)) {
        Some(country) => {
            parts.pop();
            country
        }
        None => DEFAULT_COUNTRY,
    };
    let city = parts
        .last()
        .map(|last| strip_postcode(last))
        .filter(|city| !city.is_empty());
    (city, country.to_owned())
}

fn country_named(part: &str) -> Option<&'static str> {
    let part = part.to_lowercase();
    COUNTRY_NAMES
        .iter()
        .find(|(name, _)| *name == part)
        .map(|(_, country)| *country)
}

/// `8500 Kortrijk` and `B-8500 Kortrijk` both become `Kortrijk`.
fn strip_postcode(part: &str) -> String {
    let is_postcode = |word: &&str| {
        let digits = word.strip_prefix("B-").unwrap_or(word);
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    };
    part.split_whitespace()
        .filter(|w| !is_postcode(w))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_is_the_last_part_without_postcode() {
        let cases = [
            ("Bruges", Some("Bruges"), "Belgium"),
            (
                "9 Rue des Bourlottes, 1367 Ramillies",
                Some("Ramillies"),
                "Belgium",
            ),
            ("Howest, 8500 Kortrijk", Some("Kortrijk"), "Belgium"),
            ("Campus, B-9000 Gent", Some("Gent"), "Belgium"),
            ("Leuven 3000", Some("Leuven"), "Belgium"),
            ("Place, Brussels, Belgium", Some("Brussels"), "Belgium"),
            ("EuraTechnologies, Lille, France", Some("Lille"), "France"),
            ("Maastricht, Nederland", Some("Maastricht"), "Netherlands"),
            ("Belgium", None, "Belgium"),
        ];
        for (location, city, country) in cases {
            assert_eq!(
                place(location),
                (city.map(str::to_owned), country.to_owned()),
                "{location}"
            );
        }
    }
}
