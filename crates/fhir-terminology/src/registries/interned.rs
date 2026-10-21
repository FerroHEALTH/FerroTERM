// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Concept ordinals for a grammar system: codes are interned as they are
//! met, so a valid code has a stable ordinal for the life of the provider.

use std::collections::BTreeMap;
use std::sync::{PoisonError, RwLock};

use crate::provider::{Concept, ProviderError};

/// The codes a grammar system has met, by canonical spelling, each with the
/// ordinal it answers on.
#[derive(Debug, Default)]
pub struct Interned {
    inner: RwLock<Table>,
}

#[derive(Debug, Default)]
struct Table {
    by_code: BTreeMap<String, u32>,
    codes: Vec<String>,
}

impl Interned {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The ordinal of `canonical`, assigned now when new.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the table cannot grow past
    /// `u32::MAX` codes.
    pub fn intern(&self, canonical: &str) -> Result<Concept, ProviderError> {
        if let Some(index) = self
            .inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .by_code
            .get(canonical)
        {
            return Ok(Concept::new(*index));
        }
        let mut table = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = table.by_code.get(canonical) {
            return Ok(Concept::new(*index));
        }
        let index =
            u32::try_from(table.codes.len()).map_err(|e| ProviderError::Storage(Box::new(e)))?;
        table.codes.push(canonical.to_owned());
        table.by_code.insert(canonical.to_owned(), index);
        Ok(Concept::new(index))
    }

    /// The canonical code at `concept`, when one was interned there.
    #[must_use]
    pub fn code(&self, concept: Concept) -> Option<String> {
        self.inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .codes
            .get(concept_graph::ordinal::to_usize(concept.index()))
            .cloned()
    }

    /// The number of codes met so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .codes
            .len()
    }

    /// Whether no code was met yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
