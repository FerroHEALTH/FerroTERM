// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap` resources as `$translate` evaluates them.
//!
//! A `ConceptMap` of any served FHIR version reduces to one
//! [`model::ConceptMapModel`]; R4's `equivalence` and R5's `relationship`
//! vocabularies meet in [`model::Relationship`]. Maps are kept in a
//! [`store::ConceptMapStore`] by `url` and `version`.

pub mod convert;
pub mod load;
pub mod model;
pub mod render;
pub mod store;
