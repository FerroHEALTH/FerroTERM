#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The EHDS badge corpora stay true: every row of conformance/ehds/art6-code-systems.tsv
# has three tab-separated columns and its proof, when not `-`, names a test
# function that exists, and conformance/ehds/mvc-synthetic.tsv has two columns
# with a known answer. The badges (conformance-badges.sh) count these files,
# so a badge never claims a loader no test proves.
#
#   ehds-matrix.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

matrix=conformance/ehds/art6-code-systems.tsv
mvc=conformance/ehds/mvc-synthetic.tsv
fail=0
rows=0
while IFS=$'\t' read -r system canonical proof extra; do
  [[ -z "$system" || "$system" == \#* ]] && continue
  rows=$((rows + 1))
  if [[ -z "$canonical" || -z "$proof" || -n "${extra:-}" ]]; then
    echo "ehds-matrix: $matrix: '$system' needs exactly three columns" >&2
    fail=1
    continue
  fi
  [[ "$proof" == "-" ]] && continue
  file="${proof%%::*}"
  function="${proof##*::}"
  if [[ "$file" == "$proof" || ! -f "$file" ]]; then
    echo "ehds-matrix: $matrix: '$system' names no test file: $proof" >&2
    fail=1
  elif ! grep -qE "^(async )?fn $function\(" "$file"; then
    echo "ehds-matrix: $matrix: '$system' names $function, which $file does not define" >&2
    fail=1
  fi
done < "$matrix"
if [[ "$rows" -ne 16 ]]; then
  echo "ehds-matrix: $matrix lists $rows systems; the draft EEHRxF act names 16" >&2
  fail=1
fi
while IFS=$'\t' read -r url answer extra; do
  [[ -z "$url" || "$url" == \#* ]] && continue
  case "$answer" in
    expands | names-missing) ;;
    *) echo "ehds-matrix: $mvc: '$url' has no known answer: '$answer'" >&2; fail=1 ;;
  esac
  [[ -z "${extra:-}" ]] || { echo "ehds-matrix: $mvc: '$url' needs exactly two columns" >&2; fail=1; }
done < "$mvc"
[[ "$fail" -eq 0 ]] && echo "ehds-matrix: OK ($rows systems)"
exit "$fail"
