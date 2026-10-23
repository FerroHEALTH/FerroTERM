// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The glyphs the screens carry, by what they mean here, each one of the
//! Lucide outlines `ferrohealth-ui` vendors.
//!
//! Every glyph sits beside text that already says what it means, and FerroUI
//! draws it `aria-hidden` (<https://www.w3.org/TR/wai-aria-1.2/#aria-hidden>),
//! so a screen reader announces the words alone.

use ferrohealth_ui::icon::Glyph;

/// The dashboard of everything this server serves.
pub(crate) const OVERVIEW: Glyph = Glyph::LAYOUT_DASHBOARD;

/// The code systems a server holds.
pub(crate) const CODE_SYSTEMS: Glyph = Glyph::BOOK_A;

/// Walking a hierarchy.
pub(crate) const BROWSE: Glyph = Glyph::NETWORK;

/// Expanding a value set.
pub(crate) const EXPAND: Glyph = Glyph::LAYERS;

/// The value sets a server holds.
pub(crate) const VALUE_SETS: Glyph = Glyph::FILES;

/// A code in one system reaching a code in another.
pub(crate) const CONCEPT_MAPS: Glyph = Glyph::ROUTE;

/// Translating a code through a concept map.
pub(crate) const TRANSLATE: Glyph = Glyph::ARROW_RIGHT;

/// Looking a code up in its system.
pub(crate) const LOOKUP: Glyph = Glyph::SEARCH_CODE;

/// The mark of the editor.
pub(crate) const EDIT: Glyph = Glyph::PENCIL;

/// The FHIR versions of one server.
pub(crate) const VERSION: Glyph = Glyph::SERVER;

/// The server answered.
pub(crate) const SERVING: Glyph = Glyph::CIRCLE_CHECK;

/// The server did not answer.
pub(crate) const UNREACHABLE: Glyph = Glyph::CIRCLE_X;

/// Checking a code against what a system holds.
pub(crate) const VALIDATE: Glyph = Glyph::SHIELD_CHECK;

/// About this server and console.
pub(crate) const ABOUT: Glyph = Glyph::INFO;

/// Finding something.
pub(crate) const SEARCH: Glyph = Glyph::SEARCH;
