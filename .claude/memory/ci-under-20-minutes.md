---
name: ci-under-20-minutes
description: "Owner 2026-11-09: every pull-request and merge-group run of ci.yml under 20 minutes, queue included; the merge queue proves the merged tree, so the performance gate and other long non-functional gates run on the pull request and on main, never in merge_group"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The family target (owner, 2026-11-09): a pull-request run and a merge-group
run of `ci.yml` each finish in under 20 minutes, from the run's creation to
its conclusion, runner queue included.

The merge queue proves the merged tree, never the performance. A long
non-functional gate (the A/B performance gate, fuzz, the differential lane,
a long corpus sweep) runs on the pull request and on every push to `main`.
In a merge group its job keeps its name and reports success without
running, with a summary that says why (#1186). Functional gates (build,
clippy, tests, guards, pass lists) still run in the merge group.

**Why:** hosted runners are shared across the organisation, so every job a
run starts waits for a runner, and the queue tests one group after another.

**How to apply:** no gate is weakened; only the event it runs on changes.
Fewer, fatter jobs beat many tiny ones. A new long non-functional job takes
the merge-group skip in the change that adds it.
