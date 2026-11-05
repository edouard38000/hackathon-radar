//! hackathon-radar: a self-hosted tracker that finds Hackathons that Fit a Profile.
//!
//! A [`run`] collects Candidates from each [`sources::Source`], applies the
//! Profile [`rules`] checkable from data alone, has the person's [`agent`] judge
//! the rest ([`judging`]), keeps the result in the local [`store`] and composes
//! the [`alert`] for the person.

pub mod agent;
pub mod alert;
pub mod candidate;
pub mod clock;
pub mod http;
pub mod ics;
pub mod judging;
pub mod paths;
pub mod process;
pub mod profile;
pub mod rules;
pub mod run;
pub mod scheduler;
pub mod sources;
pub mod store;
