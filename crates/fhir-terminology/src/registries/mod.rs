// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry and grammar code systems: BCP 47 language tags, BCP 13 media
//! types, and ISO 3166-1 country codes.
//!
//! BCP 47 and BCP 13 are grammars over registries: a code is valid when it
//! parses and its parts are registered, and the systems cannot be enumerated
//! (<https://hl7.org/fhir/R4B/valueset-all-languages.html>). ISO 3166-1 is a
//! table, served as a complete code system. The registry files are vendored
//! under `data/` (`data/PROVENANCE.md`).

pub mod bcp13;
pub mod bcp47;
pub mod interned;
pub mod iso3166;
pub mod subtags;
pub mod ucum;
