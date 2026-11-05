//! Polite fetching for Sources: the generic user agent (set by [`HttpClient`]),
//! a pause between requests to the same host, and `robots.txt` respected.
//!
//! One [`PoliteHttp`] is made per Run and shared by its Sources, so each host's
//! `robots.txt` is read at most once per Run.
//!
//! [`HttpClient`]: crate::http::HttpClient

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::http::HttpGet;

/// The product token `robots.txt` groups are matched against.
pub const ROBOTS_AGENT: &str = "hackathon-radar";

/// The default pause between two requests to the same host.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

/// The URL was not fetched because the host's `robots.txt` disallows it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{url} is disallowed by its site's robots.txt")]
pub struct Disallowed {
    pub url: String,
}

type Now = Box<dyn Fn() -> Instant>;
type Sleep = Box<dyn Fn(Duration)>;

/// Wraps an [`HttpGet`] to fetch politely.
pub struct PoliteHttp<H> {
    inner: H,
    interval: Duration,
    check_robots: bool,
    now: Now,
    sleep: Sleep,
    robots: RefCell<HashMap<String, Robots>>,
    last_request: RefCell<HashMap<String, Instant>>,
}

impl<H: HttpGet> PoliteHttp<H> {
    pub fn new(inner: H) -> Self {
        Self::with_clock(
            inner,
            DEFAULT_INTERVAL,
            Box::new(Instant::now),
            Box::new(std::thread::sleep),
        )
    }

    /// With a custom pause and clock, so tests can check the pacing without waiting.
    pub fn with_clock(inner: H, interval: Duration, now: Now, sleep: Sleep) -> Self {
        Self {
            inner,
            interval,
            check_robots: true,
            now,
            sleep,
            robots: RefCell::new(HashMap::new()),
            last_request: RefCell::new(HashMap::new()),
        }
    }

    /// For URLs the person added themselves, such as their own calendar feeds:
    /// those are fetched on their behalf like a calendar app would, so
    /// `robots.txt`, which addresses crawlers, isn't consulted. Pacing still applies.
    pub fn ignoring_robots(mut self) -> Self {
        self.check_robots = false;
        self
    }

    /// Fetches with the pause, without consulting `robots.txt`.
    fn paced_get(&self, origin: &str, url: &str) -> anyhow::Result<String> {
        let last = self.last_request.borrow().get(origin).copied();
        if let Some(last) = last {
            let waited = (self.now)().saturating_duration_since(last);
            if waited < self.interval {
                (self.sleep)(self.interval - waited);
            }
        }
        let result = self.inner.get_text(url);
        self.last_request
            .borrow_mut()
            .insert(origin.to_owned(), (self.now)());
        result
    }

    fn allowed(&self, origin: &str, path: &str) -> bool {
        if !self.robots.borrow().contains_key(origin) {
            // A missing or unreadable robots.txt allows everything.
            let robots = self
                .paced_get(origin, &format!("{origin}/robots.txt"))
                .map(|text| Robots::parse(&text, ROBOTS_AGENT))
                .unwrap_or_default();
            self.robots.borrow_mut().insert(origin.to_owned(), robots);
        }
        self.robots.borrow()[origin].allows(path)
    }
}

impl<H: HttpGet> HttpGet for PoliteHttp<H> {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        let Some((origin, path)) = split_url(url) else {
            return self.inner.get_text(url);
        };
        if self.check_robots && !self.allowed(&origin, &path) {
            return Err(Disallowed {
                url: url.to_owned(),
            }
            .into());
        }
        self.paced_get(&origin, url)
    }
}

/// `https://Example.org:8443/a/b?c` becomes (`https://example.org:8443`, `/a/b?c`).
fn split_url(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    if host.is_empty() {
        return None;
    }
    let mut path = rest[end..].split('#').next().unwrap_or("").to_owned();
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    Some((
        format!(
            "{}://{}",
            scheme.to_ascii_lowercase(),
            host.to_ascii_lowercase()
        ),
        path,
    ))
}

/// The `Allow` and `Disallow` rules of the `robots.txt` group that applies to us.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Robots {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    allow: bool,
    pattern: String,
}

impl Robots {
    /// Reads a `robots.txt` (RFC 9309) for the given product token: the group
    /// naming it if there is one, otherwise the `*` group.
    pub fn parse(text: &str, agent: &str) -> Self {
        let agent = agent.to_ascii_lowercase();
        let mut groups: Vec<(Vec<String>, Vec<Rule>)> = Vec::new();
        // Consecutive User-agent lines share one group.
        let mut collecting_agents = false;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            match key.as_str() {
                "user-agent" => {
                    if !collecting_agents {
                        groups.push((Vec::new(), Vec::new()));
                        collecting_agents = true;
                    }
                    if let Some(group) = groups.last_mut() {
                        group.0.push(value.to_ascii_lowercase());
                    }
                }
                "allow" | "disallow" => {
                    collecting_agents = false;
                    if let Some(group) = groups.last_mut()
                        && !value.is_empty()
                    {
                        group.1.push(Rule {
                            allow: key == "allow",
                            pattern: value.to_owned(),
                        });
                    }
                }
                _ => {}
            }
        }
        let names_us = |agents: &Vec<String>| agents.contains(&agent);
        let applies: fn(&Vec<String>) -> bool = |agents| agents.iter().any(|a| a == "*");
        let ours = groups.iter().any(|(agents, _)| names_us(agents));
        let rules = groups
            .iter()
            .filter(|(agents, _)| {
                if ours {
                    names_us(agents)
                } else {
                    applies(agents)
                }
            })
            .flat_map(|(_, rules)| rules.clone())
            .collect();
        Self { rules }
    }

    /// Whether `path` (with its query) may be fetched: the longest matching rule
    /// decides, and `Allow` wins a tie. No matching rule allows it.
    pub fn allows(&self, path: &str) -> bool {
        self.rules
            .iter()
            .filter(|rule| pattern_matches(&rule.pattern, path))
            .max_by_key(|rule| (rule.pattern.len(), rule.allow))
            .is_none_or(|rule| rule.allow)
    }
}

/// Matches a rule's path pattern, where `*` stands for any characters and a
/// final `$` anchors the end.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let pieces: Vec<&str> = pattern.split('*').collect();
    let Some(first) = pieces.first() else {
        return true;
    };
    let Some(mut rest) = path.strip_prefix(first) else {
        return false;
    };
    for (i, piece) in pieces.iter().enumerate().skip(1) {
        let last = i == pieces.len() - 1;
        if last && anchored {
            return rest.ends_with(piece);
        }
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    !anchored || rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_match_prefixes_wildcards_and_anchors() {
        assert!(pattern_matches("/admin", "/admin/users"));
        assert!(!pattern_matches("/admin", "/events/x"));
        assert!(pattern_matches("/*.ics$", "/cal/feed.ics"));
        assert!(!pattern_matches("/*.ics$", "/cal/feed.ics?x=1"));
        assert!(pattern_matches("/events/*/private", "/events/a/private/b"));
        assert!(pattern_matches("/exact$", "/exact"));
        assert!(!pattern_matches("/exact$", "/exact/more"));
    }

    #[test]
    fn the_longest_rule_wins_and_allow_wins_ties() {
        let robots = Robots::parse(
            "User-agent: *\nDisallow: /events\nAllow: /events/public\nDisallow: /x\nAllow: /x\n",
            ROBOTS_AGENT,
        );
        assert!(!robots.allows("/events/secret"));
        assert!(robots.allows("/events/public/1"));
        assert!(robots.allows("/x"));
        assert!(robots.allows("/other"));
    }

    #[test]
    fn a_group_naming_us_replaces_the_star_group() {
        let text =
            "User-agent: *\nDisallow: /\n\nUser-agent: Hackathon-Radar\nDisallow: /private\n";
        let robots = Robots::parse(text, ROBOTS_AGENT);
        assert!(robots.allows("/events"));
        assert!(!robots.allows("/private/x"));
    }

    #[test]
    fn groups_for_other_agents_are_ignored_and_empty_disallow_allows_all() {
        let text =
            "User-agent: BadBot\nUser-agent: OtherBot\nDisallow: /\n\nUser-agent: *\nDisallow:\n";
        let robots = Robots::parse(text, ROBOTS_AGENT);
        assert!(robots.allows("/anything"));
    }

    #[test]
    fn urls_split_into_origin_and_path() {
        assert_eq!(
            split_url("https://Example.org/a/b?c=1#frag"),
            Some(("https://example.org".into(), "/a/b?c=1".into()))
        );
        assert_eq!(
            split_url("https://example.org?x"),
            Some(("https://example.org".into(), "/?x".into()))
        );
        assert_eq!(split_url("not a url"), None);
    }
}
