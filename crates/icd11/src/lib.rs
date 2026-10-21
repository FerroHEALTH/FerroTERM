// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ICD-11 from the WHO ICD-API.
//!
//! `api`, behind the `api` feature, walks a local deployment of the ICD-API
//! and caches every entity of a linearization (or the Foundation) as the JSON
//! the API serves, per language; [`cache`] reads such a cache back into
//! [`entity::Entity`] records; [`expression`] parses postcoordination
//! expressions. The system URIs are THO's
//! (<https://terminology.hl7.org/CodeSystem-ICD11MMS.html>) and the test cases
//! of the HL7 terminology ecosystem IG name the other two.
//!
//! The walker is the only part that speaks HTTP, and only the offline build
//! reaches it, so the feature is off by default.
#![doc(test(attr(deny(warnings))))]

#[cfg(feature = "api")]
pub mod api;
pub mod cache;
pub mod entity;
pub mod expression;

/// The MMS linearization's system URI.
pub const MMS: &str = "http://id.who.int/icd/release/11/mms";
/// The ICF linearization's system URI.
pub const ICF: &str = "http://id.who.int/icd/release/11/icf";
/// The Foundation's system URI.
pub const FOUNDATION: &str = "http://id.who.int/icd/entity";
/// The prefix of the postcoordination axis names.
pub const SCHEMA: &str = "http://id.who.int/icd/schema/";
/// The canonical host every URI the API returns starts with.
pub const CANONICAL: &str = "http://id.who.int";

/// One of the three code systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Linearization {
    /// ICD-11 for Mortality and Morbidity Statistics.
    Mms,
    /// The International Classification of Functioning, Disability and Health.
    Icf,
    /// The WHO Family of International Classifications Foundation.
    Foundation,
}

impl Linearization {
    /// Every code system, in the order they are built.
    pub const ALL: [Self; 3] = [Self::Mms, Self::Icf, Self::Foundation];

    /// The system URI.
    #[must_use]
    pub const fn system(self) -> &'static str {
        match self {
            Self::Mms => MMS,
            Self::Icf => ICF,
            Self::Foundation => FOUNDATION,
        }
    }

    /// The short name, also the cache directory name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mms => "mms",
            Self::Icf => "icf",
            Self::Foundation => "entity",
        }
    }

    /// The linearization for a short name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.name() == name)
    }

    /// The API path of the root, for `release` (`2026-01`).
    #[must_use]
    pub fn root_path(self, release: &str) -> String {
        match self {
            Self::Foundation => String::from("/icd/entity"),
            other => format!("/icd/release/11/{release}/{}", other.name()),
        }
    }

    /// The unversioned URI of the entity `id` (`257068234`, `1363559646/other`).
    #[must_use]
    pub fn uri(self, id: &str) -> String {
        format!("{}/{id}", self.system())
    }

    /// The versioned URI of the entity `id` in `release`.
    #[must_use]
    pub fn versioned_uri(self, release: &str, id: &str) -> String {
        match self {
            Self::Foundation => self.uri(id),
            other => format!(
                "http://id.who.int/icd/release/11/{release}/{}/{id}",
                other.name()
            ),
        }
    }

    /// The entity id a URI of this code system names, when it is one:
    /// `http://id.who.int/icd/release/11/mms/257068234`,
    /// `.../release/11/2026-01/mms/1363559646/other`, `http://id.who.int/icd/entity/257068234`.
    #[must_use]
    pub fn id_of(self, uri: &str) -> Option<String> {
        let rest = uri.strip_prefix(CANONICAL)?;
        let tail = match self {
            Self::Foundation => rest.strip_prefix("/icd/entity/")?,
            other => {
                let after = rest.strip_prefix("/icd/release/11/")?;
                let (head, remainder) = after.split_once('/')?;
                if head == other.name() {
                    remainder
                } else if is_release(head) {
                    let (name, tail) = remainder.split_once('/')?;
                    if name != other.name() {
                        return None;
                    }
                    tail
                } else {
                    return None;
                }
            }
        };
        let tail = tail.trim_end_matches('/');
        let mut parts = tail.split('/');
        let id = parts.next()?;
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        match parts.next() {
            None => Some(id.to_owned()),
            Some(residual @ ("other" | "unspecified")) if parts.next().is_none() => {
                Some(format!("{id}/{residual}"))
            }
            Some(_) => None,
        }
    }

    /// The key table entry of the entity `id`: the number times four plus
    /// `1` for `/other` and `2` for `/unspecified`.
    #[must_use]
    pub fn key_of(id: &str) -> Option<u64> {
        let (number, residual) = match id.split_once('/') {
            None => (id, 0),
            Some((n, "other")) => (n, 1),
            Some((n, "unspecified")) => (n, 2),
            Some(_) => return None,
        };
        let number: u64 = number.parse().ok()?;
        number.checked_mul(4)?.checked_add(residual)
    }
}

/// Whether `text` is a release id (`2026-01`).
#[must_use]
pub fn is_release(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 7
        && bytes
            .get(..4)
            .is_some_and(|y| y.iter().all(u8::is_ascii_digit))
        && bytes.get(4) == Some(&b'-')
        && bytes
            .get(5..)
            .is_some_and(|m| m.iter().all(u8::is_ascii_digit))
}

#[cfg(test)]
mod tests {
    use super::{Linearization, is_release};

    #[test]
    fn uris_name_entities_in_both_forms() {
        let mms = Linearization::Mms;
        assert_eq!(
            mms.id_of("http://id.who.int/icd/release/11/mms/257068234")
                .as_deref(),
            Some("257068234")
        );
        assert_eq!(
            mms.id_of("http://id.who.int/icd/release/11/2026-01/mms/1363559646/other")
                .as_deref(),
            Some("1363559646/other")
        );
        assert_eq!(mms.id_of("http://id.who.int/icd/release/11/icf/12"), None);
        assert_eq!(mms.id_of("http://id.who.int/icd/release/11/mms/abc"), None);
        assert_eq!(
            mms.id_of("http://id.who.int/icd/release/11/mms/1/2/3"),
            None
        );
        assert_eq!(
            Linearization::Foundation
                .id_of("http://id.who.int/icd/entity/257068234")
                .as_deref(),
            Some("257068234")
        );
        assert_eq!(
            Linearization::Foundation.id_of("http://id.who.int/icd/release/11/mms/257068234"),
            None
        );
        assert_eq!(mms.uri("1A"), "http://id.who.int/icd/release/11/mms/1A");
        assert_eq!(
            mms.versioned_uri("2026-01", "5"),
            "http://id.who.int/icd/release/11/2026-01/mms/5"
        );
        assert_eq!(
            Linearization::key_of("1363559646/other"),
            Some(1_363_559_646 * 4 + 1)
        );
        assert_eq!(Linearization::key_of("12/x"), None);
        assert!(is_release("2026-01"));
        assert!(!is_release("mms"));
    }
}
