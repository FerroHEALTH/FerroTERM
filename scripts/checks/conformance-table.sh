#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The book's breakdown of the tx-ecosystem general mode agrees with the pass
# lists (#1409; no specification governs this: our own design). The page
# website/book/src/evaluate/conformance.md states the pass count per surface,
# a count per cause and a total per surface, and says how many cases do not
# apply. Each is checked against conformance/tx-ecosystem/: the pass lists,
# total.txt and not-applicable.tsv. The rows have to add up to the total, and
# the first two rows have to match the inventory.
#
#   conformance-table.sh [book page]
#   conformance-table.sh --self-test
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly DIR=conformance/tx-ecosystem

# The pass count of surface $1 (r4, r4b, r5).
passed() {
  local list=$DIR/passing.txt
  [[ "$1" = r4b ]] || list="$DIR/passing-$1.txt"
  grep -c . "$list"
}

# The rows of not-applicable.tsv for surface $1 with reason $2 (any when empty).
inapplicable() {
  awk -F'\t' -v surface="$1" -v reason="$2" \
    '!/^#/ && $1 == surface && (reason == "" || $3 == reason)' "$DIR/not-applicable.tsv" | wc -l | tr -d ' '
}

# Checks the page $1 against the lists; prints each disagreement and returns 1
# when there is one.
check() {
  local page=$1 failed=0 total
  total=$(tr -d '[:space:]' < "$DIR/total.txt")
  fail() { echo "conformance-table: $page: $*" >&2; failed=1; }

  grep -qF "The \`general\` mode counts $total cases on every surface." "$page" \
    || fail "does not say the general mode counts $total cases"
  local r4 r4b r5
  r4=$(passed r4); r4b=$(passed r4b); r5=$(passed r5)
  grep -qF "\`/r4\` passes $r4, \`/r4b\` $r4b and \`/r5\` $r5." "$page" \
    || fail "does not state the pass counts $r4 (r4), $r4b (r4b) and $r5 (r5)"

  # The table rows: the last three cells of each row are R4, R4B and R5.
  local rows
  rows=$(awk -F'|' '
    /^\| Why the case does not pass \|/ { inside = 1; next }
    inside && /^\|---/ { next }
    inside && /^\|/ { n = NF; print $(n-3) "\t" $(n-2) "\t" $(n-1); next }
    inside { exit }' "$page" | tr -d ' ')
  if [[ -z "$rows" ]]; then
    fail "carries no breakdown table"
    return 1
  fi
  local body totals sums
  body=$(sed '$d' <<< "$rows")
  totals=$(tail -n 1 <<< "$rows")
  sums=$(awk -F'\t' '{ a += $1; b += $2; c += $3 } END { print a "\t" b "\t" c }' <<< "$body")
  [[ "$sums" == "$totals" ]] || fail "the rows sum to $sums, and the total row says $totals"
  local want_totals
  want_totals="$((total - r4))	$((total - r4b))	$((total - r5))"
  [[ "$totals" == "$want_totals" ]] \
    || fail "the total row says $totals, and the lists leave $want_totals not passing"
  local gated surface_row
  gated="$(inapplicable r4 gated)	$(inapplicable r4b gated)	$(inapplicable r5 gated)"
  surface_row="$(inapplicable r4 r4b-surface)	$(inapplicable r4b r4b-surface)	$(inapplicable r5 r4b-surface)"
  [[ "$(sed -n 1p <<< "$body")" == "$gated" ]] \
    || fail "the first row says $(sed -n 1p <<< "$body"), and not-applicable.tsv gates $gated"
  [[ "$(sed -n 2p <<< "$body")" == "$surface_row" ]] \
    || fail "the second row says $(sed -n 2p <<< "$body"), and not-applicable.tsv holds $surface_row R4B-surface cases"

  local na4 na4b na5
  na4=$(inapplicable r4 ""); na4b=$(inapplicable r4b ""); na5=$(inapplicable r5 "")
  local flat
  flat=$(tr '\n' ' ' < "$page" | tr -s ' ')
  grep -qF "$na4 cases on R4, $na4b on R4B and $na5 on R5" <<< "$flat" \
    || fail "does not say $na4, $na4b and $na5 cases do not apply"
  grep -qF "of the $((total - r4)) cases R4 does not pass, $((total - r4 - na4)) are FerroTERM's to answer for, $((total - r5 - na5)) on R5 and $((total - r4b - na4b)) on R4B" <<< "$flat" \
    || fail "does not say how many cases are FerroTERM's to answer for: $((total - r4 - na4)), $((total - r5 - na5)) and $((total - r4b - na4b))"
  return "$failed"
}

if [[ "${1:-}" == "--self-test" ]]; then
  work=$(mktemp -d)
  trap 'rm -r "$work"' EXIT
  check website/book/src/evaluate/conformance.md
  # A page whose total row no longer adds up is refused.
  awk '/^\| \*\*Total not passing\*\*/ { sub(/\| [0-9]+ \|$/, "| 999 |") } { print }' \
    website/book/src/evaluate/conformance.md > "$work/broken.md"
  if check "$work/broken.md" 2>/dev/null; then
    echo "conformance-table: self-test: a total row that does not add up passes" >&2
    exit 1
  fi
  echo "conformance-table: self-test OK"
  exit 0
fi

page=${1:-website/book/src/evaluate/conformance.md}
check "$page"
echo "conformance-table: OK ($page agrees with the pass lists)"
