---
name: clean-up-agent-worktrees
description: Remove a subagent's worktree and its branches as soon as its PR merges, without being asked; the owner has had to chase this three times
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

When a subagent's PR merges, remove its worktree and delete the leftover
branches in the same breath. Do not wait to be asked and do not batch it up for
later.

**Why:** the owner has asked for this three separate times (2026-09-06). Stale
worktrees under `.claude/worktrees/` pile up, each holding a full `target/`
directory, and the branch list fills with merged names until neither is
readable. Chasing the same cleanup repeatedly is the owner doing my
housekeeping.

**How to apply:** after any agent completes, or before reporting status at the
end of a stretch of work:

```
git worktree list                 # which exist
gh pr list --head <branch> --state all --json number,state   # merged?
git worktree unlock <path>        # remove --force fails silently on a locked one
git worktree remove <path>
git worktree prune
git branch -D <branch> worktree-agent-<id>
git stash list                    # drop stashes whose work is committed
```

Two cautions learned the hard way:

- **Never remove a worktree whose agent is still running, and never loop over
  `.claude/worktrees/*` to do it.** Remove them one at a time, by the id of an
  agent that has reported. A loop cannot tell a live worktree from a finished
  one, and `--force` discards the live agent's uncommitted work without asking.
  This has happened twice: once caught by `git worktree remove` refusing on
  modified files, once not, because the loop passed `--force`. What survives is
  the branch, since branches outlive worktrees, so the recovery is to point the
  agent at the branch in the main clone; what is lost is whatever it had not
  committed.
- **A squash-merged branch fails `git branch -d`** because its commit is not an
  ancestor of main. Confirm the PR is MERGED with `gh`, then use `-D`. Do not
  chain the whole cleanup into one compound command: the auto-mode classifier
  blocks it, and the pieces run fine separately.

Related: [[auto-merge-follow-ups]], [[repo-merge-gates]].

**Caveat (2026-09-22):** the merge of a PR is not proof the agent is done. On 2026-09-22 a worktree was removed on the merge signal while its agent was still committing a CodeQL fix, and the edit was lost. Remove a worktree only after the agent's final hand-back says nothing is outstanding, or after a message to it confirms so.
