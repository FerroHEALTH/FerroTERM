---
name: thaw-field-random-id
description: thaw::Field mints a Uuid::new_v4() id at setup; a hydration hazard in an SSR project, and kept here for the record because the practice it recommends is still right
csr: moot
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

`thaw::Field` generates its label and input id with `Uuid::new_v4()` at
component setup (pinned thaw rev `0726a3d`, `thaw/src/field/field.rs:26`). It
renders that uuid onto the label as a **static** `attr:for` and provides it,
through `FieldInjection`, to the child `thaw::Input`, which renders it as a
**reactive** `id=Signal`.

**In an SSR project that is a defect:** the server emits uuid X and hydration
mints uuid Y, the static label `for` keeps X while the reactive input `id`
becomes Y, the label and input association breaks after hydration, and it can
trip a browser-console attribute-mismatch warning.

**Under CSR it cannot happen.** There is one render, in one process, so the id
is whatever was minted and the label points at it. `thaw::Field` is not banned
here on those grounds.

**Kept because the recommendation stands anyway.** Pass an explicit stable `id`
and `name` to every field. A stable id is what an E2E selector needs and what
makes the label association auditable, which `.claude/rules/leptos-ui.md` §9
requires for accessibility. A random id per mount gives neither.

**Correction, verified in the pinned rev while building the settings screen
(#402): an explicit `id` inside a `thaw::Field` BREAKS the label association.**
`Field` renders its label as `attr:r#for=id.get_value()` with the uuid it
minted (`thaw/src/field/field.rs:26`, and the `<Label>` in its view), and it
never consults an id prop, because `Field` has none. The child then calls
`FieldInjection::use_id_and_name` (`field.rs:175-190`), which returns the
child's own `id` prop FIRST and only falls back to the injected uuid. So
passing `id="viewer-page-size"` to a `thaw::Input` inside a `thaw::Field` makes
the label point at the uuid and the input carry the explicit id: two different
values, no association, a WCAG failure rather than a hydration one.

**The shape that works:** a plain `<label for="viewer-page-size">` beside the
control, with the same explicit `id` on the control, and no `thaw::Field`
wrapper. Use `thaw::Field` only where you accept its uuid for both halves.

See [[thaw-hydration-hazards]] for the widget survey and
[[thaw-input-name-forwarding-ok]] for the `name` fact.
