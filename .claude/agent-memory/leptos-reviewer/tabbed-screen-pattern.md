---
name: tabbed-screen-pattern
description: the tabbed-screen shape; always-mounted bodies were a hydration device and are optional under CSR, tab-gated resource sources are the part that matters
csr: changed
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The sibling project's correct multi-tab screen had two halves:

1. **All tab bodies always mounted**, toggled with
   `class:hidden=move || selected.get() != "x"`, so the server HTML and the
   client view had identical structure.
2. **Each tab's resource source gated on the active tab**:
   `move || (selected.get() == "x").then(|| id.get())`, with the fetcher
   returning the empty answer when inactive, so only the visible tab issues a
   request.

**What changed under CSR.** Half 1 existed for hydration structure stability and
is no longer required: a `<Show>` per tab that mounts only the active body is
fine, and it is cheaper. Keep always-mounted bodies only where a tab holds
scroll position or in-progress input worth preserving, which is a UX decision
rather than a correctness one.

**What still applies, and is now the whole point.** Half 2. A screen with four
tabs that fetches all four on mount fans out four requests for one page view,
and on a terminology server an inactive tab can be an expensive expansion.
Gate every tab's resource source on the active tab. Gating also preserves
loaded state, because a stable source does not refetch when the tab is shown
again.

**Standing nit, unchanged:** tab selection belongs in the URL, not a private
signal, or a refresh and a deep link both lose the active tab
(`.claude/rules/leptos-ui.md` §8, state in the URL).
