---
name: polled-resource-needs-transition
description: a resource refetched on an interval or a filter change must be read under <Transition>, not <Suspense>, or it flashes the fallback on every reload
csr: still-applies
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A resource that is refetched, whether by an interval poll or by a change in its
source, reverts to pending on every refetch. Read under `<Suspense>`, the
section then flashes its loading fallback each time.

**Rule:** `.claude/rules/leptos-ui.md` §6, from the book chapter
`async/12_transition`: reloading data uses `<Transition>` to keep the old data
visible instead of flashing the fallback.

**How to apply:** whenever a resource is periodically refetched, or reloaded on
a filter, search, page, or parameter change, its read site is `<Transition>`.
In this viewer that is nearly every data section: the health pill polls, and
every listing, search box, and paged table refetches on a URL change. Treat a
`<Suspense>` over a refetching resource as a finding.

**Why it still applies under CSR:** the fallback flash is a behaviour of the
resource and its suspense boundary. It has nothing to do with server-side
rendering.
