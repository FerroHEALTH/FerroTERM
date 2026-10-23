---
name: tree-roving-tabindex-and-selection-keys
description: a flat ARIA tree with aria-level/posinset/setsize is the sanctioned shape, but the roving tabindex desyncs from real DOM focus without a focus listener, and selection keyed by code marks two rows in a poly-hierarchy
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Confirmed reviewing the FerroTERM concept browser tree
(`app/ferroterm-console/src/pages/browse.rs`, `src/tree.rs`), against
`.claude/rules/leptos-ui.md` §9 and the ARIA Authoring Practices tree view
pattern (<https://www.w3.org/WAI/ARIA/apg/patterns/treeview/>).

**What is right, and is worth copying.** A flat `<ul role="tree">` of sibling
`<li role="treeitem">` rows, each carrying `aria-level`, `aria-posinset`, and
`aria-setsize`, is spec-sanctioned: WAI-ARIA 1.2 requires those exact
attributes when the DOM ancestry does not represent the level
(<https://www.w3.org/TR/wai-aria-1.2/#aria-level>). No `role="group"` is owed.
Row identity is the PATH of codes from the anchor (percent-encoded, joined),
which keeps two rows distinct when one concept sits under two parents, and the
row `id` derived from it is looked up with `getElementById`, so no CSS-selector
escaping problem exists. The model (visible rows, key handling) lives in a
plain module with unit tests and no browser.

**Two defects the shape invites**, both found and fixed in that tree, so what
follows is what to look for in the next one.

1. **The roving tabindex desyncs from real DOM focus.** `tabindex="0"` is
   computed from a `focused` signal that only the arrow keys write. A click on
   a row focuses it in the browser (a `tabindex="-1"` element is
   click-focusable), the signal never learns, and the next arrow key walks from
   the wrong row. Fix: an `on:focusin` on the tree container that reads the
   row id back into the signal. The APG invariant is that the element with
   `tabindex="0"` IS the focused element.
2. **Selection keyed by code while rows are keyed by path.**
   `row.concept.code == params.code` marks EVERY row drawing that concept, so a
   poly-hierarchy (the normal case for SNOMED CT) gets two
   `aria-selected="true"` rows in a tree with no `aria-multiselectable`. Key
   the selected row by the same path the row is keyed by, or carry the path in
   the address.

A third, smaller one: a selected row whose only visible cue is a background
tint carries meaning by colour alone (WCAG 2.2 SC 1.4.1), the same finding as
[[sr-only-unavailable-is-colour-only]]. The row now carries the selection in
its font weight as well.

A fourth, from the same review: a row that names itself with `aria-label` keeps
the open/close link and any trailing note out of the name a screen reader
computes for it (<https://www.w3.org/TR/accname-1.2/#computation-steps>),
which a flat tree needs because the controls sit inside the treeitem.

Related: [[directory-tree-editor]] (state keyed positionally, the hazard this
tree correctly avoids).
