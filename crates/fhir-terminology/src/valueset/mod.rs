// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet` resources as the compose layer evaluates them.
//!
//! A `ValueSet` of any served FHIR version reduces to one
//! [`model::ValueSetModel`] (its identity and its `compose`), kept in a
//! [`store::ValueSetStore`] by `url` and `version`; [`store::Resolver`]
//! answers `include.valueSet` references from the store and the providers'
//! implicit value sets, refusing a cycle.

pub mod convert;
pub mod load;
pub mod model;
pub mod negotiation;
pub mod render;
pub mod store;
