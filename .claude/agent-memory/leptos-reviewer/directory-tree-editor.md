---
name: directory-tree-editor
description: a tree whose <For> keys are positional paths keeps content correct but bleeds collapse and rename state to the sibling that shifts into position, and a refetch re-seeds the editor mid-edit
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Reviewed live in the sibling project's folder-tree editor. The design was one
working-tree signal with no per-row local state:

- `<For>` over children keyed by the **positional path** (`"0/1"`), because the
  nodes carried no stable id.
- Every rendered datum read the tree reactively by that path, and every mutation
  went through pure, unit-tested helpers.
- Because display AND the captured mutation index were both positional, content
  and delete targeting stayed correct on reorder and delete. That part was fine.
- BUT collapse state and in-progress rename were keyed by the same positional
  path, so on delete or reorder that UI state **bled to the sibling that shifted
  into the position**: delete a folder and a sibling appears collapsed. This is
  the `<For>`-index symptom (`.claude/rules/leptos-ui.md` §4), bounded to
  ancillary view state, and it is a should-fix.

Second defect, structural: the editor was created inside the main `Suspense`'s
`Suspend` closure, and the resource source included a write-version signal. So
**every write refetched, the closure re-ran, and the editor re-seeded from the
server body**. The action version increments on `Err` too, and the resource
value updated with no equality guard, so a rejected write DISCARDED the reader's
unsaved edits while the message said "reload and try again". Every successful
save also reset collapse and rename state. The main content used `<Suspense>`
rather than `<Transition>`, so each write flashed the skeleton.

**How to apply in FerroTERM:** the concept browser's taxonomy tree is the same
shape. Key `<For>` by concept id, which exists and is stable, and key collapse
and selection by that same id, never by position. Keep the tree model outside
any `Suspend` closure, and read a refetching resource under `<Transition>`. See
[[seed-once-form-idiom]] for the re-seed guard and [[builder-signal-struct-ver]]
for the focus-preserving edit shape.
