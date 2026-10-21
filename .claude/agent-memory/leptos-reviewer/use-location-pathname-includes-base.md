---
name: use-location-pathname-includes-base
description: use_location().pathname is the raw browser path INCLUDING the router base (/ui), so an aria-current or link comparison must compare against base + path
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in leptos_router 0.8.15.

`Location::pathname` is `Memo::new(move |_| url.with(|url| url.path.clone()))`
(`src/location/mod.rs:195`), and the browser `Url` is built by
`BrowserUrl::current()` as `path: location.pathname()?`
(`src/location/history.rs:82`), the unmodified `window.location.pathname`. The
`base` prop is never stripped from it: base is only prepended to the route
definitions (`RouteDefs::new_with_base`, `src/components.rs:247`) and passed to
the anchor click handler.

**Review rule with `<Router base="/ui">`:** a "am I on this page" test compares
`location.pathname.get()` to `format!("{UI_BASE}{path}")`, not to the route path
alone, and trims a trailing `/` on both sides because the index route can be
reached as `/ui` or `/ui/`. A `version_link(&location.pathname.get(), …)` style
rewrite is correct as is, because the full path is what an `href` needs.

Related: [[router-same-route-param-nav]] (read it reactively, never
`get_untracked` at setup).
