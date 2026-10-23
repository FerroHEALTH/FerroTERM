// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The screen logic behind the authoring screens, which only the editor
//! bundle carries.
//!
//! Everything here is plain values and plain functions: what a resource is
//! while it is being authored, what a save sends, what two versions of one
//! resource differ by. No component reaches into it, which is what lets the
//! rules it encodes be pinned by ordinary unit tests.
//!
//! The modules live under one directory rather than at the crate root so the
//! code-system-neutrality guard in `crate::pages` reads them: the guard walks
//! directories, so a screen's logic added here is covered the day it lands
//! rather than the day someone remembers to list it.

pub(crate) mod code_system;
pub(crate) mod concept_map;
pub(crate) mod history;
