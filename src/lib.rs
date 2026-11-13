//! hackathon-radar: a self-hosted tracker that finds Hackathons that Fit a Profile.
//!
//! A [`run`] collects Candidates from each [`sources::Source`], merges
//! Duplicate listings ([`matching`]), applies the Profile [`rules`] checkable
//! from data alone, has the person's [`agent`] judge the rest ([`judging`]),
//! works out how to get to each Hackathon ([`travel`]) and what flights cost
//! ([`fares`]), keeps deadlines and status current ([`deadlines`]), keeps the
//! result in the local [`store`] and composes the [`alert`] for the person.

pub mod agent;
pub mod alert;
pub mod candidate;
pub mod clock;
pub mod countries;
pub mod deadlines;
pub mod doctor;
pub mod export;
pub mod fares;
pub mod http;
pub mod ics;
pub mod init;
pub mod judging;
pub mod matching;
pub mod money;
pub mod paths;
pub mod process;
pub mod profile;
pub mod report;
pub mod rules;
pub mod run;
pub mod scheduler;
pub mod secrets;
pub mod sheet_import;
pub mod sheet_layout;
pub mod sources;
pub mod store;
pub mod travel;
pub mod wiring;
