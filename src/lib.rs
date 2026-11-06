//! hackathon-radar: a self-hosted tracker that finds Hackathons that Fit a Profile.
//!
//! A [`run`] collects Candidates from each [`sources::Source`], merges
//! Duplicate listings ([`matching`]), applies the Profile [`rules`] checkable
//! from data alone, has the person's [`agent`] judge the rest ([`judging`]),
//! works out how to get to each Hackathon ([`travel`]), keeps deadlines and
//! status current ([`deadlines`]), keeps the result in the local [`store`] and
//! composes the [`alert`] for the person.

pub mod agent;
pub mod alert;
pub mod candidate;
pub mod clock;
pub mod deadlines;
pub mod http;
pub mod ics;
pub mod judging;
pub mod matching;
pub mod paths;
pub mod process;
pub mod profile;
pub mod rules;
pub mod run;
pub mod scheduler;
pub mod sources;
pub mod store;
pub mod travel;
