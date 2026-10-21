---
name: a-write-control-built-from-a-url-memo-and-an-old-answer
description: under Transition the new address meets the previous answer, so a write control drawn from both sends one resource's body to another resource's id
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Found in review of #637, the viewer's version and restore screen.

A section built inside a `<Transition>` child closure that reads BOTH a
`LocalResource` and a URL-derived `Memo` pairs the new address with the old
answer for the length of the refetch. `<Transition>` keeps the previous
children mounted on purpose (the book, `async/12_transition`), and
`LocalResource` keeps the previous value until the new future resolves, so the
rows, the chosen document and the `If-Match` value are resource A's while the
id the control writes to is resource B's.

For a read-only screen that is a stale view for a moment. For a write control
it is a wrong write: a `412` when the two version identifiers disagree, and a
silent overwrite of B with A's document when they agree.

**The fix is to carry the address the read was made FOR inside the answer**,
and to draw every section that writes from the answer alone. In #637 that is a
`subject: Subject` field on the resource's own answer type, set where the read
is issued. Only the heading, which says what the screen is reading rather than
what it read, keeps reading the memo.

The same-route navigation that opens the window is the confirmed one in
[[router-same-route-param-nav]]: the params signal updates and the component
body never re-runs. The read-only half of this hazard is
[[transition-mixes-new-params-old-answer]].

Rules: `leptos-ui.md` §6 (async data), §8 (router).
