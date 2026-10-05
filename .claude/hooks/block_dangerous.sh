#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/block_dangerous.sh
#
# Claude Code PreToolUse hook (matcher: Bash). Blocks destructive commands:
#   - a recursive forced delete (delete specific files, use git rm, or work
#     under /tmp)
#   - force-pushes touching main or master, and bare force-pushes
#   - deleting main or master on a remote, and mirror pushes
#   - deletion of LICENSE or CLAUDE.md (the licence and the working discipline)
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

# A delete carrying a recursive flag (-r, -R, --recursive) and a force flag
# (-f, --force), in any spelling or order, unless every path it names sits
# under /tmp. Each command of a compound line is judged on its own, so one
# /tmp delete never clears another delete on the same line.
recursive_forced_delete() {
  local segment word recursive force path outside
  while IFS= read -r segment; do
    # Word splitting is the tokenizer here, with globbing off.
    set -f
    # shellcheck disable=SC2086
    set -- $segment
    set +f
    # Find rm wherever it stands (after sudo, xargs, then, `bash -c "`, a
    # backslash), with quotes and backslashes stripped from the word.
    word=""
    while [[ $# -gt 0 ]]; do
      word="${1//[\"\'\\]/}"
      shift
      [[ "$word" == rm || "$word" == */rm ]] && break
    done
    [[ "${word:-}" == rm || "${word:-}" == */rm ]] || continue
    recursive=0 force=0 outside=0 path=0
    for word in "$@"; do
      case "$word" in
        --recursive) recursive=1 ;;
        --force) force=1 ;;
        --*) ;;
        -*)
          [[ "$word" == *[rR]* ]] && recursive=1
          [[ "$word" == *f* ]] && force=1
          ;;
        *)
          path=1
          word="${word//[\"\'\\]/}"
          if [[ ! ( "$word" == /tmp/* || "$word" == /private/tmp/* ) || "$word" == *..* ]]; then
            outside=1
          fi
          ;;
      esac
    done
    if [[ "$recursive" -eq 1 && "$force" -eq 1 && ( "$outside" -eq 1 || "$path" -eq 0 ) ]]; then
      return 0
    fi
  done <<<"${cmd//[;&|()\`]/$'\n'}"
  return 1
}

if recursive_forced_delete; then
  echo "BLOCKED: a recursive forced delete is not allowed (block_dangerous hook). Delete specific files with 'git rm' or a plain 'rm <file>', or operate under /tmp." >&2
  exit 2
fi

# Deleting main or master on a remote is never allowed: `git push origin
# :main`, `--delete main`, or `-d main`, and a mirror push, which can delete
# every remote branch.
if printf '%s' "$cmd" | grep -qE 'git[[:space:]]+push[^;|&]*([[:space:]]:(refs/heads/)?(main|master)([[:space:]]|$)|--delete[^;|&]*[[:space:]](main|master)([[:space:]]|$)|[[:space:]]-d[[:space:]][^;|&]*(main|master)([[:space:]]|$)|--mirror)'; then
  echo "BLOCKED: deleting main or master on a remote, or a mirror push, is forbidden (CLAUDE.md hard rule)." >&2
  exit 2
fi

# Force pushes: never to main or master; a bare force-push is refused too.
# The protected names match only as WHOLE REF WORDS (delimiter-bounded, so
# `refs/heads/main`, `origin main`, and `HEAD:main` all hit) and never as raw
# substrings of the command line, which would falsely block a feature branch
# whose name merely CONTAINS a protected name.
if printf '%s' "$cmd" | grep -qE 'git[[:space:]]+push[^;|&]*(--force([^-]|$)|--force-with-lease|[[:space:]]-f([[:space:]]|$)|[[:space:]]\+[[:alnum:]])'; then
  if printf '%s' "$cmd" | grep -qE '(^|[[:space:]:/+])(main|master)([[:space:]]|$|["'"'"';&|])'; then
    echo "BLOCKED: force-push touching main or master is forbidden (CLAUDE.md hard rule)." >&2
    exit 2
  fi
  if ! printf '%s' "$cmd" | grep -qE '(feat|fix|chore|docs|refactor|perf|test|ci|build|release)/'; then
    echo "BLOCKED: bare force-push refused. Force-push (prefer --force-with-lease) only an explicit conventional-type branch (feat/, fix/, chore/, docs/, refactor/, perf/, test/, ci/, build/, release/)." >&2
    exit 2
  fi
fi

# Never delete the licence or the working discipline.
if printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])(git[[:space:]]+rm|rm)[^;|&]*(LICENSE|CLAUDE\.md)([[:space:]]|$|["'"'"';&|])'; then
  echo "BLOCKED: LICENSE (the Business Source License 1.1, the project licence) and CLAUDE.md (the working discipline) must not be deleted." >&2
  exit 2
fi

exit 0
