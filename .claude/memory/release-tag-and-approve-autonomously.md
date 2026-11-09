---
name: release-tag-and-approve-autonomously
description: The session cuts every release end to end, merges the release PR through the queue, signs and pushes the tag, and approves the crates-io deployment gate itself; nothing in a cut waits on the owner (owner 2026-09-13, 2026-10-27, 2026-10-28, 2026-11-03)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A release cut is run from the session: the version-bump PR, its merge through the merge queue (auto-merge armed like any PR), the signed tag on the merge commit, the push, the release run read and fixed, and every pending `crates-io` environment approval. Do not hand the owner a command for any of it.

**Why:** the owner, 2026-09-13: "you always run the tag never say that i need to do that"; 2026-10-27: "you approve the crates-io always okay"; 2026-10-28: "you merge it for me as well do not wait for me"; 2026-11-03: "you can approve crates.io and also tag releases fully autonomously, without me asking anything". Waiting on the owner stalled releases.

**How to apply:** after the release PR merges, check its merge commit (run the repository's preflight where it has one; FerroEHR burnt two release candidates on 2026-11-02 without it). Then `git tag -s vX.Y.Z -m vX.Y.Z <merge-sha>`, `git push origin vX.Y.Z`, and approve the gate with `gh api -X POST repos/<repo>/actions/runs/<run>/pending_deployments -F 'environment_ids[]=<id>' -f state=approved`. Never move or delete a pushed tag; a bad cut ships forward as the next version. A coordinating session that delegates a cut may keep the tag for itself: a worker then reports the merge commit instead of tagging. Docs and PR bodies never say "the owner runs" a step of the cut.
