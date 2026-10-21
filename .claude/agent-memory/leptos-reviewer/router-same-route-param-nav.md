---
name: router-same-route-param-nav
description: leptos_router only updates the params signal when a navigation matches the SAME <Route>; the component body does not re-run, so a param read untracked at setup goes stale
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in leptos_router 0.8 source.

**Declaration order decides the match.** `src/matching/nested/tuples.rs:299`
(`impl MatchNestedRoutes for ($($ty,)*)`) is
`$(if let (Some(..), remaining) = $ty.match_nested(path) { return … })*`: the
first tuple element that matches wins. So declaring a literal route before a
`:param` route really does keep the literal from being read as a param. Route
ids stay unique above 16 children: `NestedRoute.id` comes from a global counter
(`src/matching/nested/mod.rs:92`, `ROUTE_ID.fetch_add`), and the `Either`'s
`as_id()` delegates to the inner match, so nesting never collides ids.

**A same-route navigation does NOT re-run the component body.**
`src/nested_router.rs:849-851`: "a unique ID for each route … if two IDs are the
same, we do not rerender, but only update the params", and the same-id branch is
only `current.matched.set(…); current.params.set(…); current.url.set(…)`. Two
paths matching one `<Route>` share an id, so the params update and the view is
kept.

**Review rule:** a path param may be read with `with_untracked`/`get_untracked`
at setup ONLY if no reachable in-app anchor or `navigate()` reaches another
value of it under the same `<Route>`. Because a plain `<a>` is intercepted too
([[internal-nav-uses-plain-anchor]]), a switcher built from anchors is exactly
such a navigation.

**Why this bites in FerroTERM:** the FHIR version switcher (`/r4`, `/r4b`,
`/r5`, `/r6`) and the code system switcher on `/ui/systems/:url` are switchers
built from anchors under one route. Make every path param a `Signal::derive`
and every query param a reactive read.
