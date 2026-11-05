//! The rules that decide whether two listings are Duplicate listings (ADR 0008):
//! the same official link once normalised, or else the same start date, the
//! same city and closely similar names.

use std::collections::HashSet;

use chrono::{Days, NaiveDate};

use crate::candidate::Candidate;
use crate::sources::places;

/// Names scoring at least this are the same Hackathon (with the same start date and city).
pub const SAME_NAME: f64 = 0.8;

/// Names scoring at least this, but under [`SAME_NAME`], are unclear: the
/// Agent decides. Under it, the listings are different Hackathons.
pub const UNCLEAR_NAME: f64 = 0.5;

/// A shared link only counts when the listings' dates also overlap, allowing
/// this many days either side: a yearly event often keeps the same website,
/// and its editions are different Hackathons. Sources also disagree a little
/// on dates (Devpost lists a submission period, not the event days).
pub const LINK_DATE_SLACK_DAYS: u64 = 3;

/// Nor does a shared link count between listings in known cities further
/// apart than this: a link to an organiser's calendar can stand for several of
/// its events. A venue in the suburbs (Kongens Lyngby for Copenhagen) still counts.
pub const LINK_MAX_KM: f64 = 100.0;

/// Query parameters that only track where a click came from.
const TRACKING_PARAMS: &[&str] = &[
    "ref", "ref_src", "referrer", "source", "fbclid", "gclid", "dclid", "msclkid", "mc_cid",
    "mc_eid", "igshid", "si", "_hsenc", "_hsmi", "trk", "tk",
];

/// Words that say nothing about which Hackathon a name means.
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "of", "and", "x", "by", "at", "for", "in", "edition", "de", "la", "le", "du",
    "des",
];

/// Words almost every listing's name has; names are compared on the rest
/// when both have something left.
const GENERIC_WORDS: &[&str] = &[
    "hackathon",
    "hackathons",
    "hackaton",
    "hack",
    "hacks",
    "ai",
    "jam",
    "buildathon",
    "datathon",
];

/// What the rules say about two listings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    Same,
    /// Same start date and city, and names neither clearly alike nor clearly not.
    Unclear(f64),
    Different,
}

/// Compares two listings by the rules alone.
pub fn compare(a: &Candidate, b: &Candidate) -> Verdict {
    if let (Some(la), Some(lb)) = (link_key(a), link_key(b))
        && la == lb
        && dates_overlap(a, b)
        && !cities_far_apart(a, b)
    {
        return Verdict::Same;
    }
    if a.start == b.start
        && let (Some(ca), Some(cb)) = (&a.city, &b.city)
        && places::city_key(ca) == places::city_key(cb)
    {
        let score = name_similarity_in(&a.name, &b.name, &[ca, cb]);
        if score >= SAME_NAME {
            return Verdict::Same;
        }
        if score >= UNCLEAR_NAME {
            return Verdict::Unclear(score);
        }
    }
    Verdict::Different
}

/// The key a listing's link is compared by, if it has a usable link.
pub fn link_key(candidate: &Candidate) -> Option<String> {
    candidate.link.as_deref().and_then(normalise_link)
}

fn cities_far_apart(a: &Candidate, b: &Candidate) -> bool {
    match (&a.city, &b.city) {
        (Some(ca), Some(cb)) => places::far_apart(ca, cb, LINK_MAX_KM),
        _ => false,
    }
}

/// Whether the two listings' days overlap, give or take [`LINK_DATE_SLACK_DAYS`].
pub fn dates_overlap(a: &Candidate, b: &Candidate) -> bool {
    let widen = |d: NaiveDate| {
        d.checked_add_days(Days::new(LINK_DATE_SLACK_DAYS))
            .unwrap_or(d)
    };
    a.start <= widen(b.end) && b.start <= widen(a.end)
}

/// Normalises a link so different spellings of the same page compare equal:
/// the scheme, `www.`, letter case, tracking parameters (`utm_*`, `ref` and
/// the like), the fragment and trailing slashes don't count. A Luma event is
/// known by its slug whether linked on `lu.ma` or `luma.com`, and a Devpost
/// Hackathon by its subdomain whatever page of it is linked.
pub fn normalise_link(link: &str) -> Option<String> {
    let link = link.trim();
    let (scheme, rest) = link.split_once("://")?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return None;
    }
    let rest = rest.split('#').next().unwrap_or("");
    let (before_query, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (host, path) = before_query.split_once('/').unwrap_or((before_query, ""));
    let host = host.to_lowercase();
    let host = host.rsplit_once('@').map_or(host.as_str(), |(_, h)| h);
    let host = host
        .strip_suffix(":443")
        .or_else(|| host.strip_suffix(":80"))
        .unwrap_or(host);
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() {
        return None;
    }
    let path = path.trim_end_matches('/').to_lowercase();

    if host == "lu.ma" || host == "luma.com" {
        let slug = path.split('/').find(|s| !s.is_empty())?;
        // `luma.com/event/evt-…` is the same event as its slug, but nothing
        // else links that way, so it is kept whole.
        if slug == "event" {
            return Some(format!("luma.com/{path}"));
        }
        return Some(format!("luma.com/{slug}"));
    }
    if let Some(sub) = host.strip_suffix(".devpost.com")
        && !sub.is_empty()
        && sub != "help"
        && sub != "info"
    {
        return Some(format!("{sub}.devpost.com"));
    }

    let mut params: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            let name = p.split('=').next().unwrap_or("").to_ascii_lowercase();
            !name.starts_with("utm_") && !TRACKING_PARAMS.contains(&name.as_str())
        })
        .collect();
    params.sort_unstable();
    let mut key = host.to_owned();
    if !path.is_empty() {
        key.push('/');
        key.push_str(&path);
    }
    if !params.is_empty() {
        key.push('?');
        key.push_str(&params.join("&"));
    }
    Some(key)
}

/// The words of a name that identify it: folded, without stop words or years.
fn name_words(name: &str) -> Vec<String> {
    places::fold(name)
        .split_whitespace()
        .filter(|w| !STOP_WORDS.contains(w))
        .filter(|w| !is_year(w))
        .map(str::to_owned)
        .collect()
}

fn is_year(word: &str) -> bool {
    word.len() == 4
        && (word.starts_with("19") || word.starts_with("20"))
        && word.bytes().all(|b| b.is_ascii_digit())
}

/// How alike two names are, from 0 (nothing alike) to 1 (the same once normalised).
///
/// Names are folded (case, accents, punctuation) and lose stop words and
/// years. When both still have words besides generic ones such as
/// "hackathon" or "AI", only those are compared, and when every such word of
/// one name is in the other (`Since AI 2026` and `Since AI Hackathon 2026`)
/// the score is at least 0.9. Otherwise the score is the Sørensen–Dice
/// coefficient of the two names' letter pairs.
pub fn name_similarity(a: &str, b: &str) -> f64 {
    name_similarity_in(a, b, &[])
}

/// [`name_similarity`] for two listings in the same city: the city's own name
/// counts as a generic word, since `Paris Hack` and `Paris AI Days` share
/// nothing but their city.
pub fn name_similarity_in(a: &str, b: &str, cities: &[&str]) -> f64 {
    let city_words: Vec<String> = cities
        .iter()
        .flat_map(|c| [places::fold(c), places::city_key(c)])
        .flat_map(|c| c.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    let (wa, wb) = (name_words(a), name_words(b));
    let specific = |words: &[String]| -> Vec<String> {
        words
            .iter()
            .filter(|w| !GENERIC_WORDS.contains(&w.as_str()) && !city_words.contains(w))
            .cloned()
            .collect()
    };
    let (sa, sb) = (specific(&wa), specific(&wb));
    if sa.is_empty() || sb.is_empty() {
        // A name of generic words only, such as "AI Hackathon", is compared whole.
        if wa.is_empty() || wb.is_empty() {
            return if wa == wb { 1.0 } else { 0.0 };
        }
        return dice(&wa.concat(), &wb.concat());
    }
    let score = dice(&sa.concat(), &sb.concat());
    let (set_a, set_b): (HashSet<&String>, HashSet<&String>) =
        (sa.iter().collect(), sb.iter().collect());
    if set_a.is_subset(&set_b) || set_b.is_subset(&set_a) {
        score.max(0.9)
    } else {
        score
    }
}

/// The Sørensen–Dice coefficient of two strings' character bigrams.
fn dice(a: &str, b: &str) -> f64 {
    let bigrams = |s: &str| -> Vec<(char, char)> {
        let chars: Vec<char> = s.chars().collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (ba, mut bb) = (bigrams(a), bigrams(b));
    if ba.is_empty() || bb.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }
    let total = ba.len() + bb.len();
    let mut shared = 0;
    for pair in &ba {
        if let Some(i) = bb.iter().position(|p| p == pair) {
            bb.swap_remove(i);
            shared += 1;
        }
    }
    2.0 * shared as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(name: &str, start: (i32, u32, u32), city: &str, link: Option<&str>) -> Candidate {
        let start = NaiveDate::from_ymd_opt(start.0, start.1, start.2).unwrap();
        Candidate {
            source: "test".into(),
            source_key: name.into(),
            name: name.into(),
            start,
            end: start.checked_add_days(Days::new(1)).unwrap(),
            city: Some(city.into()),
            location: None,
            country: None,
            link: link.map(str::to_owned),
        }
    }

    #[test]
    fn links_lose_scheme_www_case_tracking_and_trailing_slashes() {
        let same = [
            "https://www.Example.org/Hack/",
            "http://example.org/hack",
            "https://example.org/hack?utm_source=newsletter&utm_medium=email",
            "https://example.org/hack/?ref=hackathonhub#apply",
            "https://example.org:443/hack//",
        ];
        for link in same {
            assert_eq!(
                normalise_link(link).as_deref(),
                Some("example.org/hack"),
                "{link}"
            );
        }
        assert_eq!(
            normalise_link("https://example.org/hack?id=7&utm_campaign=x&lang=en").as_deref(),
            Some("example.org/hack?id=7&lang=en")
        );
        assert_ne!(
            normalise_link("https://example.org/hack"),
            normalise_link("https://example.org/other")
        );
        assert_eq!(normalise_link("mailto:hi@example.org"), None);
        assert_eq!(normalise_link("not a link"), None);
    }

    #[test]
    fn luma_and_devpost_events_are_known_by_their_slug() {
        assert_eq!(
            normalise_link("https://lu.ma/bbh").as_deref(),
            Some("luma.com/bbh")
        );
        assert_eq!(
            normalise_link("https://luma.com/BBH?tk=abc").as_deref(),
            Some("luma.com/bbh")
        );
        assert_eq!(
            normalise_link("https://sinceai2026.devpost.com/").as_deref(),
            Some("sinceai2026.devpost.com")
        );
        assert_eq!(
            normalise_link("https://sinceai2026.devpost.com/rules?ref_content=x").as_deref(),
            Some("sinceai2026.devpost.com")
        );
    }

    #[test]
    fn similar_names_score_high_and_different_ones_low() {
        let cases = [
            (
                "{Tech: Europe} Agentic AI Hack",
                "Tech Europe Agentic AI Hack",
                1.0,
                1.0,
            ),
            ("Since AI 2026", "Since AI Hackathon 2026", 0.9, 1.0),
            ("Odoo Hackathon #6", "Odoo Hackathon 2026", 0.9, 1.0),
            ("Zürich Climate Hack", "Zurich Climate Hackathon", 1.0, 1.0),
            ("Hacking Health Camp", "Hacking Health Weekend", 0.5, 0.8),
            ("Lille Hack", "Ghent Hack", 0.0, 0.5),
            ("Hackathon", "Robotics Hackathon", 0.5, 0.8),
            ("Climate Hack", "Robotics Hack", 0.0, 0.5),
        ];
        for (a, b, low, high) in cases {
            let score = name_similarity(a, b);
            assert!(
                (low..=high).contains(&score) && (score < high || high == 1.0),
                "{a:?} vs {b:?} scored {score}, expected {low}..{high}"
            );
            assert_eq!(score, name_similarity(b, a), "the score is symmetric");
        }
    }

    #[test]
    fn the_same_link_with_overlapping_dates_is_the_same_hackathon() {
        let a = listing(
            "Big Berlin Hack",
            (2026, 11, 28),
            "Berlin",
            Some("https://luma.com/bbh"),
        );
        let b = listing(
            "BIG BERLIN HACK - 350+ Builders",
            (2026, 11, 28),
            "Berlin",
            Some("https://lu.ma/bbh?utm_source=x"),
        );
        assert_eq!(compare(&a, &b), Verdict::Same);

        // A link to an organiser's calendar, shared by its events in other cities.
        let munich = listing(
            "Data AI Summit Follow Up Hackathon",
            (2026, 11, 28),
            "Munich",
            Some("https://luma.com/bbh"),
        );
        assert_eq!(compare(&a, &munich), Verdict::Different);
        let suburb = listing(
            "Big Berlin Hack",
            (2026, 11, 28),
            "Potsdam",
            Some("https://luma.com/bbh"),
        );
        assert_eq!(compare(&a, &suburb), Verdict::Same);

        // Next year's edition keeps the website but is another Hackathon.
        let next_year = listing(
            "Big Berlin Hack",
            (2027, 11, 27),
            "Berlin",
            Some("https://luma.com/bbh"),
        );
        assert_eq!(compare(&a, &next_year), Verdict::Different);
    }

    #[test]
    fn without_a_shared_link_the_date_city_and_name_decide() {
        let hub = listing(
            "Since AI Hackathon 2026",
            (2026, 11, 6),
            "Turku",
            Some("https://sinceai.ai/hackathon"),
        );
        let devpost = listing(
            "Since AI 2026",
            (2026, 11, 6),
            "Åbo",
            Some("https://sinceai2026.devpost.com/"),
        );
        assert_eq!(compare(&hub, &devpost), Verdict::Same);

        let other_day = listing("Since AI 2026", (2026, 11, 7), "Turku", None);
        assert_eq!(compare(&hub, &other_day), Verdict::Different);

        let other_city = listing("Since AI 2026", (2026, 11, 6), "Helsinki", None);
        assert_eq!(compare(&hub, &other_city), Verdict::Different);

        let a = listing("Hacking Health Camp", (2027, 3, 12), "Liège", None);
        let b = listing("Hacking Health Weekend", (2027, 3, 12), "Luik", None);
        assert!(matches!(compare(&a, &b), Verdict::Unclear(_)));

        let c = listing("Climate Hack", (2027, 3, 12), "Liège", None);
        assert_eq!(compare(&a, &c), Verdict::Different);

        // The city's name in a listing's name doesn't count as a shared word,
        // but leaving it out doesn't make another name.
        let with_city = listing("Hacking Health Liège", (2027, 3, 12), "Liège", None);
        assert_eq!(compare(&a, &with_city), Verdict::Same);
        let paris = listing("Paris Hack", (2026, 12, 5), "Paris", None);
        let paris_days = listing("Paris AI Days", (2026, 12, 5), " paris ", None);
        assert_eq!(compare(&paris, &paris_days), Verdict::Different);
    }
}
