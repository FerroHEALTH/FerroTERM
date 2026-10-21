---
name: selected-version-derive-is-gated
description: the viewer's SelectedVersion is a Signal::derive over use_query::<ShellQuery>(), which is a Memo with PartialEq, so it notifies only on a real ?fhir= change and is safe as a resource source
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

`components/shell.rs` builds `SelectedVersion(Signal::derive(move || query
.read()… ))` over `use_query::<ShellQuery>()`. `use_query` returns a
`Memo<Result<T, ParamsError>>` and `ShellQuery` derives `PartialEq`, so the
memo swallows every query change that leaves `?fhir=` alone. The ungated
`Signal::derive` on top of it therefore notifies only when the version really
changes.

**Why this matters for review.** [[resource-read-registers-suspense]] says an
ungated `Signal::derive` in a resource source refetches on a notify rather than
on a value change. That rule does NOT fire on `SelectedVersion`: a resource
source spelled `move || { let version = version.get(); let filter =
filter.get(); async move { … } }` refetches only when the version or the
(memoized) filter changes, so paging, opening a row, or editing a second form
on the same screen issues no new request. Do not file that finding against
`SelectedVersion`; do file it against a source that reads a settings signal or
any other bare `Signal::derive` with no `PartialEq` gate behind it
(`.claude/rules/leptos-ui.md` §6).

The same gate is why the other half of a screen's URL state belongs in its own
`Memo` over `use_query_map()`: `ParamsMap` changes on every navigation, so the
`Memo<T>`'s `PartialEq` is what keeps a resource still.
