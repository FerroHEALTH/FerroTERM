---
name: show-children-rerun-is-cleaned-up
description: <Show> children is a ChildrenFn called on every flip, and the enclosing RenderEffect calls owner.with_cleanup() first, so NodeRefs and StoredValues rebuilt inside it are disposed, not leaked
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in the pinned sources (leptos 0.8.20, reactive_graph 0.2.14).

**The children closure really does re-run.** `leptos/src/show.rs` takes
`children: TypedChildrenFn<C>`, memoizes `when` into an `ArcMemo`, and returns
`move || match memoized_when.get() { true => Either::Left(children()), … }`. So
`<Show when=… >{build_a_form(…)}</Show>` calls `build_a_form` again on every
flip to true, and only on a real boolean change (the `ArcMemo` is the gate even
when `when` is an ungated `Signal::derive`).

**Nothing leaks.** `ArenaItem::new_with_storage`
(`reactive_graph/src/owner/arena_item.rs:47-63`) registers every arena item
(`NodeRef`, `StoredValue`, `Signal::stored`) with the CURRENT owner, and
`RenderEffect`'s re-run is `owner.with_cleanup(|| … fun(old_value) …)`
(`render_effect.rs:236`). `Owner::with_cleanup` is `self.cleanup(); self.with(fun)`
and `cleanup` is documented to drop "the values of any arena-allocated
`ArenaItem`s" (`owner.rs:294-305`). The previous flip's nodes are gone before
the new ones are made.

**`use_navigate()` inside such a closure is legitimate**: `leptos_router
0.8.15 src/hooks.rs:275-279` is a `use_context::<RouterContext>()` plus a
returned closure. It creates no reactive node, and the context resolves through
the owner chain, which is under the `<Router>`.

**What still IS a finding in that position:** creating a `LocalResource`,
`Effect`, or interval inside the children closure, because those re-subscribe
and re-fetch on every flip. Build the resource in the section body and pass it
in (`.claude/rules/leptos-ui.md` §6).

Related: [[for-retained-key-never-rebuilds]], [[tabbed-screen-pattern]]
