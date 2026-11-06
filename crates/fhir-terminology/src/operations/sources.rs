// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where an operation finds its code systems, value sets and concept maps.

use std::sync::Arc;

use super::{OperationError, Refused, RefusedInput};
use crate::registry::Registry;

/// Where an operation finds its code systems and value sets.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    /// The code systems.
    pub registry: &'a Registry,
    /// The value sets.
    pub value_sets: &'a crate::valueset::store::ValueSetStore,
    /// The concept maps.
    pub concept_maps: &'a crate::conceptmap::store::ConceptMapStore,
}

impl<'a> Sources<'a> {
    /// These sources with the dormant supplements `wanted` names layered over
    /// their systems, when any is named; the borrowed sources otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError::UnknownSupplement`] for a name no loaded
    /// supplement answers to.
    pub fn with_supplements(
        &self,
        wanted: &[String],
    ) -> Result<std::borrow::Cow<'a, Registry>, OperationError> {
        if wanted.is_empty() {
            return Ok(std::borrow::Cow::Borrowed(self.registry));
        }
        Ok(std::borrow::Cow::Owned(
            self.registry.with_supplements(wanted)?,
        ))
    }

    /// The value set an operation names: inline, stored by `url` (and
    /// `version`), or a provider's implicit form.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError::Invalid`] for both an inline and a `url`,
    /// [`OperationError::Required`] for neither, and
    /// [`OperationError::UnknownValueSet`] when nothing answers the `url`.
    pub fn value_set(
        &self,
        inline: Option<
            Result<crate::valueset::model::ValueSetModel, crate::valueset::model::ModelError>,
        >,
        url: Option<&str>,
        version: Option<&str>,
    ) -> Result<Arc<crate::valueset::model::ValueSetModel>, OperationError> {
        match (inline, url) {
            (Some(_), Some(_)) => Err(OperationError::Invalid(String::from(
                "provide either `url` or an inline `valueSet`, not both",
            ))),
            (Some(model), None) => {
                Ok(Arc::new(model.map_err(|e| {
                    OperationError::value_set_invalid(e.to_string())
                })?))
            }
            (None, Some(url)) => {
                if let Some(model) = self.value_sets.resolve(url, version) {
                    return Ok(model);
                }
                match self.registry.implicit_value_set(url) {
                    Some(Ok(compose)) => {
                        let metadata = self.registry.implicit_metadata(url);
                        Ok(Arc::new(crate::valueset::model::ValueSetModel {
                            // NOTE: an implicit value set has no resource, and a
                            // logical id belongs to one
                            // (<https://hl7.org/fhir/R4B/resource.html#id>).
                            id: None,
                            expansion_parameters: Vec::new(),
                            language: None,
                            url: url.to_owned(),
                            version: metadata.version,
                            supplements: Vec::new(),
                            standards_status: None,
                            name: metadata.name,
                            title: metadata.title,
                            status: String::from("active"),
                            experimental: metadata.experimental,
                            date: metadata.date,
                            publisher: None,
                            description: metadata.description,
                            copyright: metadata.copyright,
                            immutable: None,
                            compose,
                            contained: std::collections::BTreeMap::new(),
                        }))
                    }
                    Some(Err(source)) => {
                        let refused = Refused {
                            input: RefusedInput::Url,
                            text: url.to_owned(),
                            position: source.position(),
                        };
                        Err(OperationError::from(source).pointing(refused))
                    }
                    None => Err(OperationError::UnknownValueSet(match version {
                        Some(version) => format!("{url}|{version}"),
                        None => url.to_owned(),
                    })),
                }
            }
            (None, None) => Err(OperationError::Required(String::from(
                "a value set is required: the `url` parameter or an inline `valueSet`",
            ))),
        }
    }
}
