// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an expansion entry carries beyond its code: the extensions the code
//! system or the value set states on the concept, and the mark on an inactive
//! designation (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>,
//! Extensions and Inactive, Deprecated and Not-Selectable Codes).

use super::super::{OperationError, Sources};
use crate::compose::Item;
use crate::provider::compose::{Compose, Echoes};
use crate::provider::{Designation, DesignationUse};

/// The SNOMED CT concept "Inactive value", the use code the ecosystem names for
/// an inactive designation.
const INACTIVE_USE_SYSTEM: &str = "http://snomed.info/sct";
const INACTIVE_USE_CODE: &str = "900000000000546006";

/// The echoes of `item`: the value set's own, then its code system's for each
/// one the value set leaves unstated. The value set's definition is echoed
/// only when the request asks for definitions ("populate if requested in
/// expansion request").
///
/// # Errors
///
/// Returns [`OperationError`] when the item's code system cannot be resolved.
pub(super) fn echoes_of(
    sources: &Sources<'_>,
    compose: &Compose,
    item: &Item,
    definitions: bool,
) -> Result<Echoes, OperationError> {
    let stated = compose
        .include
        .iter()
        .filter(|i| i.system.as_ref().is_some_and(|s| s.url == item.system))
        .flat_map(|i| &i.concepts)
        .find(|c| c.code == item.code)
        .map(|c| c.echoes.clone())
        .unwrap_or_default();
    let resolved = sources
        .registry
        .resolve(&item.system, Some(&item.version))?;
    let defined = resolved
        .provider
        .code_system()
        .and_then(|model| model.concepts.iter().find(|c| c.code == item.code))
        .map(|c| c.echoes.clone())
        .unwrap_or_default();
    let mut echoes = stated.or(defined);
    if !definitions {
        echoes.definition = None;
    }
    Ok(echoes)
}

/// The designations an expansion entry lists, each inactive one marked. A
/// designation whose `structuredefinition-standards-status` is `withdrawn` or
/// `deprecated` takes the SNOMED CT inactive use code, since that extension's
/// context does not admit `ValueSet.expansion.contains.designation` (FHIR
/// Extensions Pack 5.4.0). One that already states a use cannot carry both in
/// `contains.designation`, so it is left out of the expansion.
pub(super) fn mark_inactive(designations: Vec<Designation>) -> Vec<Designation> {
    designations
        .into_iter()
        .filter_map(|mut designation| {
            let inactive = matches!(
                designation.standards_status.as_deref(),
                Some("withdrawn" | "deprecated")
            );
            if !inactive {
                return Some(designation);
            }
            if designation.use_.is_some() {
                return None;
            }
            designation.use_ = Some(DesignationUse {
                system: String::from(INACTIVE_USE_SYSTEM),
                code: String::from(INACTIVE_USE_CODE),
                display: Some(String::from("Inactive value")),
            });
            Some(designation)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn designation(status: Option<&str>, use_: Option<&str>) -> Designation {
        Designation {
            language: Some(String::from("de")),
            use_: use_.map(|code| DesignationUse {
                system: String::from("http://snomed.info/sct"),
                code: String::from(code),
                display: None,
            }),
            value: String::from("2nd Code"),
            standards_status: status.map(str::to_owned),
        }
    }

    #[test]
    fn an_inactive_designation_takes_the_inactive_use_code() {
        let marked = mark_inactive(vec![
            designation(Some("withdrawn"), None),
            designation(Some("deprecated"), None),
            designation(Some("trial-use"), None),
            designation(None, None),
        ]);
        let uses: Vec<Option<&str>> = marked
            .iter()
            .map(|d| d.use_.as_ref().map(|u| u.code.as_str()))
            .collect();
        assert_eq!(
            uses,
            [Some(INACTIVE_USE_CODE), Some(INACTIVE_USE_CODE), None, None]
        );
    }

    #[test]
    fn an_inactive_designation_with_a_use_is_left_out() {
        let marked = mark_inactive(vec![
            designation(Some("withdrawn"), Some("900000000000013009")),
            designation(None, Some("900000000000013009")),
        ]);
        assert_eq!(marked.len(), 1);
        assert_eq!(marked[0].standards_status, None);
    }
}
