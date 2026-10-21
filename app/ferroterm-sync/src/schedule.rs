// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! When the next run is due.
//!
//! Two shapes are configured, an interval and a time of day, and both are read
//! by `jiff`: an interval is a signed duration in the ISO 8601 or the friendly
//! form (`24h`, `PT24H`), and a time of day is a civil time (`03:00`) read in
//! the configured zone
//! (<https://docs.rs/jiff/latest/jiff/struct.SignedDuration.html>). There is no
//! cron syntax.
//!
//! The due time is computed from the end of the last run, which the service
//! persists, so a restart between two runs does not replay the run that
//! already happened in this window. With no run recorded yet, an interval is
//! due at once and a time of day waits for its next occurrence.

use crate::config::ScheduleConfig;

/// A schedule that does not read.
#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    /// The interval is not a duration.
    #[error("`{value}` is not an interval such as `24h`")]
    Every {
        /// The value the file carried.
        value: String,
        /// Why it is not a duration.
        #[source]
        source: jiff::Error,
    },
    /// The time of day is not a civil time.
    #[error("`{value}` is not a time of day such as `03:00`")]
    At {
        /// The value the file carried.
        value: String,
        /// Why it is not a time.
        #[source]
        source: jiff::Error,
    },
    /// The time zone is not one the system knows.
    #[error("`{name}` is not a time zone")]
    Zone {
        /// The zone name the file carried.
        name: String,
        /// Why the zone did not resolve.
        #[source]
        source: jiff::Error,
    },
    /// The next due time falls outside the range a timestamp holds.
    #[error("the next run does not fall inside the representable range")]
    OutOfRange {
        /// Why the arithmetic did not hold.
        #[source]
        source: jiff::Error,
    },
}

/// The schedule of one running service.
#[derive(Debug, Clone)]
pub struct Plan {
    every: Option<jiff::SignedDuration>,
    at: Option<jiff::civil::Time>,
    zone: jiff::tz::TimeZone,
}

impl Plan {
    /// Reads the schedule `config` describes, in the zone `timezone` names.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::Every`] when the interval is not a duration,
    /// [`ScheduleError::At`] when the time of day is not a time, and
    /// [`ScheduleError::Zone`] when the zone is not one the system knows.
    pub fn parse(config: &ScheduleConfig, timezone: &str) -> Result<Self, ScheduleError> {
        let every = match config.every.as_deref() {
            None => None,
            Some(value) => Some(value.parse::<jiff::SignedDuration>().map_err(|source| {
                ScheduleError::Every {
                    value: value.to_owned(),
                    source,
                }
            })?),
        };
        let at =
            match config.at.as_deref() {
                None => None,
                Some(value) => Some(value.parse::<jiff::civil::Time>().map_err(|source| {
                    ScheduleError::At {
                        value: value.to_owned(),
                        source,
                    }
                })?),
            };
        let zone = jiff::tz::TimeZone::get(timezone).map_err(|source| ScheduleError::Zone {
            name: timezone.to_owned(),
            source,
        })?;
        Ok(Self { every, at, zone })
    }

    /// Whether the service starts no run by itself.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.every.is_none() && self.at.is_none()
    }

    /// When the next run is due, given the end of the last one.
    ///
    /// A due time in the past means the window was missed, and the run starts
    /// at once. With no schedule configured there is no due time at all.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::OutOfRange`] when the next due time falls
    /// outside the range a timestamp holds.
    pub fn next_after(
        &self,
        last: Option<jiff::Timestamp>,
        now: jiff::Timestamp,
    ) -> Result<Option<jiff::Timestamp>, ScheduleError> {
        let mut due: Option<jiff::Timestamp> = None;
        if let Some(every) = self.every {
            let next = match last {
                Some(last) => last
                    .checked_add(every)
                    .map_err(|source| ScheduleError::OutOfRange { source })?,
                None => now,
            };
            due = Some(next);
        }
        if let Some(at) = self.at {
            let anchor = last.unwrap_or(now);
            let next = self.next_occurrence(at, anchor)?;
            due = Some(due.map_or(next, |first| first.min(next)));
        }
        Ok(due)
    }

    /// The first occurrence of the time of day `at` strictly after `anchor`.
    fn next_occurrence(
        &self,
        at: jiff::civil::Time,
        anchor: jiff::Timestamp,
    ) -> Result<jiff::Timestamp, ScheduleError> {
        let zoned = anchor.to_zoned(self.zone.clone());
        let today = zoned
            .with()
            .time(at)
            .build()
            .map_err(|source| ScheduleError::OutOfRange { source })?;
        if today.timestamp() > anchor {
            return Ok(today.timestamp());
        }
        let tomorrow = today
            .tomorrow()
            .map_err(|source| ScheduleError::OutOfRange { source })?;
        Ok(tomorrow.timestamp())
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::Plan;
    use crate::config::ScheduleConfig;

    fn at(text: &str) -> Result<jiff::Timestamp, jiff::Error> {
        text.parse()
    }

    #[test]
    fn no_schedule_is_no_due_time() -> Result<(), Box<dyn core::error::Error>> {
        let plan = Plan::parse(&ScheduleConfig::default(), "UTC")?;
        assert!(plan.is_empty(), "nothing is configured");
        assert_eq!(
            plan.next_after(None, at("2026-09-22T10:00:00Z")?)?,
            None,
            "a service with no schedule runs only when it is asked to"
        );
        Ok(())
    }

    #[test]
    fn an_interval_runs_from_the_end_of_the_last_run() -> Result<(), Box<dyn core::error::Error>> {
        let plan = Plan::parse(
            &ScheduleConfig {
                every: Some(String::from("24h")),
                at: None,
            },
            "UTC",
        )?;
        let last = at("2026-09-22T03:00:00Z")?;
        assert_eq!(
            plan.next_after(Some(last), at("2026-09-22T04:00:00Z")?)?,
            Some(at("2026-09-23T03:00:00Z")?),
            "a restart an hour later waits for the interval, it does not replay"
        );
        Ok(())
    }

    #[test]
    fn an_interval_with_nothing_recorded_is_due_at_once() -> Result<(), Box<dyn core::error::Error>>
    {
        let plan = Plan::parse(
            &ScheduleConfig {
                every: Some(String::from("6h")),
                at: None,
            },
            "UTC",
        )?;
        let now = at("2026-09-22T10:00:00Z")?;
        assert_eq!(
            plan.next_after(None, now)?,
            Some(now),
            "the first run of a fresh deployment starts at once"
        );
        Ok(())
    }

    #[test]
    fn a_missed_window_is_due_at_once() -> Result<(), Box<dyn core::error::Error>> {
        let plan = Plan::parse(
            &ScheduleConfig {
                every: Some(String::from("24h")),
                at: None,
            },
            "UTC",
        )?;
        let last = at("2026-09-19T03:00:00Z")?;
        let due = plan.next_after(Some(last), at("2026-09-22T10:00:00Z")?)?;
        assert_eq!(
            due,
            Some(at("2026-09-20T03:00:00Z")?),
            "a service that was down comes back due, so the run starts at once"
        );
        Ok(())
    }

    #[test]
    fn a_time_of_day_is_read_in_the_configured_zone() -> Result<(), Box<dyn core::error::Error>> {
        let plan = Plan::parse(
            &ScheduleConfig {
                every: None,
                at: Some(String::from("03:00")),
            },
            "Europe/Amsterdam",
        )?;
        let now = at("2026-09-22T10:00:00Z")?;
        assert_eq!(
            plan.next_after(None, now)?,
            Some(at("2026-09-23T01:00:00Z")?),
            "03:00 in Amsterdam is 01:00Z while summer time is in force"
        );
        Ok(())
    }

    #[test]
    fn a_time_of_day_later_today_is_the_next_occurrence() -> Result<(), Box<dyn core::error::Error>>
    {
        let plan = Plan::parse(
            &ScheduleConfig {
                every: None,
                at: Some(String::from("03:00")),
            },
            "UTC",
        )?;
        assert_eq!(
            plan.next_after(None, at("2026-09-22T01:00:00Z")?)?,
            Some(at("2026-09-22T03:00:00Z")?),
            "the same day's occurrence is taken when it is still ahead"
        );
        Ok(())
    }

    #[test]
    fn both_shapes_take_the_earlier_due_time() -> Result<(), Box<dyn core::error::Error>> {
        let plan = Plan::parse(
            &ScheduleConfig {
                every: Some(String::from("24h")),
                at: Some(String::from("03:00")),
            },
            "UTC",
        )?;
        let last = at("2026-09-22T03:00:00Z")?;
        assert_eq!(
            plan.next_after(Some(last), at("2026-09-22T12:00:00Z")?)?,
            Some(at("2026-09-23T03:00:00Z")?),
            "both shapes fall on the same instant here, and neither is earlier"
        );
        let plan = Plan::parse(
            &ScheduleConfig {
                every: Some(String::from("36h")),
                at: Some(String::from("03:00")),
            },
            "UTC",
        )?;
        assert_eq!(
            plan.next_after(Some(last), at("2026-09-22T12:00:00Z")?)?,
            Some(at("2026-09-23T03:00:00Z")?),
            "the daily time comes before the longer interval"
        );
        Ok(())
    }
}
