use chrono::{DateTime, NaiveDate, Utc};

/// Tells a Run what day it is, so tests can pin the date.
pub trait Clock {
    fn today(&self) -> NaiveDate;

    /// The current instant, for the Run history's start and end times.
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// The machine's clock, in its local time zone.
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        chrono::Local::now().date_naive()
    }
}
