#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The evidence in docs/claims.md stays verifiable: a citation names a file and
# a symbol or test (`path.rs::name`), never a line number, because line
# numbers rot silently the first time code above them moves. This check fails
# on any `.rs:<line>` citation, on a cited file that does not exist, and on a
# cited symbol or test name that no longer occurs in its file.
#
# Usage: scripts/checks/claims.sh   (no arguments)
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "$#" -ne 0 ]; then
  echo "usage: $0   (takes no arguments)" >&2
  exit 2
fi

doc=docs/claims.md
status=0

if grep -nE '[A-Za-z0-9_./-]+\.rs:[0-9]+' "$doc"; then
  echo "error: $doc cites a line number; cite path.rs::symbol or path.rs::test_name instead" >&2
  status=1
fi

checked=0
while IFS= read -r cite; do
  path="${cite%%::*}"
  name="${cite#*::}"
  checked=$((checked + 1))
  if [ ! -f "$path" ]; then
    echo "error: $doc cites $path, which does not exist" >&2
    status=1
    continue
  fi
  if ! grep -qF "$name" "$path"; then
    echo "error: $doc cites $cite, and '$name' does not occur in $path" >&2
    status=1
  fi
done < <(grep -oE '`[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)+\.rs::[A-Za-z0-9_]+' "$doc" | tr -d '`' | sort -u)

[ "$status" -eq 0 ] && echo "ok: claims ($checked symbol and test citations resolve, no line numbers)"
exit "$status"
