#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# code-systems.sh — the served code systems are listed once.
#
# The book's table (website/book/src/evaluate/code-systems.md, between the
# `code-systems:begin` and `code-systems:end` markers) is the single source
# of the code systems FerroTERM serves. The README and the landing page carry
# a short form of the same list; this check fails when a system named in the
# table is missing from either, so the three cannot drift.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly TABLE=website/book/src/evaluate/code-systems.md
readonly TARGETS=(README.md website/landing/index.html)

# The first column of every table row between the markers, backticks removed.
names="$(awk '/code-systems:begin/{p=1; next} /code-systems:end/{p=0} p && /^\| [^-|]/{print}' "$TABLE" \
  | tail -n +2 \
  | awk -F'|' '{gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2); gsub(/`/, "", $2); print $2}')"

if [[ -z "$names" ]]; then
  echo "code-systems: no rows found between the markers in $TABLE" >&2
  exit 1
fi

status=0
while IFS= read -r name; do
  for target in "${TARGETS[@]}"; do
    section="$(awk '/code-systems:begin/{p=1; next} /code-systems:end/{p=0} p' "$target" | tr -d '`' | tr '\n' ' ' | tr -s ' ')"
    if [[ -z "$section" ]]; then
      echo "code-systems: $target has no code-systems:begin/end markers" >&2
      status=1
      continue
    fi
    if ! printf '%s' "$section" | grep -qF -- "$name"; then
      echo "code-systems: '$name' is in $TABLE but not in $target" >&2
      status=1
    fi
  done
done <<< "$names"

if [[ "$status" -eq 0 ]]; then
  count="$(printf '%s\n' "$names" | wc -l | tr -d ' ')"
  echo "code-systems: OK ($count systems listed in the book, the README, and the landing page)."
fi
exit "$status"
