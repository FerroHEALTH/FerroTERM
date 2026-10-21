#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/rust_fmt_clippy.sh
#
# Claude Code PostToolUse hook (matcher: Write|Edit).
#
# For an edited .rs file: format it with rustfmt (never blocks, rustfmt
# failing to parse a draft is expected and fine), then run the comment-style
# guard (.claude/rules/comments.md), which CAN block (exit 2) to feed its
# findings back as a correction.
#
# For an edited .sh file: run shellcheck when it is available (never installs
# it; skips silently when absent).
#
# This hook does NOT run clippy per-edit, by design. A per-edit
# `cargo clippy` check-builds the owning crate plus its dependency cone on
# every file save and thrashes the cargo cache; clippy is a per-phase gate the
# agent runs explicitly (`cargo clippy --workspace --all-targets`). Everything
# here works with no Cargo workspace present (the project may be in DISCOVERY):
# rustfmt formats a single file standalone, and the comment guard reads text.

set -uo pipefail

payload="$(cat)" || true

if command -v jq >/dev/null 2>&1; then
  file_path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty' 2>/dev/null)" || true
else
  file_path="$(printf '%s' "$payload" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi

repo_root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"

case "${file_path:-}" in
*.rs) ;;
*.sh)
  [[ -f "$file_path" ]] || exit 0
  if command -v shellcheck >/dev/null 2>&1; then
    findings="$(shellcheck --severity=style "$file_path" 2>&1)" || {
      printf '%s\n' "$findings" >&2
      exit 2
    }
  fi
  exit 0
  ;;
*) exit 0 ;;
esac
[[ -f "$file_path" ]] || exit 0

if command -v rustfmt >/dev/null 2>&1; then
  rustfmt --edition 2024 "$file_path" >/dev/null 2>&1 || true
fi

# Comment-style guard (.claude/rules/comments.md): block comments, TODO(#N)
# form, NOTE/essay budgets. Exit 2 feeds the findings back as a correction.
if [[ -x "$repo_root/scripts/checks/comment-style.sh" ]]; then
  findings="$("$repo_root/scripts/checks/comment-style.sh" --files "$file_path" 2>&1)" || {
    printf '%s\n' "$findings" >&2
    exit 2
  }
fi

exit 0
