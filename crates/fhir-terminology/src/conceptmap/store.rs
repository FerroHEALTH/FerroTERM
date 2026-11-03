// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept maps a server holds, by `url` and `version`.

use crate::provider::concept_map::ConceptMapModel;
use crate::versioned::VersionedStore;

/// The concept maps by `url`, then `version`.
pub type ConceptMapStore = VersionedStore<ConceptMapModel>;
