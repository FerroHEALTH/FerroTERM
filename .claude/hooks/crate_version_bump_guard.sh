#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# PreToolUse hook (matcher: Bash). Blocks `git push` when the outgoing commits
# (against the merge-base with origin/main) change packaged content of the
# published crates/* members without bumping the lockstep crate version
# (.claude/rules/crates-publishing.md). The authoritative twin is the
# crate-version-guard CI job; this hook fails the push before CI would.
#
# Escape (mirrors the CI `no-crate-bump` label, for a diff that provably does
# not alter packaged bytes): FERROTERM_SKIP_CRATE_BUMP_GUARD=1 git push ...
#
# Reads the tool-call JSON on stdin. Exit 2 blocks; exit 0 allows.
set -euo pipefail

payload="$(cat)"
if command -v jq >/dev/null 2>&1; then
  cmd="$(printf '%s' "$payload" | jq -r '.tool_input.command // empty' 2>/dev/null || true)"
else
  cmd="$payload"
fi
[[ -n "${cmd:-}" ]] || exit 0
printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])git[[:space:]]+push([[:space:]]|$)' || exit 0
printf '%s' "$cmd" | grep -q 'FERROTERM_SKIP_CRATE_BUMP_GUARD=1' && exit 0

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0
base="$(git merge-base HEAD origin/main 2>/dev/null || true)"
[[ -n "$base" ]] || exit 0
[[ -x scripts/checks/crate-version-guard.sh ]] || exit 0

if ! out="$(scripts/checks/crate-version-guard.sh "$base" HEAD 2>&1)"; then
  printf '%s\n' "$out" >&2
  echo "BLOCKED: the outgoing commits fail the crate-line bump rule above (.claude/rules/crates-publishing.md). Bump the crate line, or re-run with FERROTERM_SKIP_CRATE_BUMP_GUARD=1 and apply the 'no-crate-bump' label to the PR." >&2
  exit 2
fi
exit 0
