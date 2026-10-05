---
name: phase-status
description: Prints the tracker's open issues (pinned first, with native relationships), the in-flight issue's unchecked acceptance criteria, and a short git status. Use when the user asks "where are we", "what's in flight", or at the start of a work session to orient.
allowed-tools: Read, Bash
argument-hint: (none)
---

# /phase-status

A fast orientation dump, the orient step of the issue workflow
(`.claude/rules/issue-workflow.md`). Read-only; makes no changes. The live
state below is injected at invocation time; ground the answer in it, not in
stale conversation memory.

## Live state (injected)

### The tracker: open GitHub issues (with native relationships)

Each line carries `<Type/Priority>` (the native issue type and the `Priority`
field, `?` where unset), `{k/n}` sub-issue progress, `child-of #parent`, and
open `BLOCKED-by`/`blocks` edges (`.claude/rules/issue-relationships.md`).

```!
gh api graphql -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issues(first: 100, states: OPEN, orderBy: {field: CREATED_AT, direction: DESC}) { nodes { number title issueType { name } issueFieldValues(first: 10) { nodes { ... on IssueFieldSingleSelectValue { field { ... on IssueFieldSingleSelect { name } } value } } } labels(first: 20) { nodes { name } } milestone { title } parent { number } subIssuesSummary { total completed } blockedBy(first: 30) { nodes { number state } } blocking(first: 30) { nodes { number state } } } } } }' -f owner="$(gh repo view --json owner --jq .owner.login)" -f name="$(gh repo view --json name --jq .name)" --jq '.data.repository.issues.nodes[] | ((.issueType.name // "?") + "/" + ([.issueFieldValues.nodes[] | select(.field.name == "Priority") | .value] | first // "?")) as $tp | ([.labels.nodes[].name] | join(", ")) as $labels | ([.blockedBy.nodes[] | select(.state == "OPEN") | "#\(.number)"]) as $b | ([.blocking.nodes[] | select(.state == "OPEN") | "#\(.number)"]) as $k | "#\(.number)  \(.title)  <\($tp)>  [\($labels)]" + (if .milestone then "  (\(.milestone.title))" else "" end) + (if .parent then "  child-of #\(.parent.number)" else "" end) + (if .subIssuesSummary.total > 0 then "  {\(.subIssuesSummary.completed)/\(.subIssuesSummary.total)}" else "" end) + (if ($b | length) > 0 then "  BLOCKED-by \($b | join(","))" else "" end) + (if ($k | length) > 0 then "  blocks \($k | join(","))" else "" end)'
```

### Git

```!
cd "${CLAUDE_PROJECT_DIR}" && git status --short | head -40 && echo "---" && git log --oneline -5
```

## Steps

1. Summarize the tracker state: which issue is the current focus (pinned /
   milestone-assigned / the one the branch implements) and what its stated
   next action is. For the in-flight issue, run `gh issue view <n> --json
   title,body,comments --jq '.title, .body, (.comments[] | "--- comment ---",
   .body)'` (`--comments` prints nothing for an issue without comments,
   `issue-workflow.md` §The loop) and report the latest status comment and the
   unchecked `## Acceptance criteria` boxes, plus `scripts/gh/rel.sh tree <n>`
   for its parent, children, and blockers. **Flag any open issue shown
   `BLOCKED-by` an open blocker:** it is stuck until the blocker closes and is
   not a pickup candidate (`.claude/rules/issue-relationships.md`). **Flag any
   open issue shown `<?/?>` or with a `?` half:** it lacks its type or
   priority (a scheduled lane filed it), and `scripts/gh/fields.sh` sets them.
2. Summarize the git state from the injected output: current branch work,
   uncommitted files, last commits. Flag uncommitted work that looks finished;
   never leave finished work sitting unmerged. Flag an open pull request
   without auto-merge armed (`gh pr list --json number,autoMergeRequest`).
3. If the user asks how things look publicly, `scripts/gh/project.sh board`
   prints the public roadmap board grouped by Status, a presentation VIEW
   over the same issues, never a second source of truth
   (`.claude/rules/project-board.md`); the issue list above stays the working
   ground truth. Flag any `In Progress` item nobody is actually working on (it
   should be parked back to `todo`). (If the board does not exist yet, the
   command fails loud; note that and continue.)
4. **Do not** modify any issue or make any commit; this is a read-only status
   check. If the user wants the next task turned into a work plan, point them
   at `/next-task` instead.
