// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! LOINC codes and their check digit.
//!
//! A LOINC code is digits, a hyphen, and one check digit computed with the
//! Mod 10 algorithm of the LOINC Users' Guide: from the right, the digits in
//! odd positions are joined and doubled, the digits in even positions are
//! appended, the digits of that number are summed, and the check digit is the
//! distance to the next multiple of ten.

/// The check digit of `body`, the digits before the hyphen.
///
/// Returns `None` when `body` is empty or not all digits.
#[must_use]
pub fn check_digit(body: &str) -> Option<char> {
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits: Vec<u32> = body.bytes().rev().map(|b| u32::from(b - b'0')).collect();
    let odd: String = digits
        .iter()
        .step_by(2)
        .map(|d| char::from(b'0' + u8::try_from(*d).unwrap_or(0)))
        .collect();
    let even: String = digits
        .iter()
        .skip(1)
        .step_by(2)
        .map(|d| char::from(b'0' + u8::try_from(*d).unwrap_or(0)))
        .collect();
    let doubled = odd.parse::<u64>().ok()?.checked_mul(2)?;
    let joined = format!("{doubled}{even}");
    let sum: u32 = joined.bytes().map(|b| u32::from(b - b'0')).sum();
    let check = (10 - sum % 10) % 10;
    Some(char::from(b'0' + u8::try_from(check).unwrap_or(0)))
}

/// Whether `code` is a well-formed LOINC code.
///
/// A term code is digits, a hyphen, and one check digit the Mod 10 algorithm
/// confirms. Part (`LP…`), answer list (`LL…`), and answer (`LA…`) codes are
/// checked for shape only: their published check digits do not follow the
/// term algorithm over the digits.
#[must_use]
pub fn is_valid(code: &str) -> bool {
    let Some((head, check)) = code.rsplit_once('-') else {
        return false;
    };
    let body = head.trim_start_matches(|c: char| c.is_ascii_uppercase());
    let prefix = head.strip_suffix(body).unwrap_or_default();
    let shape = !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_digit())
        && check.len() == 1
        && check.bytes().all(|b| b.is_ascii_digit());
    match prefix {
        "" => shape && check_digit(body).is_some_and(|expected| check.starts_with(expected)),
        "LP" | "LL" | "LA" => shape,
        _ => false,
    }
}

/// Whether `code` has the shape of a LOINC code: an optional `LP`, `LL`, or
/// `LA` prefix, digits, a hyphen, and one digit, whatever that digit is.
///
/// The published term table is the authority on which codes exist, and it
/// carries deprecated codes whose check digit does not follow the algorithm
/// (`11491-6` in LOINC 2.83), so a reader of the table checks shape only and
/// leaves the check digit to [`is_valid`] on codes a client submits.
#[must_use]
pub fn is_well_formed(code: &str) -> bool {
    let Some((head, check)) = code.rsplit_once('-') else {
        return false;
    };
    let body = head.trim_start_matches(|c: char| c.is_ascii_uppercase());
    let prefix = head.strip_suffix(body).unwrap_or_default();
    matches!(prefix, "" | "LP" | "LL" | "LA")
        && !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_digit())
        && check.len() == 1
        && check.bytes().all(|b| b.is_ascii_digit())
}

/// `body` with its check digit appended.
#[must_use]
pub fn with_check_digit(body: &str) -> Option<String> {
    check_digit(body).map(|c| format!("{body}-{c}"))
}

#[cfg(test)]
mod tests {
    use super::{check_digit, is_valid, is_well_formed, with_check_digit};

    #[test]
    fn the_users_guide_example_and_published_codes_check() {
        assert_eq!(check_digit("12345"), Some('5'));
        assert!(is_valid("2345-7"), "glucose in blood");
        assert!(is_valid("LP31755-9"), "a part code is checked for shape");
        assert!(is_valid("LL715-4"));
        assert!(is_valid("8480-6"), "systolic blood pressure");
        assert!(!is_valid("LP31755-99"));
        assert!(!is_valid("2345-8"));
        assert!(!is_valid("2345"));
        assert!(!is_valid("XX2345-7"));
        assert!(
            is_well_formed("11491-6"),
            "a published code with a failing check digit"
        );
        assert!(!is_well_formed("11491-66"));
        assert!(!is_well_formed("XX2345-7"));
        assert_eq!(with_check_digit("2345").as_deref(), Some("2345-7"));
    }
}
