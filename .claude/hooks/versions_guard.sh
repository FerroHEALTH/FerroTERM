#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/versions_guard.sh
#
# Claude Code PostToolUse hook (matcher: Write|Edit).
#
# When an edited file is the pin matrix or a file that repeats a pin (the
# product version, the licence parameters, a vendored corpus's PROVENANCE.md
# or the vendor script that writes it), run scripts/checks/versions.sh. Exit 2
# feeds its findings back as a correction; every other path is a quiet exit 0.

set -uo pipefail

payload="$(cat)" || true

if command -v jq >/dev/null 2>&1; then
  file_path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty' 2>/dev/null)" || true
else
  file_path="$(printf '%s' "$payload" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi

[[ -n "${file_path:-}" ]] || exit 0

repo_root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
rel="${file_path#"$repo_root"/}"
base="$(basename "$file_path")"

watched=0
case "$rel" in
docs/VERSIONS.md | LICENSE | NOTICE | Cargo.toml | rust-toolchain.toml | CITATION.cff | compose.yaml | README.md | website/landing/index.html) watched=1 ;;
*) ;;
esac
case "$base" in
VERSIONS.md | LICENSE | NOTICE | rust-toolchain.toml | CITATION.cff | compose.yaml | README.md | Dockerfile | PROVENANCE.md) watched=1 ;;
*) ;;
esac
case "$rel" in
scripts/vendor/*.sh) watched=1 ;;
*) ;;
esac

[[ "$watched" -eq 1 ]] || exit 0
[[ -x "$repo_root/scripts/checks/versions.sh" ]] || exit 0

findings="$("$repo_root/scripts/checks/versions.sh" 2>&1)" || {
  printf '%s\n' "$findings" >&2
  exit 2
}

exit 0
