// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! SNOMED CT identifiers.
//!
//! An SCTID is an integer of 6 to 18 digits whose last digit is a Verhoeff
//! check digit and whose two digits before it name the partition (concept,
//! description, relationship, each in the International or an extension
//! namespace). The component kinds are distinct newtypes so a swapped
//! argument fails to compile. Reference set members are identified by UUIDs.

use std::fmt;
use std::str::FromStr;

/// The kind of component an SCTID identifies, from its partition identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Partition {
    /// A concept (`00` or `10`).
    Concept,
    /// A description (`01` or `11`).
    Description,
    /// A relationship (`02` or `12`).
    Relationship,
    /// A postcoordinated expression (`16`, long format only).
    Expression,
}

/// A malformed identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// Not 6 to 18 ASCII digits without a leading zero.
    #[error("{text:?} is not an SCTID: 6 to 18 digits, no leading zero")]
    Shape {
        /// The offending text.
        text: String,
    },
    /// The Verhoeff check digit does not match.
    #[error("{value} fails its Verhoeff check digit")]
    CheckDigit {
        /// The parsed value.
        value: u64,
    },
    /// The partition identifier is not one the release file specification defines
    /// (00, 01, 02, 10, 11, 12, 16; <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/snomed-ct-identifiers/6.5-partition-identifier>).
    #[error(
        "{value} has partition identifier {partition:02}, which the release file specification does not define"
    )]
    Partition {
        /// The parsed value.
        value: u64,
        /// The two partition digits.
        partition: u8,
    },
    /// The identifier names a different component kind than expected.
    #[error("{value} is a {actual:?} identifier where a {expected:?} was expected")]
    WrongPartition {
        /// The parsed value.
        value: u64,
        /// The kind the caller expected.
        expected: Partition,
        /// The kind the partition digits name.
        actual: Partition,
    },
    /// Not a UUID in the canonical 8-4-4-4-12 hexadecimal form.
    #[error("{text:?} is not a UUID")]
    Uuid {
        /// The offending text.
        text: String,
    },
}

/// An SCTID of any partition, validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sctid(u64);

impl Sctid {
    /// Parses and validates `text`: shape, check digit, and partition.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] for a malformed identifier.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let digits = text.len();
        if !(6..=18).contains(&digits)
            || !text.bytes().all(|b| b.is_ascii_digit())
            || text.starts_with('0')
        {
            return Err(IdError::Shape {
                text: text.to_owned(),
            });
        }
        // The shape error names the text; a `ParseIntError` adds nothing to it.
        let Ok(value) = text.parse::<u64>() else {
            return Err(IdError::Shape {
                text: text.to_owned(),
            });
        };
        if !verhoeff_valid(text) {
            return Err(IdError::CheckDigit { value });
        }
        let id = Self(value);
        id.partition()?;
        Ok(id)
    }

    /// The numeric value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// The two partition digits: the two before the check digit.
    #[must_use]
    pub fn partition_digits(self) -> u8 {
        let text = self.0.to_string();
        let end = text.len().saturating_sub(1);
        text.get(end.saturating_sub(2)..end)
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(u8::MAX)
    }

    /// The component kind the partition names.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Partition`] for an undefined partition.
    pub fn partition(self) -> Result<Partition, IdError> {
        match self.partition_digits() {
            0 | 10 => Ok(Partition::Concept),
            1 | 11 => Ok(Partition::Description),
            2 | 12 => Ok(Partition::Relationship),
            16 => Ok(Partition::Expression),
            partition => Err(IdError::Partition {
                value: self.0,
                partition,
            }),
        }
    }

    /// The seven-digit namespace identifier for an extension SCTID, `None`
    /// for an International Release identifier (partition `0x`).
    #[must_use]
    pub fn namespace(self) -> Option<u32> {
        if self.partition_digits() < 10 {
            return None;
        }
        // The namespace is the seven digits before the partition and check digits.
        let text = self.0.to_string();
        let end = text.len().saturating_sub(3);
        text.get(end.saturating_sub(7)..end)
            .and_then(|digits| digits.parse().ok())
    }
}

impl fmt::Display for Sctid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Sctid {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Verhoeff check over the whole digit string, the check digit included.
fn verhoeff_valid(digits: &str) -> bool {
    const D: [[u8; 10]; 10] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [1, 2, 3, 4, 0, 6, 7, 8, 9, 5],
        [2, 3, 4, 0, 1, 7, 8, 9, 5, 6],
        [3, 4, 0, 1, 2, 8, 9, 5, 6, 7],
        [4, 0, 1, 2, 3, 9, 5, 6, 7, 8],
        [5, 9, 8, 7, 6, 0, 4, 3, 2, 1],
        [6, 5, 9, 8, 7, 1, 0, 4, 3, 2],
        [7, 6, 5, 9, 8, 2, 1, 0, 4, 3],
        [8, 7, 6, 5, 9, 3, 2, 1, 0, 4],
        [9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
    ];
    const P: [[u8; 10]; 8] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [1, 5, 7, 6, 2, 8, 3, 0, 9, 4],
        [5, 8, 0, 3, 7, 9, 6, 1, 4, 2],
        [8, 9, 1, 6, 0, 4, 3, 5, 2, 7],
        [9, 4, 5, 3, 1, 2, 6, 8, 7, 0],
        [4, 2, 8, 6, 5, 7, 3, 9, 0, 1],
        [2, 7, 9, 3, 8, 0, 6, 4, 1, 5],
        [7, 0, 4, 6, 9, 1, 3, 2, 5, 8],
    ];
    let mut check: u8 = 0;
    for (index, byte) in digits.bytes().rev().enumerate() {
        let digit = usize::from(byte - b'0');
        let permuted = P.get(index % 8).and_then(|row| row.get(digit)).copied();
        let Some(permuted) = permuted else {
            return false;
        };
        check = D
            .get(usize::from(check))
            .and_then(|row| row.get(usize::from(permuted)))
            .copied()
            .unwrap_or(u8::MAX);
    }
    check == 0
}

/// Appends the Verhoeff check digit to `digits`, for building synthetic identifiers.
#[must_use]
pub fn with_check_digit(digits: &str) -> String {
    for candidate in b'0'..=b'9' {
        let mut text = digits.to_owned();
        text.push(char::from(candidate));
        if verhoeff_valid(&text) {
            return text;
        }
    }
    // Every digit string has exactly one valid check digit; the loop above finds it.
    digits.to_owned()
}

macro_rules! component_id {
    ($name:ident, $partition:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Sctid);

        impl $name {
            /// A published identifier, spelled as a constant.
            ///
            /// Only for SCTIDs the specifications publish; `constants` tests every one.
            #[must_use]
            pub const fn published(value: u64) -> Self {
                Self(Sctid(value))
            }

            /// Parses `text` and checks that its partition names this kind.
            ///
            /// # Errors
            ///
            /// Returns [`IdError`] for a malformed identifier or another partition.
            pub fn parse(text: &str) -> Result<Self, IdError> {
                Self::try_from(Sctid::parse(text)?)
            }

            /// The underlying SCTID.
            #[must_use]
            pub const fn sctid(self) -> Sctid {
                self.0
            }

            /// The numeric value.
            #[must_use]
            pub const fn value(self) -> u64 {
                self.0.value()
            }
        }

        impl TryFrom<Sctid> for $name {
            type Error = IdError;

            fn try_from(id: Sctid) -> Result<Self, IdError> {
                match id.partition()? {
                    actual if actual == $partition => Ok(Self(id)),
                    actual => Err(IdError::WrongPartition {
                        value: id.value(),
                        expected: $partition,
                        actual,
                    }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }
    };
}

component_id!(ConceptId, Partition::Concept, "A concept identifier.");
component_id!(
    DescriptionId,
    Partition::Description,
    "A description identifier."
);
component_id!(
    RelationshipId,
    Partition::Relationship,
    "A relationship identifier."
);

/// A reference set identifier: the concept that is the reference set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RefsetId(ConceptId);

impl RefsetId {
    /// A published reference set identifier, spelled as a constant.
    #[must_use]
    pub const fn published(value: u64) -> Self {
        Self(ConceptId::published(value))
    }

    /// Parses a concept identifier naming a reference set.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] for a malformed or non-concept identifier.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        ConceptId::parse(text).map(Self)
    }

    /// The reference set as a concept.
    #[must_use]
    pub const fn concept(self) -> ConceptId {
        self.0
    }
}

impl fmt::Display for RefsetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A module identifier: the concept that is the module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(ConceptId);

impl From<ConceptId> for ModuleId {
    fn from(concept: ConceptId) -> Self {
        Self(concept)
    }
}

impl ModuleId {
    /// A published module identifier, spelled as a constant.
    #[must_use]
    pub const fn published(value: u64) -> Self {
        Self(ConceptId::published(value))
    }

    /// Parses a concept identifier naming a module.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] for a malformed or non-concept identifier.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        ConceptId::parse(text).map(Self)
    }

    /// The module as a concept.
    #[must_use]
    pub const fn concept(self) -> ConceptId {
        self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A reference set member identifier: a UUID in canonical text form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemberId([u8; 16]);

impl MemberId {
    /// Parses the canonical `8-4-4-4-12` hexadecimal form.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Uuid`] for any other text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let error = || IdError::Uuid {
            text: text.to_owned(),
        };
        let groups: Vec<&str> = text.split('-').collect();
        let lengths = [8, 4, 4, 4, 12];
        if groups.len() != lengths.len()
            || groups.iter().zip(lengths).any(|(group, len)| {
                group.len() != len || !group.bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err(error());
        }
        let hex: String = groups.concat();
        let mut bytes = [0_u8; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let pair = hex.get(index * 2..index * 2 + 2).ok_or_else(error)?;
            let Ok(value) = u8::from_str_radix(pair, 16) else {
                return Err(error());
            };
            *byte = value;
        }
        Ok(Self(bytes))
    }

    /// The sixteen bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Display for MemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for MemberId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::{ConceptId, DescriptionId, IdError, MemberId, Partition, Sctid, with_check_digit};

    #[test]
    fn published_identifiers_validate() {
        // The is-a attribute and the SNOMED CT core module, from the specification.
        let is_a = Sctid::parse("116680003").expect("valid");
        assert_eq!(is_a.partition(), Ok(Partition::Concept));
        assert_eq!(is_a.namespace(), None);
        let core = ConceptId::parse("900000000000207008").expect("valid");
        assert_eq!(core.value(), 900_000_000_000_207_008);
        let extension = Sctid::parse("11000146104").expect("valid");
        assert_eq!(extension.namespace(), Some(1_000_146));
        assert_eq!(extension.partition(), Ok(Partition::Concept));
        // A long-format postcoordinated expression identifier (partition 16) in the
        // synthetic namespace 1234567.
        let expression = Sctid::parse(&with_check_digit("42123456716")).expect("valid");
        assert_eq!(expression.partition(), Ok(Partition::Expression));
        assert!(matches!(
            Sctid::parse(&with_check_digit("42123456713")),
            Err(IdError::Partition { partition: 13, .. })
        ));
    }

    #[test]
    fn check_digits_and_shapes_are_enforced() {
        assert_eq!(
            Sctid::parse("116680004"),
            Err(IdError::CheckDigit { value: 116_680_004 })
        );
        assert!(matches!(Sctid::parse("12345"), Err(IdError::Shape { .. })));
        assert!(matches!(
            Sctid::parse("0116680003"),
            Err(IdError::Shape { .. })
        ));
        assert!(matches!(
            Sctid::parse("11668a003"),
            Err(IdError::Shape { .. })
        ));
        assert!(matches!(
            DescriptionId::parse("116680003"),
            Err(IdError::WrongPartition {
                expected: Partition::Description,
                actual: Partition::Concept,
                ..
            })
        ));
    }

    #[test]
    fn synthetic_identifiers_get_a_valid_check_digit() {
        let id = with_check_digit("12345600");
        assert_eq!(id.len(), 9);
        assert_eq!(
            Sctid::parse(&id).map(Sctid::partition),
            Ok(Ok(Partition::Concept))
        );
        let description = with_check_digit("12345601");
        assert!(DescriptionId::parse(&description).is_ok());
    }

    #[test]
    fn member_ids_round_trip() {
        let text = "dcc1637b-9178-424b-9237-bc602a1a6fbf";
        let id = MemberId::parse(text).expect("valid uuid");
        assert_eq!(id.to_string(), text);
        assert!(matches!(
            MemberId::parse("not-a-uuid"),
            Err(IdError::Uuid { .. })
        ));
        assert!(matches!(
            MemberId::parse("dcc1637b9178424b9237bc602a1a6fbf"),
            Err(IdError::Uuid { .. })
        ));
    }
}
