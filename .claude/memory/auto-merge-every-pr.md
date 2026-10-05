---
name: auto-merge-every-pr
description: "owner directive 2026-09-09, arm auto-merge on every PR at open time with gh pr merge <n> --auto (the merge queue since 2026-10-05) and keep working; do not wait for the owner to merge"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: f1d07f84-66e2-401d-8ebd-76aea8bebf4f
  modified: 2026-09-09T04:46:38.713Z
---

Enable auto-merge on every pull request as soon as it is opened:
`gh pr merge <n> --auto`. Then carry on with the next slice rather than
waiting for the merge.

Since 2026-10-05 `main` merges through GitHub's merge queue (#704), which sets
the merge method itself and refuses `--squash` and `--delete-branch`; the
repository deletes a merged head branch on its own. `.claude/settings.json`
allows `gh pr merge * --auto` and denies `--admin` and `-R`/`--repo`.

**Why:** the owner asked for it directly ("set always on auto merge please and
keep going after it got merged"). Stopping to ask for each merge stalled the
milestone loop several times in one session, and every one of those PRs was
green when it was asked about.

**How to apply:** open the PR, enable auto-merge, keep going. A PR whose bundle
guard is still red is BLOCKED, so pushing the recorded figure to it is still
allowed under [[auto-merge-follow-ups]]. Confirm a queued merge with
`gh pr view <n> --json state,mergedAt`. After it merges, rebase the next
stacked branch with `git rebase --onto origin/main <old-base>`, because merges
here are squashes. Supersedes the "the owner merges" half of
[[repo-merge-gates]].
