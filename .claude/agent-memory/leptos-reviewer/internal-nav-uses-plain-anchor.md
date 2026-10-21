---
name: internal-nav-uses-plain-anchor
description: a plain <a href> IS a client-side navigation, because leptos_router intercepts every same-origin anchor through a window-level click handler
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand against leptos_router 0.8 source:

- `src/location/history.rs:161`: `BrowserUrl::init` installs
  `window_event_listener(ev::click, …)` unconditionally.
- `src/location/mod.rs:298` `handle_anchor_click` walks `ev.composed_path()`
  for any `HtmlAnchorElement`, calls `ev.prevent_default()`, and performs a
  History API navigation. It bails out only for a modified click (button other
  than 0, or meta, alt, ctrl, shift), a non-empty `target`, `download`,
  `rel="external"`, a cross-origin href, or a path outside the router base.

So a plain `<a href="/ui/x">` is a client-side navigation exactly like `<A>`;
the only thing `<A>` adds is `aria-current` and active-class handling. Do not
flag a plain internal anchor as a full-page reload, and do treat every internal
anchor as a client-side navigation when reasoning about remount semantics. See
[[router-same-route-param-nav]], which is what makes that load-bearing.

**The FerroTERM corollary:** `rel="external"` is required on any anchor that
opens a raw FHIR request (`/r4b/CodeSystem/$lookup?…`), or the router
intercepts the click and answers its own 404 for a path it does not own. The
request-disclosure affordance on every screen is exactly such a link.
