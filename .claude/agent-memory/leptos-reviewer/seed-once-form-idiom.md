---
name: seed-once-form-idiom
description: the edit-form shape; the hydration reasoning is moot under CSR, the guard against a refetch overwriting in-progress input is not
csr: changed
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The accepted shape for a form seeded from a resource, with the parts that
survive marked.

**Still applies:**

- Form state is a `Copy` struct of `RwSignal`s created in the SETUP function,
  above the `<Transition>`, so a re-run of the async closure cannot re-create it.
- The seed function returns early while its recorded source version equals the
  loaded one. It is idempotent per loaded version, so a re-run for the same
  version never overwrites edits in progress. This is the guard that matters:
  without it a refetch discards what the reader is typing (see
  [[directory-tree-editor]], where a rejected write did exactly that).
- Only a SUCCESSFUL write refetches. An action's version increments on `Err`
  too, so a resource source keyed on the raw version refetches after a failure
  and throws away the input that failed.
- A controlled `<textarea>` is `prop:value` plus `on:input:target`, and a
  `<select>` is driven by `prop:value` on the select.

**Moot under CSR:** the reasoning about the child text expression being
evaluated at view construction so the server pass and hydration agree. There is
no server pass. Keep the `prop:value` idiom anyway, because the `value`
attribute only sets the initial value; that part is plain Leptos, not hydration.

**Standing UX caveat, unchanged:** between mount and the first seed the form is
empty and editable, and the seed then overwrites whatever was typed. The fix is
disabling the control until the seed has happened, not a wait helper in the E2E
suite.
