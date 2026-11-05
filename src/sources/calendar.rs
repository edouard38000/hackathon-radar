//! Calendar feeds (ICS): the ones the person adds to their Profile, and the
//! shared reading of a calendar event into a Candidate that Luma uses too.

use crate::candidate::Candidate;
use crate::http::HttpGet;
use crate::ics;
use crate::sources::{Source, places};

/// The Source name of the person's own calendar feeds.
pub const NAME: &str = "ics";

/// One calendar feed the person added to their Profile. Every event in it is
/// a Candidate; the Agent decides which are Hackathons.
pub struct IcsCalendar<H> {
    http: H,
    url: String,
}

impl<H: HttpGet> IcsCalendar<H> {
    pub fn new(http: H, url: impl Into<String>) -> Self {
        Self {
            http,
            url: url.into(),
        }
    }
}

impl<H: HttpGet> Source for IcsCalendar<H> {
    fn name(&self) -> &str {
        NAME
    }

    fn fetch(&self) -> anyhow::Result<Vec<Candidate>> {
        Ok(parse(NAME, &self.http.get_text(&self.url)?))
    }
}

/// Every event of a calendar with a name and a start date, as Candidates of `source`.
pub fn parse(source: &str, calendar: &str) -> Vec<Candidate> {
    ics::parse_events(calendar)
        .iter()
        .filter_map(|event| to_candidate(source, event))
        .collect()
}

/// Reads one calendar event. The link is its URL; or else a link given as its
/// LOCATION, which Luma does for events featured from other sites (their
/// description only links to the calendar); or else the first link in its
/// description. The city and country come from its address (LOCATION, or an
/// `Address:` block in the description), or else from its GEO coordinates.
pub fn to_candidate(source: &str, event: &ics::Event) -> Option<Candidate> {
    let name = event.text("SUMMARY")?;
    let start = event.start_date()?;
    let end = event.end_date()?;
    let description = event.text("DESCRIPTION");
    let location_link = event
        .text("LOCATION")
        .filter(|l| is_link(l) && !is_luma_event_page(l));
    let link = event
        .text("URL")
        .or(location_link)
        .or_else(|| description.as_deref().and_then(first_link));
    // Some calendars put a link where the address goes, when the address is hidden.
    let location = event.text("LOCATION").filter(|l| !is_link(l));
    let address = location
        .clone()
        .or_else(|| description.as_deref().and_then(address_block));
    let (mut city, mut country) = address
        .as_deref()
        .map(places::from_address)
        .unwrap_or((None, None));
    if (city.is_none() || country.is_none())
        && let Some((near_city, near_country)) =
            geo(event).and_then(|(lat, lon)| places::nearest_city(lat, lon))
    {
        if city.is_none() {
            city = Some(near_city.to_owned());
        }
        if country.is_none() {
            country = Some(near_country.to_owned());
        }
    }
    let source_key = event
        .text("UID")
        .or_else(|| link.clone())
        .unwrap_or_else(|| format!("{name}@{start}"));
    Some(Candidate {
        source: source.to_owned(),
        source_key,
        name,
        start,
        end,
        city,
        location,
        country,
        link,
    })
}

fn is_link(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

/// `https://luma.com/event/evt-…`, which Luma gives as the LOCATION of its own
/// events with a hidden address; the description has the event's real page.
fn is_luma_event_page(link: &str) -> bool {
    ["https://luma.com/event/", "https://lu.ma/event/"]
        .iter()
        .any(|prefix| link.starts_with(prefix))
}

/// The first `http(s)://` link in a text.
fn first_link(text: &str) -> Option<String> {
    let at = text.find("https://").or_else(|| text.find("http://"))?;
    let link: String = text[at..]
        .chars()
        .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '<' | '>' | ')'))
        .collect();
    Some(link.trim_end_matches(['.', ',', ';']).to_owned())
}

/// The lines after `Address:` in a description, up to the next blank line,
/// as Luma writes them; `None` when the address is hidden.
fn address_block(description: &str) -> Option<String> {
    let (_, after) = description.split_once("Address:")?;
    let lines: Vec<&str> = after
        .trim_start_matches(['\n', ' '])
        .lines()
        .map(str::trim)
        .take_while(|l| !l.is_empty())
        .collect();
    let text = lines.join("\n");
    (!text.is_empty() && !text.starts_with("Check event page")).then_some(text)
}

/// GEO is `latitude;longitude` (RFC 5545 3.8.1.6).
fn geo(event: &ics::Event) -> Option<(f64, f64)> {
    let value = &event.property("GEO")?.value;
    let (lat, lon) = value.split_once(';')?;
    Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_addresses_come_out_of_descriptions() {
        assert_eq!(
            first_link("Get up-to-date information at: https://luma.com/bbh\n\nAddress:"),
            Some("https://luma.com/bbh".into())
        );
        assert_eq!(first_link("no link"), None);
        assert_eq!(
            address_block("Info\n\nAddress:\nThe Delta\nBerlin, Germany\n\nHosted by X"),
            Some("The Delta\nBerlin, Germany".into())
        );
        assert_eq!(
            address_block("Address:\nCheck event page for more details.\n\nHosted by X"),
            None
        );
        assert_eq!(address_block("Hosted by X"), None);
    }

    #[test]
    fn a_featured_event_links_to_its_own_site_not_the_calendar() {
        let feed = "BEGIN:VEVENT\nUID:calev-lxjK1csF4shBDpO@events.lu.ma\n\
            DTSTART:20270320T070000Z\nSUMMARY:Hack HPI - Health Edition\n\
            DESCRIPTION:Find more information on https://luma.com/tech-europe\n\
            LOCATION:https://hackhpi.org/\nGEO:52.394075;13.132506\nEND:VEVENT\n";
        let event = &ics::parse_events(feed)[0];
        let candidate = to_candidate("luma", event).unwrap();
        assert_eq!(candidate.link.as_deref(), Some("https://hackhpi.org/"));
        assert_eq!(candidate.location, None);
        assert_eq!(candidate.city.as_deref(), Some("Potsdam"));
    }
}
