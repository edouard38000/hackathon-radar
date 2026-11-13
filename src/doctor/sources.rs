//! Sources: one light request to each Source a Run reads (its first page),
//! through the same polite client a Run uses. A Source that doesn't answer
//! is a warning, since sites go down for a while.

use chrono::NaiveDate;

use super::{Check, Section, fetch_error};
use crate::http::HttpGet;
use crate::profile::Profile;
use crate::sources::packs::{self, SourceSpec};
use crate::sources::{cassini, dev_events, devpost, ethglobal, hackathon_be, hackathonhub, mlh};

const S: Section = Section::Sources;

/// One Source's light request.
struct Probe {
    id: String,
    name: String,
    url: String,
    /// The person's own calendar: fetched without robots.txt, and its URL,
    /// which can be private, stays out of the output.
    own: bool,
}

fn probes(profile: &Profile, today: NaiveDate) -> Vec<Probe> {
    let mut luma_count = 0;
    let mut ics_count = 0;
    packs::plan(profile)
        .into_iter()
        .map(|spec| {
            let public = |id: &str, name: &str, url: &str| Probe {
                id: format!("source.{id}"),
                name: name.to_owned(),
                url: url.to_owned(),
                own: false,
            };
            match spec {
                SourceSpec::Devpost => public("devpost", "Devpost", devpost::API_URL),
                SourceSpec::Mlh => public(
                    "mlh",
                    "MLH",
                    &mlh::page_url(mlh::season(today), mlh::REGIONS[0], 1),
                ),
                SourceSpec::EthGlobal => public("ethglobal", "ETHGlobal", ethglobal::EVENTS_URL),
                SourceSpec::Cassini => public("cassini", "CASSINI Hackathons", cassini::PAGE_URL),
                SourceSpec::DevEvents => public("dev-events", "dev.events", dev_events::FEED_URL),
                SourceSpec::HackathonBe => {
                    public("hackathon-be", "hackathon.be", hackathon_be::CALENDAR_URL)
                }
                SourceSpec::HackathonHub => {
                    public("hackathonhub", "hackathonhub.eu", hackathonhub::SITEMAP_URL)
                }
                SourceSpec::Luma(url) => {
                    luma_count += 1;
                    let calendar = url
                        .split_once("id=")
                        .map_or(url, |(_, id)| id.split('&').next().unwrap_or(id));
                    public(
                        &format!("luma.{luma_count}"),
                        &format!("Luma calendar {calendar}"),
                        url,
                    )
                }
                SourceSpec::Ics(url) => {
                    ics_count += 1;
                    Probe {
                        id: format!("source.ics.{ics_count}"),
                        name: format!("Your calendar on {}", host(&url)),
                        url,
                        own: true,
                    }
                }
            }
        })
        .collect()
}

/// The host of a URL, which is all that is shown of the person's calendars.
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    // Leave out any user:password@ before the host.
    host.rsplit_once('@').map_or(host, |(_, host)| host)
}

pub(super) fn check(
    profile: &Profile,
    sources_http: &dyn HttpGet,
    calendars_http: &dyn HttpGet,
    today: NaiveDate,
    checks: &mut Vec<Check>,
) {
    for probe in probes(profile, today) {
        let http = if probe.own {
            calendars_http
        } else {
            sources_http
        };
        let shown = if probe.own {
            "it".to_owned()
        } else {
            probe.url.clone()
        };
        checks.push(match http.get_text(&probe.url) {
            Ok(_) => Check::pass(S, probe.id, probe.name, format!("Reachable ({shown}).")),
            Err(error) => Check::warn(
                S,
                probe.id,
                probe.name,
                format!(
                    "Couldn't fetch {shown}: {}.",
                    fetch_error(&error, &probe.url, probe.own)
                ),
                "Nothing to do if the site is down for a while; if it lasts, check your \
                 connection, or the Source's address if you added it yourself.",
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_host_of_a_calendar_is_shown() {
        assert_eq!(
            host("https://calendar.google.com/calendar/ical/private-abc/basic.ics"),
            "calendar.google.com"
        );
        assert_eq!(
            host("https://me:secret@cal.example.org/x.ics"),
            "cal.example.org"
        );
    }
}
