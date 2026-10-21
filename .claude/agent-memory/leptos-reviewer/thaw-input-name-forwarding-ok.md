---
name: thaw-input-name-forwarding-ok
description: thaw::Input forwards an explicit name to the real <input>; recorded for ActionForm progressive enhancement, which does not exist under CSR
csr: moot
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

`thaw::Input` renders `name=name` on the underlying `<input>` element (pinned
thaw rev `0726a3d`, `thaw/src/input/mod.rs:166`), and `Field::use_id_and_name`
honours an explicit `name` prop first.

**Why it was recorded.** In the sibling project it proved that a
`<thaw::Input name="username">` inside an `<ActionForm>` submits its field under
progressive enhancement, with no WebAssembly loaded. The finding was "do not
flag this as missing name forwarding".

**Moot here.** There is no `<ActionForm>`, no server function, and no
no-JavaScript path, so nothing depends on native form submission carrying the
field.

**The residue.** A `name` is still worth passing: it is what a plain
`<form method="GET">` uses to build the query string when the viewer navigates
its own URL state (`.claude/rules/leptos-ui.md` §8), and it is what makes a form
legible in the browser's developer tools. When a `name` is passed, verify the
string matches the query parameter the screen reads back.
