---
name: milestone-autonomy
description: "Owner directive, restated 2026-09-04 for v0.0.11: work a milestone's issues end to end without pausing, cut the release the moment it empties, move stragglers forward, file new issues for new work; the session also tags and approves crates.io (owner 2026-11-03)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Work the current milestone to zero open issues without stopping to ask what to do next, then cut the release: bump the version everywhere, cut the changelog, open the release PR. The owner said it first for v0.0.4 through v0.0.7 and restated it on 2026-09-04 for v0.0.11: "keep going till there is nothing left in the milestone like the last one so we can cut a new release".

**Why:** deciding the order of issues inside a milestone is not something they want to be asked about. Pick by priority and blockers ([[repo-merge-gates]], the issue-workflow rules), state the order once, then execute.

**How to apply:** one PR per issue with auto-merge armed, the crate line bumped per PR, and each issue ticked and commented before moving on. New work found en route becomes its own issue in the same or the next milestone, never a silent deferral. Cut the release when the milestone empties and carry it through: since 2026-10-27 and 2026-11-03 the session merges the release PR through the queue, signs and pushes the tag, and approves the crates.io gate itself ([[release-tag-and-approve-autonomously]]). See [[release-cut-cadence]], [[gates-in-container]].
