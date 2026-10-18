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

/// A raised surface: a card, a pane, a panel.
pub(crate) const PANEL: &str = "fui-card";

/// A label over a group, in small caps, for a thing that is not a heading.
pub(crate) const EYEBROW: &str = "text-micro font-semibold tracking-wide text-muted uppercase";

/// Body prose that supports something else.
pub(crate) const MUTED: &str = "text-small text-muted";

/// The quietest readable text: a hint under a control, a note beside a figure.
pub(crate) const HINT: &str = "fui-help";

/// An identifier, a code, or a canonical.
pub(crate) const CODE: &str = "fui-mono text-ink wrap-break-word";

/// An identifier that supports something else.
pub(crate) const CODE_MUTED: &str = "fui-mono text-muted wrap-break-word";

/// A table that compares rows; FerroUI styles its header and cells.
pub(crate) const TABLE: &str = "fui-table";

/// A table's header cell, styled by the table.
pub(crate) const TH: &str = "whitespace-nowrap";

/// A table's cell, styled by the table.
pub(crate) const TD: &str = "align-top";

/// A table cell whose content is one token and must not wrap.
pub(crate) const TD_TIGHT: &str = "align-top whitespace-nowrap";

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
// TODO(#730): the inline notice of FerroUI issue 11 replaces this tint.
pub(crate) const NOTICE: &str =
    "rounded-md border border-edge bg-warn-ground p-default text-small text-warn";

/// A link inside prose; the components' base layer draws every link.
pub(crate) const LINK: &str = "";

/// The mark for a fact the server did not state.
///
/// An em dash, not a sentence. Eleven cells that each say "Not loaded from an
/// artifact" make absence the densest thing on a screen; the sentence belongs
/// in the mark's own title, where a reader who wants it can find it.
pub(crate) const ABSENT: &str = "text-muted";

/// The character that mark draws.
pub(crate) const ABSENT_MARK: &str = "\u{2014}";

/// The indentation of a row at each depth of a hierarchy, written as classes
/// so no row carries a `style` attribute the policy would refuse.
// TODO(#730): FerroUI's depth classes (FerroUI issue 37) replace these.
const DEPTHS: [&str; 9] = [
    "depth-0", "depth-1", "depth-2", "depth-3", "depth-4", "depth-5", "depth-6", "depth-7",
    "depth-8",
];

/// The indentation class of a row at `depth`, deeper rows at the last step.
pub(crate) fn depth(depth: u32) -> &'static str {
    let index = usize::try_from(depth).unwrap_or(usize::MAX);
    DEPTHS
        .get(index)
        .or(DEPTHS.last())
        .copied()
        .unwrap_or_default()
}
