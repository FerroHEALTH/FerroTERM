---
name: repo-merge-gates
description: "main requires signed commits and the CI `conclusion` check and merges through the merge queue since 2026-10-05 (no strict up-to-date rule, no manual rebase train); the FerroTERM Roadmap board is a FerroHEALTH org project"
metadata: 
  node_type: memory
  type: project
  originSessionId: 1fab17b9-946d-49fc-af2e-4719ce97bc31
  modified: 2026-09-02T14:53:33.612Z
---

The `main` ruleset on FerroHEALTH/FerroTERM requires: one approving review plus code-owner review, signed commits (the owner's GPG key signs by default), and the `conclusion` status check with strict up-to-date policy. Merge commits are disabled; squash and rebase are allowed, and branches delete on merge.

**Why:** Claude cannot approve or merge its own PRs, so work stacks: the next issue's branch is cut from the previous issue's branch and its PR targets that branch until the base merges.

**How to apply:** Open the PR, watch `gh pr checks <n> --watch`, tick the issue criteria, post the handoff comment, and tell the owner the PR is waiting on their review. `scripts/gh/project.sh status <n> in-progress` fails loud until the owner creates the "FerroTERM Roadmap" Project v2 (`.claude/rules/project-board.md`); mention it, do not work around it.


**Update 2026-09-03:** at the owner's request the `main` ruleset's approving-review
requirement and code-owner review were set to zero/off (temporarily) so PRs
with auto-merge enabled merge once the `conclusion` check passes. Signed
commits and the status check stay. Enable auto-merge on every PR
(`gh pr merge <n> --auto --squash`); the owner can restore the review rule
later.

**Rule (owner, 2026-09-03, "very very important"):** auto-merge only fires when
the branch is fully up to date with `main` (the ruleset's strict status
checks). Before arming `gh pr merge --auto`, rebase the branch onto
`origin/main` and force-push with lease; after any other PR merges, rebase
every open PR again. Never leave a green PR sitting behind main.

**Update 2026-10-05 (#704):** the `main` ruleset carries the merge queue
(squash, `ALLGREEN`, up to 5 entries built at a time) and the strict
up-to-date policy is off. The queue builds each entry on top of the ones ahead
of it, so the rebase-every-open-PR rule above is retired: arm
`gh pr merge <n> --auto` and let the queue order the merges. A PR that
conflicts with `main` still needs a local signed rebase
(`gh pr update-branch --rebase` strips the signature). The "FerroTERM Roadmap"
board is a FerroHEALTH organisation project; the user-owned board never
existed.
