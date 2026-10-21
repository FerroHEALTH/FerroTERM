---
name: pr-body-licence-checkbox
description: Every PR body must use .github/PULL_REQUEST_TEMPLATE.md with the licensing box ticked, or the contribution-licence-guard check fails
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every pull request body follows `.github/PULL_REQUEST_TEMPLATE.md` verbatim, with the line `- [x] I accept the terms in [CONTRIBUTING.md § Licensing of contributions](...)` ticked, plus the ticked checklist. `gh pr create --body` with free text fails the `contribution-licence-guard` CI job (`scripts/checks/contribution-licence.sh` greps for that exact line), and the `conclusion` check then blocks the merge.

**Why:** the owner flagged on 2026-09-22 that PRs kept failing this guard ("you are every time missing this"); the check exists since #575.

**How to apply:** write every PR body from the template (also in subagent prompts), tick the licensing box and the checklist items that hold, keep `Closes #N` under "What changed and why". Fix an existing PR with `gh pr edit <n> --body`. See [[auto-merge-every-pr]] and [[repo-merge-gates]].
