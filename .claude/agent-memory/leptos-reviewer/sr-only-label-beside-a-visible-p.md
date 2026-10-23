---
name: sr-only-label-beside-a-visible-p
description: a control with an sr-only label drawn under a visible p class=LABEL passes the e2e label check and still fails SC 2.5.3 Label in Name
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The shape: a visible `<p class=styles::LABEL>{"Relationship (equivalence)"}</p>`
over a `<select>` whose own `<label for=...>` is `sr-only` and reads something
else ("Relationship to the source code"). Seen in
`app/ferroterm-console/src/pages/concept_map_editor.rs`, `relationship_control`
(#636).

Two failures:

- **SC 2.5.3 Label in Name** (Level A,
  <https://www.w3.org/TR/WCAG22/#label-in-name>): the accessible name must
  contain the text presented visually. It does not, so speech input cannot
  address the control by the words on the screen.
- **SC 1.3.1 Info and Relationships**
  (<https://www.w3.org/TR/WCAG22/#info-and-relationships>): the visible words
  are associated with nothing, so a screen reader never hears them. When the
  visible text is the one thing that differs per FHIR version, that difference
  reaches nobody.

**The gate does not catch it.** `e2e/tests/it/accessibility.rs:301-304` asks
only whether a control has SOME label (`label[for]`, an ancestor `<label>`,
`aria-label`, `aria-labelledby`), and the `sr-only` label satisfies it.
Reviewers carry this one.

**Fixes, in order:** make the visible element the `<label for>` and delete the
`sr-only` copy, which needs the shared control to take an owned or reactive
label rather than a `&'static str`; or give the `<p>` an id and point the
control's `aria-labelledby` at it.

Related: [[sr-only-unavailable-is-colour-only]] (the other half of the same
mistake: `sr-only` text is never the fix for a sighted reader's problem).
