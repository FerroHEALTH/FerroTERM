#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/session_start_issues.sh
#
# Claude Code SessionStart hook: prints the open GitHub issue list (the
# tracker, .claude/rules/issue-workflow.md) annotated with native issue
# relationships (parent/sub-issue progress + blocked-by/blocking, via one
# batched GraphQL call, see .claude/rules/issue-relationships.md), git
# status, and the last 10 commits, so every session starts oriented.
#
# Graceful when `gh` is missing, unauthenticated, or there are no issues: each
# `gh` call falls back to a short note and the hook still exits 0 (a hook that
# fails must never block a session from starting).

set -uo pipefail

root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
cd "$root" || exit 0

if ! command -v gh >/dev/null 2>&1; then
  echo "=== tracker ==="
  echo "(gh CLI not installed, install it to see the open issue list)"
  gh_ok=0
elif ! gh auth status >/dev/null 2>&1; then
  echo "=== tracker ==="
  echo "(gh is not authenticated, run 'gh auth login' to see the open issue list)"
  gh_ok=0
else
  gh_ok=1
fi

if [[ "$gh_ok" = "1" ]]; then
  repo_nwo="$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)"
  echo "=== tracker: open GitHub issues (read one with: gh issue view <n> --json title,body,comments) ==="
  echo "--- pinned (current focus) ---"
  # shellcheck disable=SC2016 # $owner/$name are GraphQL variables, expanded by the server
  gh api graphql \
    -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { pinnedIssues(first: 3) { nodes { issue { number title } } } } }' \
    -f owner="${repo_nwo%%/*}" -f name="${repo_nwo##*/}" \
    --jq '.data.repository.pinnedIssues.nodes[].issue | "#\(.number)  \(.title)"' 2>/dev/null || echo "  (none)"
  echo "--- open (<Type/Priority>; child-of = sub-issue; {k/n} = sub-issue progress; BLOCKED-by = has an open blocker, not a /next-task candidate; relationships: .claude/rules/issue-relationships.md) ---"
  # One batched GraphQL call yields each open issue's labels, milestone, parent,
  # sub-issue progress, and open blockers/blocks, so the tracker shows work
  # structure, not just a flat list.
  # shellcheck disable=SC2016 # $owner/$name are GraphQL variables and $tp/$labels/$b/$k are jq bindings
  issues="$(gh api graphql \
    -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issues(first: 100, states: OPEN, orderBy: {field: CREATED_AT, direction: DESC}) { nodes { number title issueType { name } issueFieldValues(first: 10) { nodes { ... on IssueFieldSingleSelectValue { field { ... on IssueFieldSingleSelect { name } } value } } } labels(first: 20) { nodes { name } } milestone { title } parent { number } subIssuesSummary { total completed } blockedBy(first: 30) { nodes { number state } } blocking(first: 30) { nodes { number state } } } } } }' \
    -f owner="${repo_nwo%%/*}" -f name="${repo_nwo##*/}" \
    --jq '.data.repository.issues.nodes[]
      | ((.issueType.name // "?") + "/" + ([.issueFieldValues.nodes[] | select(.field.name == "Priority") | .value] | first // "?")) as $tp
      | ([.labels.nodes[].name] | join(", ")) as $labels
      | ([.blockedBy.nodes[] | select(.state == "OPEN") | "#\(.number)"]) as $b
      | ([.blocking.nodes[]  | select(.state == "OPEN") | "#\(.number)"]) as $k
      | "#\(.number)  \(.title)  <\($tp)>  [\($labels)]"
        + (if .milestone then "  (\(.milestone.title))" else "" end)
        + (if .parent then "  child-of #\(.parent.number)" else "" end)
        + (if .subIssuesSummary.total > 0 then "  {\(.subIssuesSummary.completed)/\(.subIssuesSummary.total)}" else "" end)
        + (if ($b | length) > 0 then "  BLOCKED-by \($b | join(","))" else "" end)
        + (if ($k | length) > 0 then "  blocks \($k | join(","))" else "" end)' 2>/dev/null || true)"
  if [[ -n "$issues" ]]; then echo "$issues"; else echo "  (no open issues)"; fi
  echo
fi

echo "=== git status ==="
git status --short --branch 2>/dev/null | head -40
echo
echo "=== last 10 commits ==="
git log --oneline -10 2>/dev/null

exit 0
