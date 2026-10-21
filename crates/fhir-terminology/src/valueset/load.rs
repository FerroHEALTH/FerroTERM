// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading `ValueSet` resources from a directory, beside the `CodeSystem`s.

use std::path::Path;

use fhir_types::codec::{Json, Path as ElementPath, expect_object};

use super::convert;
use super::model::ValueSetModel;
use crate::fhir_codesystem::load::{FhirVersion, LoadError, scan_json};

/// Loads every `ValueSet` resource in a directory.
///
/// Files whose `resourceType` is not `ValueSet` are skipped; the result is
/// sorted by file name so it is deterministic.
///
/// # Errors
///
/// Returns [`LoadError`] when the directory or a `ValueSet` file fails.
pub fn load_dir(dir: &Path, version: FhirVersion) -> Result<Vec<ValueSetModel>, LoadError> {
    let mut models = Vec::new();
    for (path, value) in scan_json(dir, "ValueSet")? {
        models.push(
            model_from_value(&value, version).map_err(|decoded| match decoded {
                Decoded::Decode(source) => LoadError::Decode {
                    path: path.clone(),
                    version,
                    resource_type: "ValueSet",
                    source,
                },
                Decoded::Model(source) => LoadError::ValueSet {
                    path: path.clone(),
                    source,
                },
            })?,
        );
    }
    Ok(models)
}

/// A decode or model failure before its source is known.
pub(crate) enum Decoded {
    Decode(fhir_types::codec::DecodeError),
    Model(super::model::ModelError),
}

/// The model of one `ValueSet` resource written in `version`.
pub(crate) fn model_from_value(
    value: &fhir_types::codec::Value,
    version: FhirVersion,
) -> Result<ValueSetModel, Decoded> {
    let mut element = ElementPath::root("ValueSet");
    let object = expect_object(value, &element).map_err(Decoded::Decode)?;
    let model = match version {
        FhirVersion::R4 => convert::r4::convert(
            &fhir_types::r4::value_set::ValueSet::from_json(object, &mut element)
                .map_err(Decoded::Decode)?,
        ),
        FhirVersion::R4B => convert::r4b::convert(
            &fhir_types::r4b::value_set::ValueSet::from_json(object, &mut element)
                .map_err(Decoded::Decode)?,
        ),
        FhirVersion::R5 => convert::r5::convert(
            &fhir_types::r5::value_set::ValueSet::from_json(object, &mut element)
                .map_err(Decoded::Decode)?,
        ),
        FhirVersion::R6 => convert::r6::convert(
            &fhir_types::r6::value_set::ValueSet::from_json(object, &mut element)
                .map_err(Decoded::Decode)?,
        ),
    };
    let model = model.map_err(Decoded::Model)?;
    if model.url.is_empty() {
        return Err(Decoded::Model(super::model::ModelError::NoUrl));
    }
    Ok(model)
}
