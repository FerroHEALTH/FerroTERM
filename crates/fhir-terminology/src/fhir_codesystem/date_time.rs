// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `dateTime` reading behind the retirement and deprecation dates of
//! a concept (<https://hl7.org/fhir/R5/datatypes.html#dateTime>).

/// Whether the `retirementDate` in `text` retires the concept as at `now`.
///
/// Only a date still to come leaves the concept active; text that is no
/// `dateTime` states no date to wait for, so the retirement is in force.
pub(super) fn retires_at(text: &str, now: jiff::Timestamp) -> bool {
    earliest_instant(text).is_none_or(|instant| instant <= now)
}

/// The first instant the FHIR `dateTime` in `text` covers, or `None` when the
/// text is not one.
///
/// `dateTime` is a year, a year and month, a date, or a date and a whole time
/// with a timezone, and nothing else
/// (<https://hl7.org/fhir/R5/datatypes.html#dateTime>).
fn earliest_instant(text: &str) -> Option<jiff::Timestamp> {
    let (date_text, time_text) = match text.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let date = period_start(date_text)?;
    let Some(time_text) = time_text else {
        // NOTE: no FHIR/SNOMED spec governs this: our own design compares a
        // partial `dateTime` at the first instant of the period it names.
        return date
            .at(0, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .ok()
            .map(|zoned| zoned.timestamp());
    };
    if date_text.split('-').count() != 3 || !is_time_with_zone(time_text) {
        return None;
    }
    text.parse::<jiff::Timestamp>().ok()
}

/// The first day of the period a `YYYY`, `YYYY-MM`, or `YYYY-MM-DD` names.
fn period_start(text: &str) -> Option<jiff::civil::Date> {
    let mut parts = text.split('-');
    let year = i16::try_from(digits(parts.next()?, 4)?).ok()?;
    if year == 0 {
        return None;
    }
    let month = match parts.next() {
        Some(month) => i8::try_from(digits(month, 2)?).ok()?,
        None => 1,
    };
    let day = match parts.next() {
        Some(day) => i8::try_from(digits(day, 2)?).ok()?,
        None => 1,
    };
    if parts.next().is_some() {
        return None;
    }
    jiff::civil::Date::new(year, month, day).ok()
}

/// Whether `text` is `hh:mm:ss` with an optional fraction and a timezone.
fn is_time_with_zone(text: &str) -> bool {
    let Some(start) = text.rfind(['Z', '+', '-']) else {
        return false;
    };
    let Some((time, zone)) = text.split_at_checked(start) else {
        return false;
    };
    is_zone(zone) && is_time(time)
}

/// Whether `text` is `Z` or an offset of `+hh:mm` / `-hh:mm` up to 14:00.
fn is_zone(text: &str) -> bool {
    if text == "Z" {
        return true;
    }
    let mut characters = text.chars();
    if !matches!(characters.next(), Some('+' | '-')) {
        return false;
    }
    let Some((hours, minutes)) = characters.as_str().split_once(':') else {
        return false;
    };
    match (digits(hours, 2), digits(minutes, 2)) {
        (Some(hours), Some(minutes)) => {
            (hours < 14 && minutes < 60) || (hours == 14 && minutes == 0)
        }
        _ => false,
    }
}

/// Whether `text` is `hh:mm:ss` with an optional decimal fraction of a second.
fn is_time(text: &str) -> bool {
    let mut parts = text.split(':');
    let (Some(hours), Some(minutes), Some(rest), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let (seconds, fraction) = match rest.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (rest, None),
    };
    if fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    match (digits(hours, 2), digits(minutes, 2), digits(seconds, 2)) {
        // A leap second is `60` in the lexical form the specification gives.
        (Some(hours), Some(minutes), Some(seconds)) => hours < 24 && minutes < 60 && seconds <= 60,
        _ => false,
    }
}

/// The value of `text` when it is exactly `count` ASCII digits.
fn digits(text: &str, count: usize) -> Option<u32> {
    if text.len() != count || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{earliest_instant, retires_at};

    /// The instant every case is judged against.
    fn now() -> jiff::Timestamp {
        "2026-01-01T00:00:00Z".parse().expect("a fixed timestamp")
    }

    #[test]
    fn every_date_time_form_names_the_first_instant_it_covers() {
        let instant = |text: &str| earliest_instant(text).expect("a dateTime").to_string();
        assert_eq!(instant("2001"), "2001-01-01T00:00:00Z");
        assert_eq!(instant("2001-06"), "2001-06-01T00:00:00Z");
        assert_eq!(instant("2001-06-15"), "2001-06-15T00:00:00Z");
        assert_eq!(instant("2001-06-15T12:30:00Z"), "2001-06-15T12:30:00Z");
        assert_eq!(instant("2001-06-15T12:30:00.5Z"), "2001-06-15T12:30:00.5Z");
        assert_eq!(instant("2001-06-15T12:30:00+02:00"), "2001-06-15T10:30:00Z");
    }

    // The `dateTime` regular expression admits a four-digit non-zero year, a
    // two-digit month and day, and a whole time with a timezone, nothing else
    // (<https://hl7.org/fhir/R5/datatypes.html#dateTime>).
    #[test]
    fn text_the_date_time_form_refuses_names_no_instant() {
        for text in [
            "",
            "retired",
            "0000",
            "1",
            "99",
            "20010615",
            "2001-6",
            "2001-13",
            "2001-02-30",
            "2001-06-15-01",
            "2001-06-15T12",
            "2001-06-15T12:30",
            "2001-06-15T12:30:00",
            "2001-06-15T12:30:00+15:00",
            "2001-06-15T12:30:00+02:00[Europe/Paris]",
            "2001-06T12:30:00Z",
        ] {
            assert_eq!(earliest_instant(text), None, "{text:?}");
        }
    }

    #[test]
    fn only_a_date_still_to_come_leaves_the_concept_active() {
        assert!(retires_at("2001-06-15", now()));
        assert!(!retires_at("2999-01-01", now()));
        assert!(
            retires_at("yesterday", now()),
            "text that states no date to wait for retires the concept"
        );
    }
}
