---
name: redirect-needs-ssrmode-async
description: a server-side <Redirect> lands only because the route runs in SsrMode::Async; SsrMode does not exist under CSR, so the entry is kept for the record only
csr: moot
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

**Kept for the record.** Nothing in this entry applies to a client-side
rendered viewer: there is no `SsrMode`, no `leptos_axum`, and no server-side
redirect. It is preserved because the mechanism is worth knowing if this
project ever revisits the rendering decision (`docs/viewer.md` §1, "What would
change the decision").

`leptos_router::components::Redirect` is `#[component(transparent)]`: its body
runs at view-construction time, and on the server it calls the
`ServerRedirectFunction`, which `leptos_axum` provides as
`leptos_axum::redirect`. Two conditions make it work:

1. **`Accept: text/html`.** `redirect()` inserts `Location` always, but sets
   `302 Found` only when the request accepts `text/html`; otherwise it adds a
   custom redirect header, so a scripted client can still read the payload.
2. **Rendering mode.** Status and headers are applied only after the FIRST
   chunk of the stream. Under `SsrMode::Async` the first chunk is the entire
   document, so a redirect decided anywhere in the tree still lands. Under the
   default out-of-order streaming the head is flushed first and the redirect is
   lost. A `<Redirect>` decided from a route body is therefore coupled to that
   route's `ssr=SsrMode::Async`, and the coupling belongs in a written comment.

**Under CSR the client-side equivalent is `use_navigate()`**, which has none of
this behaviour and no header timing to reason about.

Related: [[redirect-path-must-be-percent-encoded]]
