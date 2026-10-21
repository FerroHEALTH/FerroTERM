// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading `ConceptMap` resources from a directory, beside the `CodeSystem`s.

use std::path::Path;

use fhir_types::codec::{Json, Path as ElementPath, expect_object};

use super::convert;
use super::model::ConceptMapModel;
use crate::fhir_codesystem::load::{FhirVersion, LoadError, scan_json};

/// Loads every `ConceptMap` resource in a directory.
///
/// Files whose `resourceType` is not `ConceptMap` are skipped; the result is
/// sorted by file name so it is deterministic.
///
/// # Errors
///
/// Returns [`LoadError`] when the directory or a `ConceptMap` file fails.
pub fn load_dir(dir: &Path, version: FhirVersion) -> Result<Vec<ConceptMapModel>, LoadError> {
    let mut models = Vec::new();
    for (path, value) in scan_json(dir, "ConceptMap")? {
        let mut element = ElementPath::root("ConceptMap");
        let decode = |source| LoadError::Decode {
            path: path.clone(),
            version,
            resource_type: "ConceptMap",
            source,
        };
        let object = expect_object(&value, &element).map_err(decode)?;
        let model = match version {
            FhirVersion::R4 => convert::r4::convert(
                &fhir_types::r4::concept_map::ConceptMap::from_json(object, &mut element)
                    .map_err(decode)?,
            ),
            FhirVersion::R4B => convert::r4b::convert(
                &fhir_types::r4b::concept_map::ConceptMap::from_json(object, &mut element)
                    .map_err(decode)?,
            ),
            FhirVersion::R5 => convert::r5::convert(
                &fhir_types::r5::concept_map::ConceptMap::from_json(object, &mut element)
                    .map_err(decode)?,
            ),
            FhirVersion::R6 => convert::r6::convert(
                &fhir_types::r6::concept_map::ConceptMap::from_json(object, &mut element)
                    .map_err(decode)?,
            ),
        };
        models.push(model.map_err(|source| LoadError::ConceptMap {
            path: path.clone(),
            source,
        })?);
    }
    Ok(models)
}
