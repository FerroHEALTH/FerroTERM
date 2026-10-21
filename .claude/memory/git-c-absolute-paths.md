---
name: git-c-absolute-paths
description: In this harness a `cd` inside a compound Bash command does not reliably set the working directory for git; use `git -C <abs path>` and absolute paths for every worktree operation
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A `cd <worktree> && git checkout -B <branch>` ran in a different worktree than
the one named, and the follow-up force push overwrote a sibling PR's remote
branch with another PR's content (2026-09-24, PRs #674 and #676).

**Why:** the Bash tool's working directory persists and is reported one call
late, so a compound command's `cd` and the harness's notion of cwd disagree.

**How to apply:** every git call on a worktree uses `git -C /abs/path ...`,
file edits use absolute paths, and a force push is preceded by
`git -C <path> rev-parse --abbrev-ref HEAD` in the same command. Prefer
`gh pr update-branch <n>` (a merge, no force) to bring a PR up to date. See
[[auto-merge-follow-ups]] and [[clean-up-agent-worktrees]].
