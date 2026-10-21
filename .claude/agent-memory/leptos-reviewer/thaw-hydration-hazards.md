---
name: thaw-hydration-hazards
description: which thaw widgets mint random ids; the hydration consequence is moot under CSR, the verify-the-widget-source method is not
csr: changed
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified against the pinned thaw git checkout source
(rev `0726a3d6788f07e929996e77399d83655ffaacde`):

- **`thaw::Field`** does `StoredValue::new(Uuid::new_v4().to_string())`
  (`thaw/src/field/field.rs:26`) and wires its `<label for>` to that per-render
  UUID.
- **`thaw::Upload`** is clean: its `id` is
  `#[prop(optional, into)] MaybeProp<String>`, and with no id passed it renders
  no id attribute. There is no auto-generated UUID.

**What changed under CSR.** The original finding was a hydration hazard: the
server pass emitted uuid X, the client minted uuid Y, and the static label `for`
kept X while the reactive input `id` became Y. With no server pass there is no
mismatch, so `thaw::Field` is not banned here on hydration grounds.

**What survives, and is the reason to keep this entry.** Two things:

1. **The method.** Widget behaviour around `id` and `for` is not uniform across
   a component library. Read the widget's source before approving an `id`/`for`
   association; do not assume every thaw widget shares any one defect.
2. **The practice.** Pass an explicit stable `id` and `name` anyway.
   `Field::use_id_and_name` returns an explicit `id` prop before falling back to
   the uuid. A stable id is what makes an E2E selector reliable and what makes
   the label association readable to a reviewer, which is an accessibility
   requirement (`.claude/rules/leptos-ui.md` §9), not a hydration one.

See [[thaw-field-random-id]] for the original finding and
[[thaw-input-name-forwarding-ok]] for the `name` fact.
