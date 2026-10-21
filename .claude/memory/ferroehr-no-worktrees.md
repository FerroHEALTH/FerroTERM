---
name: ferroehr-no-worktrees
description: "An agent sent into the FerroEHR clone follows that repository's owner rule of one checkout and no git worktrees"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 72434f36-450a-494f-91d0-0892d1f31a75
  modified: 2026-09-23T06:49:51.919Z
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The FerroEHR repository (`/Users/rubentalstra/RustroverProjects/FerroEHR`) carries an owner directive of 2026-08-06 in its own memory (`no-worktrees-single-checkout.md`): every change happens in the main checkout on the current branch, never in a `git worktree`, because the owner follows the work in one working directory. On 2026-09-23 a brief from this session told an agent to use a worktree there; the agent complied and flagged the conflict.

**Why:** each repository's owner rules outrank a brief written from the other repository.

**How to apply:** a brief for FerroEHR (or FerroBRIDGE) tells the agent to read that repository's CLAUDE.md and memory first and to follow them where they differ from FerroTERM's habits; for FerroEHR that means the main checkout, a branch, no worktree. See [[clean-up-agent-worktrees]] for the FerroTERM side.
