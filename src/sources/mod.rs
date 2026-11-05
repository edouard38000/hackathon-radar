//! Sources: places hackathon listings are collected from. Each one turns its
//! listings into Candidates behind the same small interface.

pub mod hackathon_be;

use crate::candidate::Candidate;

pub trait Source {
    /// A short, stable name, stored with each Candidate.
    fn name(&self) -> &str;

    /// Collects the Source's current listings as Candidates.
    fn fetch(&self) -> anyhow::Result<Vec<Candidate>>;
}
