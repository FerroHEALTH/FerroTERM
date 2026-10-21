---
name: navigate-resolve-false-under-base
description: under <Router base="/ui">, use_navigate with the default NavigateOptions prepends the base a second time, so an absolute in-app address must pass resolve = false
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in `leptos_router` 0.8.15.

- `RouterContext::navigate` (`src/components.rs:134-145`) branches on
  `options.resolve`: true calls
  `resolve_path(self.base, path, Some(current.path()))`, false calls
  `resolve_path("", path, None)`.
- `resolve_path` (`src/matching/resolve_path.rs:3-29`): when `path` starts with
  `/`, the result prefix is the BASE, and the normalized path is appended to
  it. So with `base = "/ui"` and `path = "/ui/expand?…"` the default options
  produce `/ui/ui/expand?…`, which the router answers with its 404 fallback.
  With `resolve: false` the prefix is `/` and the address is passed through
  unchanged.

**Review rule:** any `use_navigate()` call in this viewer that passes an
address already carrying `/ui` MUST pass
`NavigateOptions { resolve: false, ..Default::default() }`. A relative address
(`"expand?…"`) is the other correct shape and keeps the default options. Do not
flag `resolve: false` as unusual; flag its absence.

`has_scheme` also treats any path containing `://` as external, which is
another reason every interpolated value is percent-encoded before it reaches an
address ([[redirect-path-must-be-percent-encoded]]).

Related: [[use-location-pathname-includes-base]], [[router-same-route-param-nav]]
