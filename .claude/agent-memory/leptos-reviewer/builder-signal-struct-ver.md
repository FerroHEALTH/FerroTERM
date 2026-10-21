---
name: builder-signal-struct-ver
description: the single-RwSignal plus struct_ver gating pattern for a deeply nested editable tree that must not destroy input focus on edit
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The validated shape for a deeply nested tree or form that must NOT destroy
input focus while it is edited:

- **One `RwSignal<Model>`** holds all editable state; a separate
  `struct_ver: RwSignal<u32>` is bumped ONLY on structural edits (add, remove,
  regroup, toggle, reshape). The render closures read `struct_ver.get()`
  (tracked) then the model `get_untracked()`, so a text keystroke (a
  `model.update` with NO bump) does not re-render the tree and the `<input>`
  keeps focus.
- The live preview is a `Memo` reading the model TRACKED, so it updates on every
  keystroke while the editor DOM is untouched.
- Leaf editors seed fresh local `RwSignal`s from the snapshot on each structural
  re-render and write the rebuilt value back through `model.update(…)`. No
  `Effect` anywhere: every write is an `on:input`/`on:click` listener, so there
  are zero signal-writes-signal effects (`.claude/rules/leptos-ui.md` §2).
- Tree mutation is pure `Vec<usize>`-path helpers, all unit-tested outside
  components (`.claude/rules/leptos-ui.md` §10). Radio `name`s and input `id`s
  come from a deterministic path key, never a random id.

**Why:** the standard "each row is an `ArcRwSignal` in a `<For>`" pattern does
not fit an n-ary tree; this design threads one `Copy` context bundle instead and
gates re-render explicitly.

**How to apply:** when reviewing a similar tree editor, confirm the bump signal
is bumped ONLY on a structural change, the render closures read the model
untracked, and the tracked subscribers are just the preview text nodes. That
combination is what preserves focus. There is no stale-index risk, because every
mutation bumps and rebuilds with fresh paths.

**Where it lands in FerroTERM:** the taxonomy tree in the concept browser and
any multi-clause expansion form.
