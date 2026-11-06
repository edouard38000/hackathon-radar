//! Exports (ticket 15): a deadline calendar ([`ics`]) and a CSV laid out like
//! the personal tracker's Sheet ([`sheet`]). Both are generated from the store
//! through [`crate::report::collect`] and never read back (ADR 0003).

pub mod ics;
pub mod sheet;
