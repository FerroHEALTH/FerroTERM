---
name: live-region-inside-a-show-is-never-announced
description: an aria-live element nested in the Show whose condition first makes it true enters the tree with its first message, so the first result is never announced
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Found in review of #637, the viewer's version and restore screen.

An `aria-live` element is monitored for changes only once it is already in the
accessibility tree (<https://www.w3.org/TR/wai-aria-1.2/#aria-live>). Putting
one inside the `<Show>` whose condition is what first makes it exist means the
region and its first message arrive together: the first result announces
nothing, and only the second one does. The same screen's other region, created
at build time and left empty, is the correct shape.

**The fix is to hoist the region out of the gate, empty**, and gate only the
heading and the table under it. Where an empty region would look like a gap,
give it a sentence that says what to do next, which is content rather than
noise and still changes when the first result lands.

**A browser journey cannot catch this.** A `text_becoming` wait on the region's
id polls the DOM, and the text is there either way; what differs is whether the
assistive technology was watching. The check is in review.

Rules: `leptos-ui.md` §9 (accessibility), WCAG 2.2 Level AA.
