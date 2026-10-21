// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A client for the Atom syndication dialect terminology services publish.
//!
//! National terminology services announce their downloadable content in an
//! Atom feed (RFC 4287, <https://www.rfc-editor.org/rfc/rfc4287>) carrying the
//! NCTS Atom Syndication Format extensions, which Ontoserver documents at
//! <https://www.ontoserver.csiro.au/docs/6.22.5/syndication.html>. This crate
//! reads that feed into a typed model ([`model`], [`parse`]), decides what a
//! run takes from it ([`select`]), fetches one content item with its digest
//! verified ([`download`]), and offers [`source::Source`] as the seam each
//! service add-on implements. A service that keeps its FHIR resources behind
//! its FHIR API rather than in the feed is listed through [`fhir_api`] into
//! the same entry model, so one selection and one lane serve both.
//!
//! Nothing here is specific to a code system, a country, or an operator: the
//! feed dialect is the same everywhere it is served, and a service's identity
//! lives entirely in its add-on.
#![doc(test(attr(deny(warnings))))]

pub mod download;
pub mod fhir_api;
pub mod model;
pub mod parse;
pub mod select;
pub mod source;
