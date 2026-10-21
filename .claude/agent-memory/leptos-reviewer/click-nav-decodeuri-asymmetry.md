---
name: click-nav-decodeuri-asymmetry
description: a click navigation runs the path (not the query) through decodeURI before pushing it, so an encoded path segment reaches the params one decode-pass lighter than the same address opened fresh; only a literal % is actually lossy
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in leptos_router 0.8.15.

**Two different paths into one param.**

- **Fresh load / refresh / `goto`:** `BrowserUrl::current()` reads
  `window.location.pathname` raw, `ParamSegment::test`
  (`src/matching/horizontal/param_segments.rs:53-63`) stops at a LITERAL `/`
  and captures the still-encoded slice, and `ParamsMap::insert`
  (`src/params.rs:29`) decodes it once with `Url::unescape` =
  `js_sys::decode_uri_component`. One decode pass.
- **Anchor click:** the window handler computes
  `path_name = Url::unescape_minimal(&url.path)` (`src/location/mod.rs:351`),
  which under CSR is `js_sys::decode_uri`, and pushes `path_name + "?" +
  url.search`. The params then decode that a second time. Two passes on the
  PATH. `url.search` is never touched, so a query parameter takes one pass on
  both routes.

**What survives and what does not.** `decodeURI` keeps the reserved set
`; / ? : @ & = + $ , #` escaped (ECMA-262 `Decode`, `reservedURISet`), so
`%2F`, `%3F`, `%23`, `%3D`, `%26` all survive the extra pass and a code system
canonical in a path segment round-trips. `%25` does not: `decodeURI("%25")` is
`"%"`, so a canonical carrying a literal `%` decodes one level too far on a
click and disagrees with the same address opened fresh.

**Review rule.** A value with structural characters is safest in the QUERY,
which `unescape_minimal` never sees (`.claude/rules/leptos-ui.md` §7, every
value interpolated into a URL is percent-encoded). If it must be a path
segment, say so with a `// NOTE:` and pin the limit with a test. A unit test
that hand-models "split on `/`, then percent-decode" only models the
fresh-load half, so it cannot see this at all: the click half needs an E2E
journey, and the fresh-load half needs a `goto` on the encoded address.

Related: [[redirect-path-must-be-percent-encoded]],
[[router-same-route-param-nav]], [[internal-nav-uses-plain-anchor]]
