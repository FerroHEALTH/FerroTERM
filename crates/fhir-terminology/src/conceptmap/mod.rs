// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap` resources as `$translate` evaluates them.
//!
//! A `ConceptMap` of any served FHIR version reduces to one
//! [`ConceptMapModel`]; R4's `equivalence` and R5's `relationship`
//! vocabularies meet in [`crate::provider::concept_map::Relationship`]. Maps
//! are kept in a [`store::ConceptMapStore`] by `url` and `version`.

pub mod convert;
pub mod load;
pub mod render;
pub mod store;

use crate::provider::concept_map::ConceptMapModel;
use crate::versioned::Versioned;

impl Versioned for ConceptMapModel {
    fn url(&self) -> &str {
        &self.url
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}
