---
name: ferroehr-no-worktrees
description: "Superseded: FerroEHR allows git worktrees since 2026-10-19, only at .claude/worktrees/<name>, each with its own target directory; a brief from here follows that repository's own rules"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroEHR banned worktrees on 2026-08-06 so the owner could follow the work in one directory, and a brief from this session that sent an agent into a worktree there broke that rule on 2026-09-23. The owner lifted the ban on 2026-10-19 (FerroEHR `worktrees-for-parallel-branches.md`), and the family rule of 2026-10-21 puts every worktree at `<repo>/.claude/worktrees/<name>` ([[worktrees-in-repo]]). Since 2026-11-09 each FerroEHR worktree builds into its own target directory, because a shared target lets one worktree overwrite another's workspace-member outputs.

**Why:** each repository's owner rules outrank a brief written from another repository.

**How to apply:** a brief for FerroEHR, FerroBRIDGE or any sibling tells the agent to read that repository's CLAUDE.md and memory first and to follow them where they differ from FerroTERM's habits. See [[clean-up-agent-worktrees]] for the FerroTERM side.
