---
name: one-shot-async-in-a-resource-reruns-on-the-way-out
description: A one-shot side effect (an OAuth code exchange) must not live in a Resource, because a navigation away re-runs or re-reads it; use spawn_local once
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A `Resource` (or `LocalResource`) is a cache keyed on its source signals: it re-runs when a source changes and is read again by every consumer that mounts, including on the way out of a route when the router swaps views. An operation that must happen exactly once, such as exchanging an authorization code for a token (RFC 6749 §4.1.3: a code is single-use), does not belong there. In FerroTERM's `/ui/callback` (PR #647, `app/ferroterm-viewer/src/pages/callback.rs`) the exchange first sat in a resource and could be re-run by the navigation that followed success; it now runs once in `spawn_local` from the page's mount, writes the session signal, and navigates.

**Why:** a resource models a value derived from inputs; a one-shot effect modelled as a value re-derives when the inputs or the readers change.

**How to apply:** review any `Resource` whose fetcher has a side effect (POSTs, token exchanges, writes). Move the effect to `spawn_local` (or an `Action` when a retry button is wanted) and keep resources for reads. See [[for-retained-key-never-rebuilds]] for the sibling keyed-identity hazard.
