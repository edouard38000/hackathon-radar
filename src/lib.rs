//! hackathon-radar: a self-hosted tracker that finds Hackathons that Fit a Profile.
//!
//! A [`run`] collects Candidates from each [`sources::Source`], applies the
//! Profile [`rules`] checkable from data alone, keeps the result in the local
//! [`store`] and composes the [`alert`] for the person.

pub mod alert;
pub mod candidate;
pub mod clock;
pub mod http;
pub mod ics;
pub mod paths;
pub mod profile;
pub mod rules;
pub mod run;
pub mod scheduler;
pub mod sources;
pub mod store;
