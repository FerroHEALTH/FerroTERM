// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
// The build script includes this file (`include!`), so it carries no module
// doc of its own: an inner doc comment cannot appear where it is included.

/// The date the `## [<version>] - <date>` heading of `changelog` carries, when
/// it is a FHIR `date`.
///
/// Returns [`ReleaseDate::Unreleased`] when the version has no heading of its
/// own, and [`ReleaseDate::Malformed`] when its heading carries something that
/// is no FHIR `date`.
pub(crate) fn release_date<'a>(changelog: &'a str, version: &str) -> ReleaseDate<'a> {
    let heading = format!("## [{version}] - ");
    let Some(date) = changelog
        .lines()
        .find_map(|line| line.strip_prefix(heading.as_str()))
        .map(str::trim)
    else {
        return ReleaseDate::Unreleased;
    };
    if is_date(date) {
        ReleaseDate::Released(date)
    } else {
        ReleaseDate::Malformed(date)
    }
}

/// What the changelog says about the version being built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseDate<'a> {
    /// The heading carries this FHIR `date`.
    Released(&'a str),
    /// The version has no heading of its own.
    Unreleased,
    /// The heading carries this, which is no FHIR `date`.
    Malformed(&'a str),
}

/// Whether `text` is a FHIR `date`: `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`, each
/// part a real calendar value (<https://hl7.org/fhir/R4B/datatypes.html#date>).
pub(crate) fn is_date(text: &str) -> bool {
    let mut parts = text.split('-');
    let (Some(year), month, day, None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if day.is_some() && month.is_none() {
        return false;
    }
    let number = |part: Option<&str>, width: usize, last: u32| match part {
        None => true,
        Some(part) => {
            part.len() == width
                && part.bytes().all(|b| b.is_ascii_digit())
                && part
                    .parse::<u32>()
                    .is_ok_and(|value| (1..=last).contains(&value))
        }
    };
    number(Some(year), 4, 9999) && number(month, 2, 12) && number(day, 2, 31)
}

#[cfg(test)]
mod tests {
    use super::{ReleaseDate, is_date, release_date};

    const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- a thing\n\n## [0.0.9] - 2026-09-04\n\n- another\n\n## [0.0.8] - not a date\n";

    #[test]
    fn a_released_heading_gives_its_date_and_every_other_shape_says_why_not() {
        assert_eq!(
            release_date(CHANGELOG, "0.0.9"),
            ReleaseDate::Released("2026-09-04")
        );
        assert_eq!(
            release_date(CHANGELOG, "0.1.0"),
            ReleaseDate::Unreleased,
            "a version with no heading of its own"
        );
        assert_eq!(
            release_date(CHANGELOG, "0.0.8"),
            ReleaseDate::Malformed("not a date")
        );
    }

    #[test]
    fn a_fhir_date_is_a_year_a_month_or_a_day() {
        assert!(is_date("2026"));
        assert!(is_date("2026-09"));
        assert!(is_date("2026-09-04"));
        assert!(!is_date("Unreleased"));
        assert!(!is_date(""));
        assert!(!is_date("2026-13-01"), "month 13");
        assert!(!is_date("2026-09-32"), "day 32");
        assert!(!is_date("2026-9-4"), "the parts are padded");
        assert!(!is_date("2026-09-04T12:00:00Z"), "a date, not a timestamp");
        assert!(!is_date("2026-09-04-01"), "a fourth part");
    }
}
