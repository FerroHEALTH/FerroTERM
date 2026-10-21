---
name: resource-read-registers-suspense
description: a plain .with()/.get() read of a resource registers with the enclosing SuspenseContext, and a Signal::derive resource source has no equality gate, so an unrelated notify refetches
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in `reactive_graph` 0.2.14,
`src/computed/async_derived/arc_async_derived.rs:629-665`.

- `ReadUntracked::try_read_untracked` calls `use_context::<SuspenseContext>()`
  and, when one is in scope, takes a task handle and pushes the context onto
  the derived value's `suspenses` list. So `resource.with(|answered| …)` inside
  `<Suspense>`/`<Transition>` suspends correctly; `<Suspend>` and `.await` are
  not the only shapes that work. Do not file a finding asking for `Suspend`
  when a `.with()` read is already inside the boundary.
- The lookup is by CONTEXT, so a read taken outside the boundary (a `Memo` for
  a live region, built in the section body) never registers, which is what
  makes such a memo update independently of the fallback.

**The other half, which is a real finding shape.** A resource source built from
`Signal::derive` has no equality gate: the derived closure re-runs whenever any
signal it touched notifies, and the resource refetches on the notify, not on a
value change. A source that reads a settings signal (a page size, a display
language) therefore issues a fresh request when that signal is written even
though the request is identical. `Memo::new` over a `PartialEq` request type is
the guard; recommend it wherever a resource source reads more than the route.

Related: [[polled-resource-needs-transition]]
