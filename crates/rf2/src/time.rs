// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! The RF2 `effectiveTime`: a calendar date written `YYYYMMDD`.

use std::fmt;
use std::str::FromStr;

use jiff::civil::Date;

/// A malformed effective time.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{text:?} is not an effectiveTime (YYYYMMDD)")]
pub struct EffectiveTimeError {
    /// The offending text.
    pub text: String,
}

/// An RF2 effective time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectiveTime(Date);

impl EffectiveTime {
    /// Parses `YYYYMMDD`.
    ///
    /// # Errors
    ///
    /// Returns [`EffectiveTimeError`] for any other text or an impossible date.
    pub fn parse(text: &str) -> Result<Self, EffectiveTimeError> {
        let error = || EffectiveTimeError {
            text: text.to_owned(),
        };
        if text.len() != 8 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error());
        }
        let group = |range: std::ops::Range<usize>| -> Result<i16, EffectiveTimeError> {
            text.get(range)
                .and_then(|g| g.parse::<i16>().ok())
                .ok_or_else(error)
        };
        let year = group(0..4)?;
        // The error names the text; the range and calendar errors add nothing to it.
        let Ok(month) = i8::try_from(group(4..6)?) else {
            return Err(error());
        };
        let Ok(day) = i8::try_from(group(6..8)?) else {
            return Err(error());
        };
        let Ok(date) = Date::new(year, month, day) else {
            return Err(error());
        };
        Ok(Self(date))
    }

    /// The calendar date.
    #[must_use]
    pub const fn date(self) -> Date {
        self.0
    }

    /// The `YYYYMMDD` form.
    #[must_use]
    pub fn compact(self) -> String {
        format!(
            "{:04}{:02}{:02}",
            self.0.year(),
            self.0.month(),
            self.0.day()
        )
    }
}

impl fmt::Display for EffectiveTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.compact())
    }
}

impl FromStr for EffectiveTime {
    type Err = EffectiveTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::EffectiveTime;

    #[test]
    fn dates_parse_and_print() {
        let time = EffectiveTime::parse("20260630").expect("valid");
        assert_eq!(time.compact(), "20260630");
        assert_eq!(time.date().year(), 2026);
        assert!(EffectiveTime::parse("20260631").is_err());
        assert!(EffectiveTime::parse("2026-06-30").is_err());
        assert!(EffectiveTime::parse("").is_err());
        assert!(
            EffectiveTime::parse("20020131").expect("valid")
                < EffectiveTime::parse("20260630").expect("valid")
        );
    }
}
