// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the `$lookup` properties of a concept say beside their values.

use concept_graph::ordinal::Ordinal;
use concept_store::record;

use super::SnomedProvider;
use crate::provider::{Concept, Property, PropertyValue, error::ProviderError};
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
                value: PropertyValue::Coding {
                    system: LOINC_SYSTEM.to_owned(),
                    code: code.to_owned(),
                    display: None,
                },
                ..Property::default()
            });
        }
    }
}

/// The properties of a precoordinated concept: the FHIR-defined ones, its
/// defining relationships and its LOINC codes, each concept value described.
pub(super) fn of_concept(
    provider: &SnomedProvider,
    concept: Concept,
) -> Result<Vec<Property>, ProviderError> {
    let ordinal = SnomedProvider::ordinal(concept);
    let Some(record) = provider.stored().store.concept(ordinal).map_err(storage)? else {
        return Ok(Vec::new());
    };
    let stored = provider
        .stored()
        .store
        .properties(ordinal)
        .map_err(storage)?;
    let of = |key: u32| stored.iter().find(|(k, _)| *k == key).map(|(_, v)| v);
    let mut out = vec![Property {
        code: String::from("inactive"),
        value: PropertyValue::Boolean(!record.active),
        ..Property::default()
    }];
    let defined = of(provider.system().keys.definition_status)
        .is_some_and(|values| super::selection::states_defined(values));
    out.push(Property {
        code: String::from("sufficientlyDefined"),
        value: PropertyValue::Boolean(defined),
        ..Property::default()
    });
    if let Some(values) = of(provider.system().keys.module)
        && let Some(record::PropertyValueRef::Code(module)) = values.first()
    {
        out.push(Property {
            code: String::from("moduleId"),
            value: PropertyValue::Code((*module).to_owned()),
            ..Property::default()
        });
    }
    if let Some(time) = record.effective_time {
        out.push(Property {
            code: String::from("effectiveTime"),
            value: PropertyValue::DateTime(date_of(&time)),
            ..Property::default()
        });
    }
    if let Some(tag) = provider.semantic_tag(ordinal)? {
        out.push(Property {
            code: String::from(super::SEMANTIC_TAG),
            value: PropertyValue::Code(tag),
            ..Property::default()
        });
    }
    provider.stored().relatives(concept, &mut out)?;
    for (key, sctid) in &provider.system().keys.attributes {
        let Some(values) = of(*key) else {
            continue;
        };
        for value in values {
            let Some(value) = provider.stored().property_value(*value)? else {
                continue;
            };
            push_once(
                &mut out,
                Property {
                    code: sctid.clone(),
                    value,
                    ..Property::default()
                },
            );
        }
    }
    push_loinc(provider, ordinal, &mut out);
    describe(provider, &mut out)?;
    Ok(out)
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
            value: PropertyValue::Boolean(!provider.expression_is_active(record)?),
            ..Property::default()
        },
        Property {
            code: String::from("sufficientlyDefined"),
            value: PropertyValue::Boolean(record.sufficiently_defined()),
            ..Property::default()
        },
    ];
    out.extend(SnomedProvider::expression_attributes(record));
    Ok(out)
}

/// An RF2 `effectiveTime` (`YYYYMMDD`) as the FHIR `date` the `effectiveTime`
/// property's `dateTime` takes (<https://terminology.hl7.org/SNOMEDCT.html>);
/// a value of another shape is passed on as it is.
pub(super) fn date_of(time: &str) -> String {
    match (time.get(0..4), time.get(4..6), time.get(6..8), time.len()) {
        (Some(year), Some(month), Some(day), 8) if time.bytes().all(|b| b.is_ascii_digit()) => {
            format!("{year}-{month}-{day}")
        }
        _ => time.to_owned(),
    }
}

impl SnomedProvider {
    /// The semantic tag of a concept: the text in the last brackets of its
    /// active fully specified name in the edition's own language.
    pub(super) fn semantic_tag(&self, concept: Ordinal) -> Result<Option<String>, ProviderError> {
        let keys = &self.system().keys;
        let wanted = crate::stored::primary_subtag(&self.system().base_language);
        let mut tag = None;
        self.stored()
            .store
            .scan_designations(concept, |d| {
                if d.active
                    && d.use_ordinal == keys.fsn
                    && crate::stored::has_primary_subtag(d.language, &wanted)
                {
                    tag = tag_of(d.term);
                }
                tag.is_none()
            })
            .map_err(storage)?;
        Ok(tag)
    }
}

/// The phrase inside the last brackets that close a fully specified name.
fn tag_of(term: &str) -> Option<String> {
    let inner = term.trim_end().strip_suffix(')')?;
    let open = inner.rfind('(')?;
    inner
        .get(open.saturating_add(1)..)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
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
        let PropertyValue::Code(code) = &property.value else {
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
    use super::{date_of, push_once, tag_of};

    #[test]
    fn the_semantic_tag_is_the_last_bracketed_phrase_and_the_date_is_iso() {
        assert_eq!(
            tag_of("Repair of tendon of hand (procedure)").as_deref(),
            Some("procedure")
        );
        assert_eq!(
            tag_of("Structure (body structure) of hand (body structure)").as_deref(),
            Some("body structure")
        );
        assert_eq!(tag_of("No tag"), None);
        assert_eq!(tag_of("Empty ()"), None);
        assert_eq!(date_of("20050131"), "2005-01-31");
        assert_eq!(date_of("2005"), "2005");
    }
    use crate::provider::{Property, PropertyValue};

    #[test]
    fn a_value_repeated_in_another_role_group_is_listed_once() {
        let site = |code: &str| Property {
            code: String::from("363698007"),
            value: PropertyValue::Code(code.to_owned()),
            ..Property::default()
        };
        let mut out = Vec::new();
        push_once(&mut out, site("10200004"));
        push_once(&mut out, site("10200004"));
        push_once(&mut out, site("64033007"));
        assert_eq!(out, [site("10200004"), site("64033007")]);
    }
}
