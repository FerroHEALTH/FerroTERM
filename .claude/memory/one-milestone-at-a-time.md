---
name: one-milestone-at-a-time
description: Every worker works the current release milestone until it is cut; an issue filed en route goes in the current milestone and is fixed before the cut; a straggler blocked outside the repository moves to the next (owner 2026-10-19)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

All workers work the current release milestone until it is empty and cut; only then does work on the next milestone start. An issue filed while working (an en-route finding, a follow-up, a guard gap) goes in the current milestone and is fixed before the next unit starts: the fix-first cadence.

**Why:** the owner, 2026-10-19: "you need to go per milestone right? so we can cut the release and then do all our effort on that new release". A release ships sooner when every worker pulls toward it.

**How to apply:** assign workers only issues of the open release milestone. An issue that waits on another repository (an upstream crate release, a sibling's change) moves to the next milestone with a comment saying what it waits on, so it never holds the cut ([[next-milestone-always]]). An `upstream-report` carries no milestone. Cut the release the moment the milestone is empty.
