#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every first-party file names one copyright holder, the Licensor of LICENSE
# (no specification governs this: our own design). A header that names any
# other holder, typed by hand or carried in from an older branch, fails here.
# Vendored trees keep their upstream terms and are skipped: every directory
# that holds a PROVENANCE.md.
#
#   scripts/checks/copyright-holder.sh              check the tree
#   scripts/checks/copyright-holder.sh --self-test  check the check
#
# Exit 0 when every SPDX-FileCopyrightText header line names the holder. Exit
# 1 naming each file and line that does not.
set -euo pipefail

if [[ "${1:-}" == "--self-test" ]]; then
  script="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/scripts/checks" "$tmp/vendor"
  cp "$script" "$tmp/scripts/checks/copyright-holder.sh"
  printf '// SPDX-FileCopyrightText: Cadasto B.V.\n' >"$tmp/a.rs"
  printf '<!-- SPDX-FileCopyrightText: Cadasto B.V. -->\n' >"$tmp/b.md"
  printf '/* SPDX-FileCopyrightText: Cadasto B.V. */\n' >"$tmp/c.css"
  printf 'Prose that quotes SPDX-FileCopyrightText: Someone Else.\n' >"$tmp/d.md"
  printf '# SPDX-FileCopyrightText: Upstream Authors\n' >"$tmp/vendor/e.sh"
  printf 'upstream\n' >"$tmp/vendor/PROVENANCE.md"
  (cd "$tmp" && git init -q && git add -A && bash scripts/checks/copyright-holder.sh >/dev/null 2>&1) || {
    echo "self-test: a tree naming only the holder failed" >&2
    exit 1
  }
  printf '# SPDX-FileCopyrightText: Someone Else\n' >"$tmp/f.toml"
  if out="$(cd "$tmp" && git add -A && bash scripts/checks/copyright-holder.sh 2>&1)"; then
    echo "self-test: a header naming another holder passed" >&2
    exit 1
  fi
  grep -q 'f.toml:1 names "Someone Else"' <<<"$out" || {
    echo "self-test: the other holder was not named" >&2
    exit 1
  }
  echo "ok: copyright-holder self-test (another holder fails; prose and vendored trees are skipped)"
  exit 0
fi

if [[ $# -ne 0 ]]; then
  echo "usage: $0 [--self-test]" >&2
  exit 2
fi

cd "$(dirname "$0")/../.."

readonly HOLDER="Cadasto B.V."

# The vendored trees, as git pathspec exclusions.
excludes=()
while IFS= read -r provenance; do
  excludes+=(":(exclude)$(dirname "$provenance")/**")
done < <(git ls-files '*PROVENANCE.md')

# A header line is the tag behind nothing but a comment marker: `//`, `#`,
# `<!--`, `/*` or `*`. Prose that quotes the tag, and an emitter that prints
# one, are not headers.
header='^[[:space:]]*(//|#|<!--|/\*|\*)[[:space:]]*SPDX-FileCopyrightText:'
fail=0
while IFS= read -r hit; do
  file=${hit%%:*}
  rest=${hit#*:}
  line=${rest%%:*}
  text=${rest#*:}
  holder=$(sed -E 's/.*SPDX-FileCopyrightText:[[:space:]]*//; s/[[:space:]]*(-->|\*\/)[[:space:]]*$//; s/[[:space:]]+$//' <<<"$text")
  if [[ "$holder" != "$HOLDER" ]]; then
    echo "copyright-holder: $file:$line names \"$holder\", not $HOLDER" >&2
    fail=1
  fi
done < <(git grep -nE "$header" -- . ${excludes[@]+"${excludes[@]}"} || true)

if [[ $fail -ne 0 ]]; then
  exit 1
fi
echo "copyright-holder: every header names $HOLDER"
