---
name: for-retained-key-never-rebuilds
description: a <For> row whose key is unchanged is never rebuilt, so a child taking a plain (non-signal) value prop keeps stale data after a refetch; a plain Vec<AnyView> rebuilds positionally and does refresh
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in the pinned sources (leptos 0.8.20, tachys 0.2.18).

**`<For>` only builds ADDED keys.** `leptos/src/for_loop.rs:144` is
`move || keyed(each(), key.clone(), children.clone())`, and
`tachys/src/view/keyed.rs:149` `Keyed::rebuild` diffs the key sets and calls
`apply_diff`, whose only `view_fn(at, item)` call is inside
`for DiffOpAdd { at, mode } in add_cmds` (`keyed.rs:747-750`). Retained keys are
moved (`children[to] = moved_children[i]`), never re-rendered. The children
closure is therefore never called again with the new item.

**Nothing upstream rescues it.** `AnyView::rebuild` (`any_view.rs:386-400`)
reuses the existing state whenever `type_id` matches, so a closure node that
re-runs and returns the same concrete view type rebuilds INTO the `Keyed` state.
A `LocalResource` keeps its previous value while refetching
(`reactive_graph/.../arc_async_derived.rs:366-389` sets `loading` and only
replaces the value when the new future resolves), so there is no intermediate
`None` that would drop and rebuild the list.

**The consequence to look for in review:** `<For key=|x| x.id.clone() let:x>`
plus a child component taking a plain `x: T` prop. Whenever the source document
is re-read and the ids are stable (a FHIR version switch over the same code
systems), every row keeps the OLD body while the surrounding chrome updates.
Fixes, in order: pass `Signal<T>`/`ArcRwSignal<T>` rows, include the varying
datum in the key, or drop `<For>`.

**The counterpart:** `Vec<T>::rebuild` (`tachys/src/view/iterators.rs:176-199`)
is an unkeyed positional diff that calls `T::rebuild(new, old)` for every
retained position, so `.iter().map(|row| view!{…}.into_any()).collect::<Vec<_>>()`
DOES refresh its contents. For a list that is a whole-document replacement with
no per-row state and no reordering identity, the Vec is the correct shape and
`<For>` is the defect. `.into_any()` per item also makes divergent branch types
at one position safe, because a `type_id` mismatch replaces rather than rebuilds.

Related: [[directory-tree-editor]] (the same keying hazard from the state side).
