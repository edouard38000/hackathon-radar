//! Matching: merging Duplicate listings into one Hackathon (ADR 0008).
//!
//! The same Hackathon is often listed on several Sources. Each Run compares
//! the listings it collected with each other and with the listings stored by
//! earlier Runs, by the rules in [`compare`]. Pairs the rules can't settle go
//! to the Agent as a small separate question ([`question`]); its answers are
//! stored, so each pair is only asked once. Until a pair is settled, both of
//! its Hackathons wait, so the same Hackathon is never announced twice.
//!
//! Every listing is stored with the Hackathon it belongs to. A Hackathon
//! carries the identity (Source and key) of one of its listings, which stays
//! the same across Runs, so the rest of the Run (rules, judging, Alerts,
//! Announced) works on one Candidate per Hackathon.

pub mod compare;
pub mod question;

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{Days, NaiveDate};

use crate::agent::Agent;
use crate::candidate::Candidate;
use crate::profile::Profile;
use crate::rules;
use crate::sources::places;
use crate::store::{Listing, ListingKey, Store, key_of};
use compare::Verdict;

/// The most unclear pairs sent to the Agent on one Run; the rest wait for the next.
pub const MAX_PAIRS_PER_RUN: usize = 10;

/// Stored listings that ended longer ago than this take no part in matching.
const STORED_LISTING_DAYS: u64 = 30;

#[derive(Debug, Default)]
pub struct Merged {
    /// One Candidate per Hackathon found this Run, carrying the Hackathon's identity.
    pub hackathons: Vec<Candidate>,
    /// How many collected listings were Duplicate listings of another.
    pub duplicates: usize,
    /// How many unclear pairs the Agent answered on this Run.
    pub answered: usize,
    /// How many Hackathons wait for the Agent to settle an unclear pair.
    pub waiting: usize,
    /// Why the Agent couldn't answer this Run, if it couldn't.
    pub failure: Option<String>,
}

/// A listing taking part in matching: collected this Run, stored before, or both.
struct Node {
    candidate: Candidate,
    stored: Option<Listing>,
    found: bool,
}

/// Merges the listings `found` this Run into Hackathons, stores every listing
/// with its Hackathon, and returns one Candidate per Hackathon.
pub fn merge(
    agent: &dyn Agent,
    profile: &Profile,
    store: &mut Store,
    found: Vec<Candidate>,
    today: NaiveDate,
) -> anyhow::Result<Merged> {
    let mut seen = HashSet::new();
    let found: Vec<Candidate> = found
        .into_iter()
        .filter(|c| seen.insert(key_of(c)))
        .collect();

    let stored: HashMap<ListingKey, Listing> = store
        .listings()?
        .into_iter()
        .map(|l| (l.key(), l))
        .collect();
    let nodes = nodes(found, &stored, today);
    let mut groups = UnionFind::new(nodes.len());

    // Listings stored as one Hackathon stay one.
    let mut by_hackathon: HashMap<&ListingKey, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        if let Some(listing) = &node.stored {
            match by_hackathon.get(&listing.hackathon) {
                Some(&first) => groups.union(first, i),
                None => {
                    by_hackathon.insert(&listing.hackathon, i);
                }
            }
        }
    }

    let mut unclear = Vec::new();
    for (i, j) in comparable_pairs(&nodes) {
        match compare::compare(&nodes[i].candidate, &nodes[j].candidate) {
            Verdict::Same => groups.union(i, j),
            Verdict::Unclear(_) => {
                match store.same_hackathon(&nodes[i].candidate, &nodes[j].candidate)? {
                    Some(true) => groups.union(i, j),
                    Some(false) => {}
                    None => unclear.push((i, j)),
                }
            }
            Verdict::Different => {}
        }
    }

    // Ask the Agent about unclear pairs that matter: both sides could Fit.
    let mut asked = Vec::new();
    let mut unsettled = Vec::new();
    for (i, j) in unclear {
        if groups.find(i) == groups.find(j) {
            continue;
        }
        let could_fit = |n: usize| {
            let hackathon = merged_candidate(&nodes, &groups.members(n), &stored);
            !rules::apply(vec![hackathon], profile, today).is_empty()
        };
        if !(could_fit(i) && could_fit(j)) {
            continue;
        }
        if asked.len() < MAX_PAIRS_PER_RUN {
            asked.push((i, j));
        } else {
            unsettled.push((i, j));
        }
    }
    let mut answered = 0;
    let mut failure = None;
    if !asked.is_empty() {
        let pairs: Vec<(Candidate, Candidate)> = asked
            .iter()
            .map(|&(i, j)| (nodes[i].candidate.clone(), nodes[j].candidate.clone()))
            .collect();
        let ids = question::Ids::new(pairs.len());
        let request = question::request(&pairs, &ids, profile);
        let answers = agent
            .judge(&request)
            .map_err(|e| format!("{e:#}"))
            .and_then(|answer| question::read_answers(&ids, &answer).map_err(|e| e.to_string()));
        match answers {
            Ok(answers) => {
                let mut settled = HashSet::new();
                let rows: Vec<(&Candidate, &Candidate, bool, &str)> = answers
                    .iter()
                    .map(|a| {
                        let (x, y) = &pairs[a.pair];
                        (x, y, a.same, a.reason.as_str())
                    })
                    .collect();
                store.save_same_hackathon_answers(agent.name(), &rows, today)?;
                for a in &answers {
                    settled.insert(a.pair);
                    if a.same {
                        let (i, j) = asked[a.pair];
                        groups.union(i, j);
                    }
                }
                answered = answers.len();
                unsettled.extend(
                    asked
                        .iter()
                        .enumerate()
                        .filter(|(n, _)| !settled.contains(n))
                        .map(|(_, &pair)| pair),
                );
            }
            Err(error) => {
                failure = Some(format!(
                    "the Agent couldn't say whether listings are the same Hackathon: {error}"
                ));
                unsettled.extend(asked.iter().copied());
            }
        }
    }
    let waiting_roots: HashSet<usize> = unsettled
        .iter()
        .filter(|&&(i, j)| groups.find(i) != groups.find(j))
        .flat_map(|&(i, j)| [groups.find(i), groups.find(j)])
        .collect();

    // Each group with a listing found this Run is one Hackathon, in the order
    // its first listing was found.
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let root = groups.find(i);
        if node.found && !by_root.contains_key(&root) {
            roots.push(root);
        }
        by_root.entry(root).or_default().push(i);
    }
    let mut merged = Merged {
        answered,
        failure,
        ..Merged::default()
    };
    let mut to_save: Vec<(&Candidate, ListingKey)> = Vec::new();
    for root in roots {
        let members = &by_root[&root];
        let primary = primary(&nodes, members, &stored);
        let earlier: BTreeSet<&ListingKey> = members
            .iter()
            .filter_map(|&m| nodes[m].stored.as_ref().map(|l| &l.hackathon))
            .collect();
        for key in earlier {
            store.merge_hackathon(key, &primary)?;
        }
        let found_members: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&m| nodes[m].found)
            .collect();
        for &m in &found_members {
            to_save.push((&nodes[m].candidate, primary.clone()));
        }
        if waiting_roots.contains(&root) {
            merged.waiting += 1;
            continue;
        }
        merged.duplicates += found_members.len() - 1;
        merged
            .hackathons
            .push(merged_candidate(&nodes, members, &stored));
    }
    store.save_listings(&to_save, today)?;
    Ok(merged)
}

/// The listings found this Run, then the stored ones that can still match:
/// those not long over, and every Hackathon a found listing already belongs to.
fn nodes(
    found: Vec<Candidate>,
    stored: &HashMap<ListingKey, Listing>,
    today: NaiveDate,
) -> Vec<Node> {
    let cutoff = today
        .checked_sub_days(Days::new(STORED_LISTING_DAYS))
        .unwrap_or(today);
    let found_keys: HashSet<ListingKey> = found.iter().map(key_of).collect();
    let wanted: HashSet<&ListingKey> = found_keys
        .iter()
        .filter_map(|k| stored.get(k).map(|l| &l.hackathon))
        .collect();
    let mut nodes: Vec<Node> = found
        .into_iter()
        .map(|candidate| Node {
            stored: stored.get(&key_of(&candidate)).cloned(),
            candidate,
            found: true,
        })
        .collect();
    let mut rest: Vec<&Listing> = stored
        .values()
        .filter(|l| !found_keys.contains(&l.key()))
        .filter(|l| {
            l.candidate.end >= cutoff || wanted.contains(&l.hackathon) || wanted.contains(&l.key())
        })
        .collect();
    rest.sort_by_key(|l| l.key());
    nodes.extend(rest.into_iter().map(|l| Node {
        candidate: l.candidate.clone(),
        stored: Some(l.clone()),
        found: false,
    }));
    nodes
}

/// The pairs worth comparing: sharing a link, or a start date and a city, with
/// at least one side found this Run.
fn comparable_pairs(nodes: &[Node]) -> Vec<(usize, usize)> {
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let c = &node.candidate;
        if let Some(link) = compare::link_key(c) {
            buckets.entry(format!("link {link}")).or_default().push(i);
        }
        if let Some(city) = &c.city {
            buckets
                .entry(format!("day {} {}", c.start, places::city_key(city)))
                .or_default()
                .push(i);
        }
    }
    let mut pairs = BTreeSet::new();
    for members in buckets.values() {
        for (n, &i) in members.iter().enumerate() {
            for &j in &members[n + 1..] {
                if nodes[i].found || nodes[j].found {
                    pairs.insert((i.min(j), i.max(j)));
                }
            }
        }
    }
    pairs.into_iter().collect()
}

/// The listing a group's Hackathon is named by. A Hackathon stored by an
/// earlier Run keeps its identity (the earliest one, if several are now
/// merged); a new one is named by its most complete listing, the first
/// Source's on a tie.
fn primary(nodes: &[Node], members: &[usize], stored: &HashMap<ListingKey, Listing>) -> ListingKey {
    let earlier = members
        .iter()
        .filter_map(|&m| nodes[m].stored.as_ref().map(|l| &l.hackathon))
        .min_by_key(|key| (stored.get(*key).map(|l| l.first_seen), (*key).clone()));
    if let Some(key) = earlier {
        return key.clone();
    }
    let completeness = |c: &Candidate| {
        4 * usize::from(c.country.is_some())
            + 2 * usize::from(c.city.is_some())
            + usize::from(c.link.is_some())
    };
    let best = members
        .iter()
        .copied()
        .max_by_key(|&m| (completeness(&nodes[m].candidate), std::cmp::Reverse(m)))
        .expect("a group has a listing");
    key_of(&nodes[best].candidate)
}

/// The group's Hackathon: its primary listing, with what that listing lacks
/// (city, country, location, link) taken from the group's other listings.
fn merged_candidate(
    nodes: &[Node],
    members: &[usize],
    stored: &HashMap<ListingKey, Listing>,
) -> Candidate {
    let key = primary(nodes, members, stored);
    let mut hackathon = members
        .iter()
        .map(|&m| &nodes[m].candidate)
        .find(|c| key_of(c) == key)
        .or_else(|| stored.get(&key).map(|l| &l.candidate))
        .cloned()
        .unwrap_or_else(|| nodes[members[0]].candidate.clone());
    hackathon.source = key.0;
    hackathon.source_key = key.1;
    for &m in members {
        let other = &nodes[m].candidate;
        if hackathon.city.is_none() {
            hackathon.city.clone_from(&other.city);
        }
        if hackathon.country.is_none() {
            hackathon.country.clone_from(&other.country);
        }
        if hackathon.location.is_none() {
            hackathon.location.clone_from(&other.location);
        }
        if hackathon.link.is_none() {
            hackathon.link.clone_from(&other.link);
        }
    }
    hackathon
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&self, mut i: usize) -> usize {
        while self.parent[i] != i {
            i = self.parent[i];
        }
        i
    }

    /// Joins two groups; the lower index becomes the root, so roots follow
    /// the order listings were found in.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (low, high) = (ra.min(rb), ra.max(rb));
            self.parent[high] = low;
        }
    }

    /// The members of `i`'s group, in order.
    fn members(&self, i: usize) -> Vec<usize> {
        let root = self.find(i);
        (0..self.parent.len())
            .filter(|&n| self.find(n) == root)
            .collect()
    }
}
