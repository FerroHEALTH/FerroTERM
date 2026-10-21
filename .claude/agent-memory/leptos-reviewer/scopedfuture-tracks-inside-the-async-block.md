---
name: scopedfuture-tracks-inside-the-async-block
description: Signal reads inside a Resource's async block are tracked (ScopedFuture), so reading a signal after an await re-subscribes the resource to it
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Leptos wraps a resource's fetcher in a `ScopedFuture`, so a signal read anywhere inside the async block, including after an `.await`, subscribes the resource to that signal. A resource that reads, say, the served-version signal after fetching then re-fetches whenever that signal changes, which looks like a spurious re-run. Seen while reviewing #647's discovery read in the shell (`app/ferroterm-viewer/src/components/shell.rs`), where the combined discovery future read session state after its awaits.

**Why:** tracking in Leptos is dynamic and follows the reads, not the source list a reader expects.

**How to apply:** read every input a fetcher needs before the first `.await` (or take them as the resource's explicit source), and read post-await state through `get_untracked()` when it must not re-subscribe. Point at this note when a resource re-runs for a reason its source does not explain. See [[one-shot-async-in-a-resource-reruns-on-the-way-out]].
