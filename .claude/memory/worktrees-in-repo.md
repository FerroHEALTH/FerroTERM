---
name: worktrees-in-repo
description: Every git worktree in a family repository lives at <repo>/.claude/worktrees/<name>, never as a sibling folder and never in the session scratchpad (owner 2026-10-21, repeated 2026-10-27 and 2026-11-01)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every git worktree, in every FerroHEALTH repository, lives inside that repository at `<repo>/.claude/worktrees/<name>`, which `.gitignore` excludes. A worktree never goes in a sibling folder such as `RustroverProjects/FerroTERM-wt-778`, and never in the session scratchpad, which holds notes, PR bodies and logs only.

**Why:** on 2026-10-21 the owner found worker worktrees scattered beside the repositories and called it custom logic they did not want. It happened again on 2026-10-27 and on 2026-11-01, the last time because a brief mixed up the scratch-files rule with the worktree rule; the owner was angry.

**How to apply:** create a worktree with `git worktree add .claude/worktrees/<name> -b <type>/<slug> origin/main` from the repository root, and write that path out in every worker brief, beside "scratch files only under `<scratchpad>/<task>/`". Move a stray one with `git worktree move`. Never commit from a main checkout. Remove the worktree after the merge ([[clean-up-agent-worktrees]]).
