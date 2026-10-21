---
name: chartistry-chart-hydration
description: leptos-chartistry's Chart self-gates on a client measurement and uses deterministic ids; the hydration verdict is moot under CSR, the placeholder behaviour still shapes E2E waits
csr: moot
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified against the pinned `leptos-chartistry` 0.2.3 source:

- **`<Chart>` gates the SVG** behind
  `<Show when=have_dimensions fallback=|| <p>"Loading..."</p>>`, where
  `have_dimensions = watch.bounds.get().is_some()` and `bounds` comes from
  `use_watched_node`, a client-only `getBoundingClientRect`. The chart draws
  only after a post-mount effect measures the node. This holds for any
  `AspectRatio`, including a fixed outer ratio.
- **No random ids.** Series ids are a deterministic `next_id: usize` counter
  (`series/mod.rs`) assigned when the series is built in the component body. No
  `Uuid`, no `rand`.

**Moot half.** The original conclusion was that the widget is hydration-safe,
because the server render and the client's first render both emit the same
`Loading...` placeholder. Under CSR there is no server render, so the question
does not arise, and a chartistry chart needs no `<Suspense>` wrapper of its own
for that reason.

**The residue, which is what to keep.** The chart renders a placeholder until
its container has been measured, which happens after mount. So:

- An E2E journey waits for the drawn SVG, not for the chart's container. A
  journey that asserts right after navigation catches the placeholder.
- A chart inside a hidden container is never measured and never draws. On a
  tabbed screen, a chart in a `class:hidden` body stays a placeholder until the
  tab is shown, which is another reason to mount tab bodies lazily
  ([[tabbed-screen-pattern]]).

Do not flag a chartistry chart as a rendering hazard on the basis of client
measurement alone.
