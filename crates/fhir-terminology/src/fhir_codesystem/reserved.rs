// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code spaces a code system reserves for its users' own codes
//! (`codesystem-reservedCodeSpace`).
//!
//! "A code in a reserved code space is a valid code in the code system,
//! whether or not anything in scope defines it. When nothing does, its display
//! is the code space's default display"
//! (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, `CodeSystem`
//! supplements). The space cannot be enumerated, so each code is interned the
//! first time it is met, after the ordinals of the concepts the resource
//! defines.

use regex::Regex;

use crate::provider::code_system::ReservedCodeSpace;
use crate::provider::error::ProviderError;
use crate::provider::{Concept, Located};
use crate::registries::interned::Interned;

/// The display of a reserved code nothing defines, when its space states none
/// (the terminology ecosystem's `User-assigned`).
const USER_ASSIGNED: &str = "User-assigned";

/// The reserved code spaces of one code system, and the codes met in them.
#[derive(Debug)]
pub(super) struct Reserved {
    /// Each space's anchored pattern, with the display of a code in it.
    spaces: Vec<(Regex, String)>,
    /// The reserved codes met so far.
    interned: Interned,
    /// The first ordinal past the concepts the resource defines.
    base: u32,
}

impl Reserved {
    /// The spaces `declared` states, numbered from `base`.
    ///
    /// # Errors
    ///
    /// Returns the [`regex::Error`] of a pattern that is no regular expression.
    pub(super) fn new(declared: &[ReservedCodeSpace], base: u32) -> Result<Self, regex::Error> {
        let mut spaces = Vec::with_capacity(declared.len());
        for space in declared {
            // NOTE: "a regular expression that a code must match in full"
            // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), so the pattern is anchored.
            let pattern = Regex::new(&format!("^(?:{})$", space.pattern))?;
            let display = space
                .default_display
                .clone()
                .unwrap_or_else(|| USER_ASSIGNED.to_owned());
            spaces.push((pattern, display));
        }
        Ok(Self {
            spaces,
            interned: Interned::default(),
            base,
        })
    }

    /// Whether the system reserves any code space.
    pub(super) fn is_empty(&self) -> bool {
        self.spaces.is_empty()
    }

    /// The reserved code `code`, interned, when a space holds it.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the ordinals run out.
    pub(super) fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        if !self
            .spaces
            .iter()
            .any(|(pattern, _)| pattern.is_match(code))
        {
            return Ok(None);
        }
        let interned = self.interned.intern(code)?;
        let ordinal = self
            .base
            .checked_add(interned.index())
            .ok_or_else(|| ProviderError::Storage(Box::new(Exhausted)))?;
        Ok(Some(Located {
            concept: Concept::new(ordinal),
            code: code.to_owned(),
        }))
    }

    /// The reserved code at `concept`, when it is one.
    pub(super) fn code(&self, concept: Concept) -> Option<String> {
        let offset = concept.index().checked_sub(self.base)?;
        self.interned.code(Concept::new(offset))
    }

    /// The default display of the reserved code at `concept`, when it is one.
    pub(super) fn display(&self, concept: Concept) -> Option<String> {
        let code = self.code(concept)?;
        self.spaces
            .iter()
            .find(|(pattern, _)| pattern.is_match(&code))
            .map(|(_, display)| display.clone())
    }
}

/// The ordinals past the defined concepts are all taken.
#[derive(Debug, thiserror::Error)]
#[error("the reserved code spaces have more codes than the provider can number")]
struct Exhausted;
