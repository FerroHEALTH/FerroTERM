---
name: leptos-router-form-interception
description: leptos_router never intercepts native form submits (click-on-anchor and popstate only), so a plain <form> is the right tool; <Form method="GET"> to the SAME path is a trap
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified in leptos_router 0.8 source:

- The Router installs exactly TWO global listeners:
  `window_event_listener(ev::click, …)` and `ev::popstate`
  (`src/location/history.rs:161` and `:200`). There is **no window or document
  `submit` listener anywhere in the crate.**
- The click handler acts only when an `HtmlAnchorElement` appears in
  `ev.composed_path()` (`src/location/mod.rs:321-329`), and bails on `target`,
  `download`, `rel=external`, and cross-origin. A click on
  `<button type="submit">` is therefore ignored.
- Submit interception exists ONLY as `.on(ev::submit, on_submit)` that the
  `Form` component attaches to its own element (`src/form.rs:309`).

So a plain `<form method="GET" action="/ui/x">` plus a Rust `on:submit` that
calls `ev.prevent_default()` and navigates is safe and correct.

**Trap:** `leptos_router::components::Form` with `method="GET"` pointed at the
SAME route path does a client-side navigation, and `NestedRoutesView::rebuild`
short-circuits: "if the path is the same, we do not need to re-route, we can
just update the search query" (`src/nested_router.rs:152-162`). The route
component does NOT re-run, so anything decided from the query at setup (an
untracked read) silently no-ops. Same-path query changes need a reactive read,
a `Memo` or `Signal` over `use_query_map`.

**Why this bites in FerroTERM:** the expansion runner at `/ui/expand` and every
search box are GET forms pointed at their own path, so their parameters must be
read reactively.

Path params ARE percent-decoded on read (`ParamsMap::insert` calls
`Url::unescape`, `src/params.rs:29`), so encoding a value into a link
round-trips.

Related: [[redirect-path-must-be-percent-encoded]]
