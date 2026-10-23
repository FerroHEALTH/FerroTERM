---
name: memo-source-gates-a-resource-refetch
description: verified in reactive_graph 0.2.14, an RwSignal read through Signal::derive refetches a resource on every write, and a Memo in the same place does not
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in the pinned source (reactive_graph 0.2.14).

The resource's fetcher loop is
`computed/async_derived/arc_async_derived.rs:325-333`: on every notification it
runs `any_subscriber.update_if_necessary()` and re-runs the future only when
that returns true.

- **A plain signal source re-runs it always.** A write calls `mark_dirty` on
  its subscribers, and `computed/async_derived/inner.rs:37-42` sets the async
  derived's state to `Dirty`; `update_if_necessary` (`inner.rs:59-65`) returns
  `true` for `Dirty` before it looks at any source. So the fetcher re-runs even
  when the value the closure computed is unchanged.
- **A `Memo` source gates it.** The memo takes the `mark_dirty`, its
  subscribers get `mark_check` (`inner.rs:47-50`), and the async derived's
  `update_if_necessary` walks its sources (`inner.rs:71-77`): the memo
  recomputes, compares with `PartialEq`, and returns `false` when the value is
  the same, so the fetcher does not re-run.

`Signal::derive` is not a node at all: reading it inside the fetcher's tracking
scope subscribes the resource to whatever the closure read, so
`Signal::derive(move || draft.with(|d| d.field.clone()))` subscribes the
resource to the WHOLE `draft` signal.

**The consequence to look for in review** (`.claude/rules/leptos-ui.md` section 6):
a per-row `LocalResource` in an editing form whose source reads one field of the
form's own draft through a derive. Every keystroke in every other field of the
form re-issues that row's HTTP request. Found in
`app/ferroterm-console/src/pages/concept_map_editor.rs` (the code picker's
`system` source, #636).

**The fix:** `Memo::new(move |_| ...)`, which needs the projected type to derive
`PartialEq`. The book's rule that a signal depending on a signal is a derived
closure or a memo is the same rule seen from the read side
(`reactivity/working_with_signals`).

Related: [[resource-read-registers-suspense]] (the same gap, from the suspense
side), [[prop-value-rebuild-has-no-equality-gate]] (the same gap, from the DOM
side).
