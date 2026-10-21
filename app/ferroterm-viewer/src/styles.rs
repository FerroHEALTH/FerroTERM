// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The console's class vocabulary: the FerroUI components' classes a screen
//! paints with, under the names the screens use.
//!
//! A screen names a component class from `ferrohealth_ui` or a role, a type
//! step and a spacing step of its stylesheet, and never a palette entry, a
//! raw size, or a `dark:` colour; `scripts/checks/viewer-tokens.sh` keeps it
//! so. Every pairing here is one FerroUI's contrast test measures in every
//! theme of the FerroTERM brand, and the painted-colour journey in
//! `e2e/tests/it/accessibility.rs` measures it as the browser draws it
//! (<https://www.w3.org/TR/WCAG22/#contrast-minimum>).

use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;

/// A label over a group, in small caps, for a thing that is not a heading.
pub(crate) const EYEBROW: &str = "fui-label";

/// Body prose that supports something else.
pub(crate) const MUTED: &str = "fui-muted";

/// The quietest readable text: a hint under a control, a note beside a figure.
pub(crate) const HINT: &str = "fui-help";

/// An identifier, a code, or a canonical.
pub(crate) const CODE: &str = "fui-mono fui-break";

/// An identifier that supports something else.
pub(crate) const CODE_MUTED: &str = "fui-mono fui-muted fui-break";

/// A table that compares rows; FerroUI styles its header and cells.
pub(crate) const TABLE: &str = "fui-table";

/// A table's header cell, which the table keeps on one line.
pub(crate) const TH: &str = "";

/// A table's cell, which the table aligns to the top of its row.
pub(crate) const TD: &str = "";

/// A table cell whose content is one short token.
pub(crate) const TD_TIGHT: &str = "";

/// The control that submits a form.
pub(crate) const SUBMIT: &str = Variant::Primary.class();

/// A control that acts without submitting.
pub(crate) const BUTTON: &str = Variant::Secondary.class();

/// A control with no chrome until it is pointed at.
pub(crate) const BUTTON_QUIET: &str = Variant::Ghost.class();

/// A text input.
pub(crate) const INPUT: &str = ferrohealth_ui::view::form::INPUT;

/// The label over an input.
pub(crate) const LABEL: &str = "fui-label";

/// A standing mark: a state a resource or a version is in.
pub(crate) const BADGE: &str = Tone::Neutral.badge();

/// A standing mark for something that is working.
pub(crate) const BADGE_OK: &str = Tone::Ok.badge();

/// A standing mark for something that failed.
pub(crate) const BADGE_DANGER: &str = Tone::Bad.badge();

/// A standing notice about the answer being shown.
pub(crate) const NOTICE: &str = "fui-notice fui-tone-warn";

/// A link inside prose; the components' base layer draws every link.
pub(crate) const LINK: &str = "";

/// The mark for a fact the server did not state.
///
/// An em dash, not a sentence. Eleven cells that each say "Not loaded from an
/// artifact" make absence the densest thing on a screen; the sentence belongs
/// in the mark's own title, where a reader who wants it can find it.
pub(crate) const ABSENT: &str = "fui-muted";

/// The character that mark draws.
pub(crate) const ABSENT_MARK: &str = "\u{2014}";
