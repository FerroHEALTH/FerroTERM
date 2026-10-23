---
name: sr-only-unavailable-is-colour-only
description: a pager's disabled control that says ", unavailable" in sr-only text leaves a sighted reader with hue alone; fixed in listing.rs, still open in expand.rs
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The viewer's page controls render an available step as
`<a class="… text-brand-700 hover:underline">` and an unavailable one as
`<span class="… text-slate-500">{text} <span class="sr-only">", unavailable"</span></span>`
(`app/ferroterm-console/src/pages/expand.rs`, the expansion runner's pager). The
doc comment above it claims "a tint alone carries no meaning", but the words
that carry the meaning are `sr-only`, so the only cue a sighted reader gets is
the hue and the missing hover underline.

`src/listing.rs`, the pager the value set and concept map screens share, now
renders the word visibly instead. The expansion runner still carries the
original shape.

**Rule:** `.claude/rules/leptos-ui.md` §9, WCAG 2.2 SC 1.4.1 Use of Color
(<https://www.w3.org/TR/WCAG22/#use-of-color>): colour is not the only visual
means of conveying information or indicating an action.

**Fixes, in order:** render the state in visible text ("Previous page
(unavailable)", which is what `listing.rs` does), add a non-colour cue plus
`aria-disabled="true"`, or omit the control entirely on the page where it leads
nowhere.

**Review note:** an `sr-only` span is a screen-reader fix, never a 1.4.1 fix.
Whenever a review finds one justifying a colour difference, the sighted half is
still open.
