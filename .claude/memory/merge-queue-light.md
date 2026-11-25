---
name: merge-queue-light
description: The merge-group run is light (the guards, rustfmt, cargo-deny, the crate-version guard and a unit-test smoke set); the full suite runs on the pull request and on every push to main (owner 2026-11-25)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner's ruling for the whole family (2026-11-25): the merge queue must be
fast. The pull request already ran the full suite, so the merge_group run
does not run it again. Queues of about ten entries dropped pull requests at
the 60-minute check timeout when every group re-ran the full tests, e2e,
perf and coverage.

In merge_group, `ci.yml` runs the tier-1 guards, rustfmt, cargo-deny, the
crate-version guard and a smoke set: the build in one profile and the unit
tests (`--lib`, or `--bins` for a binary-only package). Every other lane
(clippy, the full test matrix, doctests, rustdoc, msrv, coverage, e2e
containers, browser journeys, perf and CPU gates, conformance corpora,
fuzz, publish dry runs) carries `github.event_name != 'merge_group'` in its
job condition, so it starts no runner. The required check keeps its name and
reads a skipped lane as a pass, so the rulesets need no edit.

**How to apply:** a new lane that compiles beyond the smoke build, measures,
or starts a container or a browser runs on the pull request and on main and
is skipped in merge_group at job level. A cheap guard that reads the merged
tree (the ratchets, the changelog and crate-version guards) runs in
merge_group too. Never change a threshold or a corpus to save time.
