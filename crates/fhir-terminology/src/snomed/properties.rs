// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the `$lookup` properties of a concept say beside their values.

use concept_graph::ordinal::Ordinal;

use super::SnomedProvider;
use crate::provider::{Property, PropertyValue, error::ProviderError};
use crate::stored::storage;

/// `30051010000102 |LOINC code identifier|`, the identifier scheme of the
/// LOINC codes the LOINC Extension states in the RF2 identifier file.
pub(super) const LOINC_SCHEME: u64 = 30_051_010_000_102;

/// The property that carries a concept's LOINC code.
pub(super) const LOINC_PROPERTY: &str = "loinc";

/// The code system of LOINC (<https://terminology.hl7.org/LOINC.html>).
pub(super) const LOINC_SYSTEM: &str = "http://loinc.org";

/// The LOINC codes the RF2 identifier file gives `concept`, as `loinc`
/// properties.
///
/// The tx-ecosystem IG's `tx-source` readme (LOINC Extension content) is the only
/// source that names the property and its Coding, pending SNOMED review: our
/// own design otherwise.
pub(super) fn push_loinc(provider: &SnomedProvider, concept: Ordinal, out: &mut Vec<Property>) {
    for (scheme, code) in provider.system().identifiers.of_concept(concept) {
        if scheme == LOINC_SCHEME {
            out.push(Property {
                code: LOINC_PROPERTY.to_owned(),
                value: Some(PropertyValue::Coding {
                    system: LOINC_SYSTEM.to_owned(),
                    code: code.to_owned(),
                    display: None,
                }),
                ..Property::default()
            });
        }
    }
}

/// The FHIR-defined properties of "the given code or expression" that an
/// expression itself states (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED
/// CT Properties"): its status and its own refinement.
pub(super) fn of_expression(
    provider: &SnomedProvider,
    record: &super::expression::Registered,
) -> Result<Vec<Property>, ProviderError> {
    let mut out = vec![
        Property {
            code: String::from("inactive"),
            value: Some(PropertyValue::Boolean(
                !provider.expression_is_active(record)?,
            )),
            ..Property::default()
        },
        Property {
            code: String::from("sufficientlyDefined"),
            value: Some(PropertyValue::Boolean(record.sufficiently_defined())),
            ..Property::default()
        },
    ];
    out.extend(SnomedProvider::expression_attributes(record));
    Ok(out)
}

/// Adds `property` unless `out` holds the same code and value.
///
/// A value a concept carries in more than one role group is one value of its
/// property, so it is listed once; no specification governs this: our own
/// design.
pub(super) fn push_once(out: &mut Vec<Property>, property: Property) {
    if !out
        .iter()
        .any(|held| held.code == property.code && held.value == property.value)
    {
        out.push(property);
    }
}

/// Each property whose value is a concept of the edition gets that concept's
/// display as its `description`, "Human Readable representation of the
/// property value (e.g. display for a code)"
/// (`OperationDefinition/CodeSystem-lookup`, `property.description`).
pub(super) fn describe(
    provider: &SnomedProvider,
    out: &mut [Property],
) -> Result<(), ProviderError> {
    for property in out.iter_mut() {
        let Some(PropertyValue::Code(code)) = &property.value else {
            continue;
        };
        let Some(ordinal) = provider.stored().store.ordinal(code).map_err(storage)? else {
            continue;
        };
        property.description = provider.choose_display(ordinal, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::push_once;
    use crate::provider::{Property, PropertyValue};

    #[test]
    fn a_value_repeated_in_another_role_group_is_listed_once() {
        let site = |code: &str| Property {
            code: String::from("363698007"),
            value: Some(PropertyValue::Code(code.to_owned())),
            ..Property::default()
        };
        let mut out = Vec::new();
        push_once(&mut out, site("10200004"));
        push_once(&mut out, site("10200004"));
        push_once(&mut out, site("64033007"));
        assert_eq!(out, [site("10200004"), site("64033007")]);
    }
}
