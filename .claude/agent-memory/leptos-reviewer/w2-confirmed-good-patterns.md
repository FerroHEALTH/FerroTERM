---
name: w2-confirmed-good-patterns
description: patterns verified correct in the sibling viewer; the auth half is moot under CSR, the erasure, integer, and theme findings stand
csr: changed
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Confirmed correct in the sibling project's viewer review. Do not re-litigate
the ones that carry over.

**Carries over unchanged:**

- **Section-boundary `.into_any()` erasure.** Each screen section is bound to an
  erased local rather than built as one monolithic `view!` tree. This is
  REQUIRED, because a deeply nested component tree blows rustc's layout
  recursion depth at codegen. Do not suggest collapsing the sections.
- **No `usize`/`isize` in a serialized type.** WebAssembly is 32-bit; use
  fixed-size integers in anything that crosses the wire or is stored. The
  sibling used `u16` for a status code, which is the right shape.
- **The theme `Effect`** reads `localStorage` and writes the theme signals. That
  is a legitimate sync with the outside world, NOT a forbidden
  signal-writes-signal effect (`.claude/rules/leptos-ui.md` §2). A fixed theme
  id keeps the thaw style selector deterministic.
- **No `unwrap`/`expect` outside tests, zero re-exports, no `use X as Y`, every
  public item documented.** All of that is workspace discipline and is
  unchanged.

**Moot under CSR:**

- The server-function authentication findings. Every `#[server]` fn calling a
  session guard first was the sibling project's core rule. This viewer declares
  no server function and holds no session, so the whole class is absent. If a
  review here starts talking about server-function auth, the reviewer has the
  wrong project loaded.

See [[thaw-field-random-id]] and [[thaw-input-name-forwarding-ok]] for the thaw
form facts.
