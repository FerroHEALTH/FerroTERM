---
name: auto-merge-follow-ups
description: "Never push a follow-up commit to a PR that has auto-merge armed unless it is still BLOCKED; PR #149 merged between two pushes on 2026-09-03 and a slice of work silently missed main"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1fab17b9-946d-49fc-af2e-4719ce97bc31
  modified: 2026-09-03T13:17:48.657Z
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

With `gh pr merge --auto --squash` armed, a PR merges the moment its checks pass. On 2026-09-03 the second refactor slice was pushed to PR #149 after CI had gone green on the first; the squash merged the first slice only, the branch kept the second, and the next branch built on the wrong base (a broken tree was even committed and pushed for a few minutes).

**Why:** auto-merge is a race with any further push; "one PR, one unit of work" is the safe shape.

**How to apply:** open a new PR for each further slice (branch from `origin/main`), or check `gh pr view N --json mergeStateStatus` is `BLOCKED` right before pushing more to an open PR. After a `git stash pop` or cherry-pick, look for `UU` in `git status` before committing; `set -e` does not stop a pipeline whose last command succeeds. See [[perl-edit-pitfalls]] for the same class of "verify before you commit" lesson.
