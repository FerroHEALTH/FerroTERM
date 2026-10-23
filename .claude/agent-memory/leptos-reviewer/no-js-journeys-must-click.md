---
name: no-js-journeys-must-click
description: the no-JavaScript journey is gone under CSR, but the review rule survives; an E2E assertion on page source matches inert markup and proves nothing
csr: changed
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

**The original finding.** A set of end-to-end journeys asserted on
`driver.source()` substrings instead of interacting with the page, justified by
a comment claiming the shell arrived as inert `<template>` fragments without
JavaScript. That justification was stale: the routes in question had been moved
to a rendering mode that sends one complete document, which emits no
`<template>` placeholders.

**Moot half.** This viewer is client-side rendered, so there is no
no-JavaScript journey to write. Without JavaScript the page is an empty
`<body>`, which the Leptos book states plainly for CSR
(`src/csr_wrapping_up.md`). Do not file a finding asking for progressive
enhancement here; `docs/console.md` §1 records the trade deliberately.

**The half that survives, and it is the important one.** A
`source.contains("name=\"filter\"")`-style assertion matches inert markup too,
so it passes even when the control is unreachable, disabled, or its handler is
broken. It cannot prove the journey. Require every journey to drive the real
widget: wait for the element, type into it, click it, and then wait for the URL
or the DOM to change. A direct `goto("/ui/x?param=…")` proves only the
shareable-URL case, never the form.

**Corollary for CSR specifically:** because the document is empty until the
bundle boots, every journey needs an explicit wait on a rendered element before
it asserts anything. A journey that asserts immediately after `goto` is testing
the loading state.

Related: [[leptos-router-form-interception]], [[chartistry-chart-hydration]]
