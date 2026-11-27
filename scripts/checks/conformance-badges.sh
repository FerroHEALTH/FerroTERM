#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The conformance badges: one shields.io endpoint JSON per served FHIR version,
# derived from the committed pass lists of the HL7 terminology ecosystem suite
# (conformance/tx-ecosystem/, the lists CI gates on every push) and the suite
# total the suite script records, one per mode run by hand before a release
# (conformance/tx-ecosystem/hand-run.tsv, whose day conformance/tx-ecosystem/README.md
# names), one naming the FHIR versions served, and the EHDS badges over
# conformance/ehds/. The numbers are never typed by hand: a badge
# says what the pass list says. The docs lane writes them into the site under
# /conformance/, and the README badges read them through
# https://img.shields.io/endpoint (https://shields.io/badges/endpoint-badge).
# Every badge, and every static badge in the README, carries a logo from
# scripts/checks/badge-logos/: the EU emblem on EU blue for an EU act, else a
# pictogram. A badge without one fails. The post-quantum badge counts the
# safe uses of conformance/post-quantum.tsv. Every run fails when the README
# shows a badge this script does not write or leaves out one it writes, and
# when the pass-list table of conformance/tx-ecosystem/README.md disagrees
# with the lists, the suite total or hand-run.tsv, and when the book's
# conformance page miscounts the cases not-applicable.tsv lists. A
# badge whose label runs past 30 characters or whose message runs past 20
# fails the run.
#
#   conformance-badges.sh <out-dir>
#   conformance-badges.sh --self-test   # a claimed certification or a missing logo is refused
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/badge-logos/logo.sh
source scripts/checks/badge-logos/logo.sh

out="${1:?usage: conformance-badges.sh <out-dir> | --self-test}"
self_test=""
if [[ "$out" == "--self-test" ]]; then
  self_test=1
  out="$(mktemp -d)"
  trap 'rm -r "$out"' EXIT
fi
mkdir -p "$out"

total="$(tr -d '[:space:]' < conformance/tx-ecosystem/total.txt)"
# The suite version the pin table names (docs/VERSIONS.md is the source of truth).
suite="$(grep -oE 'test cases [0-9]+\.[0-9]+\.[0-9]+' docs/VERSIONS.md | head -1 | sed 's/test cases //')"
[[ -n "$suite" ]] || { echo "conformance-badges: docs/VERSIONS.md names no suite version" >&2; exit 1; }

for version in r4 r4b r5 r6; do
  list=conformance/tx-ecosystem/passing.txt
  [[ "$version" = r4b ]] || list="conformance/tx-ecosystem/passing-$version.txt"
  [[ -f "$list" ]] || continue
  passed="$(grep -c . "$list" || true)"
  # The cases the runner counts that do not apply on this surface, from the
  # inventory scripts/checks/tx-ecosystem.sh holds to each run (#1361).
  inapplicable="$(awk -F'\t' -v surface="$version" '!/^#/ && $1 == surface' conformance/tx-ecosystem/not-applicable.tsv | wc -l | tr -d ' ')"
  message="$passed/$total"
  colour="$(badge_count_color "$passed" "$total")"
  label="$(printf '%s' "$version" | tr '[:lower:]' '[:upper:]')"
  jq -cn --arg label "$label" --arg message "$message" --arg color "$colour" \
    --argjson logo "$(badge_logo_fields shield-check)" \
    '{schemaVersion: 1, label: $label, message: $message, color: $color} + $logo' > "$out/$version.json"
  echo "conformance-badges: $version $message, $inapplicable n/a ($colour)"
done

# The EHDS badges, self-tested: each counts a committed corpus that a test or
# scripts/checks/ehds-matrix.sh holds true, and names the exact version it
# measures. A corpus that does not exist yet writes no badge. The badges stay
# short, and the small caption under the README's conformance heading says
# once what they count (owner decision, 2026-11-25); a README without it fails.
# Every badge's colour follows its count N/M by the shared rule: green when
# all pass, yellow when some do, red when none do.
#
#   badge FILE LABEL N M SUFFIX LOGO
badge() {
  local file="$1" label="$2" message="$3/$4$5" colour logo="$6"
  colour="$(badge_count_color "$3" "$4")"
  # A badge reports what a synthetic or free corpus proves, so it never claims
  # a certification or a conformity assessment (owner ruling, 2026-11-24).
  if printf '%s %s' "$label" "$message" | grep -qiE 'certified|compliant|conformant'; then
    echo "conformance-badges: '$label' claims a certification or conformity" >&2
    exit 1
  fi
  jq -cn --arg label "$label" --arg message "$message" --arg color "$colour" \
    --argjson logo "$(badge_logo_fields "$logo")" \
    '{schemaVersion: 1, label: $label, message: $message, color: $color} + $logo' > "$out/$file"
  echo "conformance-badges: $file $message"
}

# The pass list of a surface and a mode, named as scripts/checks/tx-ecosystem.sh
# names it.
pass_list() {
  local list=conformance/tx-ecosystem/passing
  [[ "$1" = r4b ]] || list="$list-$1"
  [[ "$2" = general ]] || list="$list-$2"
  printf '%s.txt' "$list"
}

# The rows of hand-run.tsv: surface, mode, the cases the run ran, and the day
# it was measured. scripts/checks/tx-ecosystem.sh fails a hand run whose count
# of cases differs from its row.
hand_run=conformance/tx-ecosystem/hand-run.tsv
hand_rows() {
  awk -F'\t' '!/^#/ && NF && $1 != "surface"' "$hand_run"
}
while IFS=$'\t' read -r surface mode ran measured; do
  list="$(pass_list "$surface" "$mode")"
  if [[ ! -f "$list" || ! "$ran" =~ ^[0-9]+$ || ! "$measured" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "conformance-badges: $hand_run row '$surface $mode $ran $measured' names no pass list, count, or date" >&2
    exit 1
  fi
  passed="$(grep -c . "$list" || true)"
  if [[ "$passed" -gt "$ran" ]]; then
    echo "conformance-badges: $list lists $passed cases, more than the $ran $hand_run says ran" >&2
    exit 1
  fi
  badge "hand-$(printf '%s' "$mode" | tr '.' '-')-$surface.json" \
    "$mode $(printf '%s' "$surface" | tr '[:lower:]' '[:upper:]')" \
    "$passed" "$ran" "" shield-check
done < <(hand_rows)

# The FHIR versions served, from the version table of the book's FHIR versions
# page; a version marked ballot-tracking there says so on the badge.
versions_page=website/book/src/integrate/fhir-versions.md
served="$(awk -F'|' '/served under `\/r[0-9a-z]+`/ {
  version = $2; gsub(/[[:space:]]/, "", version)
  printf "%s%s%s", sep, version, ($4 ~ /ballot/ ? " ballot" : ""); sep = "·"
}' "$versions_page")"
[[ -n "$served" ]] || { echo "conformance-badges: $versions_page names no served version" >&2; exit 1; }
jq -cn --arg message "$served" --argjson logo "$(badge_logo_fields exchange)" \
  '{schemaVersion: 1, label: "versions served", message: $message, color: "blue"} + $logo' > "$out/fhir-versions.json"
echo "conformance-badges: fhir-versions.json $served"

count_rows() {
  grep -cvE '^(#|[[:space:]]*$)' "$1" || true
}

mvc=conformance/ehds/mvc-synthetic.tsv
if [[ -f "$mvc" ]]; then
  listed="$(count_rows "$mvc")"
  badge ehds-mvc.json "MyHealth@EU MVC 10.1.0 import" \
    "$listed" "$listed" " value sets" eu
fi

matrix=conformance/ehds/art6-code-systems.tsv
if [[ -f "$matrix" ]]; then
  systems="$(count_rows "$matrix")"
  proven="$(grep -vE '^(#|[[:space:]]*$)' "$matrix" | awk -F'\t' '$3 != "-"' | wc -l | tr -d ' ')"
  badge ehds-art6.json "EEHRxF draft 2026-11-23 Art 6" \
    "$proven" "$systems" " loaders" eu
fi

# The post-quantum safe uses of conformance/post-quantum.tsv, drawn by the
# shared helper; scripts/checks/post-quantum.sh holds the file to the tree.
badge_post_quantum conformance/post-quantum.tsv > "$out/post-quantum.json"
echo "conformance-badges: post-quantum.json $(jq -r .message "$out/post-quantum.json")"

# Every badge is its count alone, within the shared length limits; the
# breakdown is the book's conformance page and conformance/tx-ecosystem/README.md.
badge_length_check "$out"/*.json || exit 1
badge_length_readme README.md || exit 1
badge_block_check README.md || exit 1
badge_render_check README.md || exit 1
# The suite badges carry no suite version, so their README heading names it.
grep -qxF "**HL7 tx-ecosystem $suite**<br>" README.md || {
  echo "conformance-badges: README.md has no heading **HL7 tx-ecosystem $suite**<br> over the suite badges" >&2
  exit 1
}

# The README shows exactly the badges this script writes.
readme_badges=README.md
written="$(find "$out" -maxdepth 1 -name '*.json' -exec basename {} \; | sort)"
shown="$(grep -oE 'ferroterm\.eu%2Fconformance%2F[^)]+\.json' "$readme_badges" | sed 's/.*%2F//' | sort -u)"
if [[ "$written" != "$shown" ]]; then
  echo "conformance-badges: the README's endpoint badges differ from the badges written (- written, + shown):" >&2
  diff <(printf '%s\n' "$written") <(printf '%s\n' "$shown") | grep -E '^[<>]' | sed -e 's/^</  -/' -e 's/^>/  +/' >&2
  exit 1
fi

# The pass-list table of conformance/tx-ecosystem/README.md says what the lists,
# the suite total and hand-run.tsv say, row for row, and its prose names the
# day of each hand run.
table=conformance/tx-ecosystem/README.md
expected_rows() {
  local version
  for version in r4b r4 r5 r6; do
    [[ ! -f "$(pass_list "$version" general)" ]] || printf '%s\tgeneral\t%s\n' "$version" "$total"
  done
  hand_rows | cut -f1-3
}
table_failures=0
while IFS=$'\t' read -r surface mode ran; do
  list="$(pass_list "$surface" "$mode")"
  row="| \`$mode\` | \`/$surface\` | \`${list##*/}\` | $(grep -c . "$list" || true) of $ran |"
  if ! grep -qF -- "$row" "$table"; then
    echo "conformance-badges: $table has no row starting $row" >&2
    table_failures=$((table_failures + 1))
  fi
done < <(expected_rows)
tick='`'
table_row="^[|] ${tick}[^${tick}]+${tick} [|] ${tick}/"
if [[ "$(grep -cE "$table_row" "$table")" != "$(expected_rows | wc -l | tr -d ' ')" ]]; then
  echo "conformance-badges: $table has a pass-list row that no list or hand-run.tsv row accounts for" >&2
  table_failures=$((table_failures + 1))
fi
for measured in $(hand_rows | cut -f4 | sort -u); do
  if ! tr -s '[:space:]' ' ' < "$table" | grep -qF "measured by hand on $measured"; then
    echo "conformance-badges: $table does not say the hand runs were measured by hand on $measured" >&2
    table_failures=$((table_failures + 1))
  fi
done
# The book's conformance page names, per surface, the general cases that do
# not apply there, as not-applicable.tsv counts them.
book=website/book/src/evaluate/conformance.md
not_applicable() {
  awk -F'\t' -v surface="$1" '!/^#/ && $1 == surface' conformance/tx-ecosystem/not-applicable.tsv | wc -l | tr -d ' '
}
inapplicable_sentence="do not apply on their surface: $(not_applicable r4) cases on R4, $(not_applicable r4b) on R4B and $(not_applicable r5) on R5."
if ! tr -s '[:space:]' ' ' < "$book" | grep -qF "$inapplicable_sentence"; then
  echo "conformance-badges: $book does not say the first two rows $inapplicable_sentence" >&2
  table_failures=$((table_failures + 1))
fi
[[ "$table_failures" -eq 0 ]] || exit 1

if [[ -n "$self_test" ]]; then
  # Each surface's badge is its count alone; the book page names the cases
  # that do not apply there.
  for version in r4 r4b r5; do
    if ! jq -er '.message | test("^[0-9]+/[0-9]+$")' "$out/$version.json" > /dev/null; then
      echo "conformance-badges: self-test: the $version badge is more than its count" >&2
      exit 1
    fi
  done
  if tr -s '[:space:]' ' ' < "$book" | grep -qF "do not apply on their surface: 999 cases on R4,"; then
    echo "conformance-badges: self-test: the not-applicable sentence matches a wrong count" >&2
    exit 1
  fi
  badge_length_check "$out"/*.json || exit 1
  badge long.json "tx-ecosystem 1.11.0 tx.fhir.org R4B" 29 189 ", by hand 2026-11-27" shield-check > /dev/null
  if badge_length_check "$out/long.json" 2>/dev/null; then
    echo "conformance-badges: self-test: a badge past the length limits passes" >&2
    exit 1
  fi
  rm "$out/long.json"
  badge_length_self_test > /dev/null || exit 1
  badge_count_color_self_test > /dev/null || exit 1
  printf '[![x](https://img.shields.io/badge/CRA-information%%20for%%20all%%20users-blue?logo=x)](x)\n' > "$out/long.md"
  if badge_length_readme "$out/long.md" 2>/dev/null; then
    echo "conformance-badges: self-test: a static badge past 20 characters passes" >&2
    exit 1
  fi
  rm "$out/long.md"
  for claim in "FHIR R4 certified" "EHDS compliant" "Conformant terminology"; do
    if (badge probe.json "$claim" 1 1 "" eu) 2>/dev/null; then
      echo "conformance-badges: self-test: '$claim' was written" >&2
      exit 1
    fi
  done
  badge probe.json "EEHRxF draft 2026-11-23 Art 6 code systems" 1 16 " loaders" eu > /dev/null
  badge_logo_check "$out"/*.json || exit 1
  badge_color_check "$out"/*.json || exit 1
  for want in "0 16 red" "1 16 yellow" "16 16 brightgreen"; do
    read -r passed whole colour <<<"$want"
    badge colour.json "EEHRxF draft 2026-11-23 Art 6 code systems" "$passed" "$whole" " loaders" eu > /dev/null
    [[ "$(jq -r .color "$out/colour.json")" == "$colour" ]] || {
      echo "conformance-badges: self-test: $passed of $whole is not $colour" >&2
      exit 1
    }
  done
  jq -c '.color = "blue"' "$out/colour.json" > "$out/blue.json"
  if badge_color_check "$out/blue.json" 2>/dev/null; then
    echo "conformance-badges: self-test: a counted badge in blue passes" >&2
    exit 1
  fi
  rm "$out/blue.json" "$out/colour.json"
  [[ "$(jq -r '.namedLogo + .labelColor' "$out/probe.json")" == "europeanunion#003399" ]] || {
    echo "conformance-badges: self-test: the EEHRxF badge carries no EU emblem on EU blue" >&2
    exit 1
  }
  jq -c 'del(.namedLogo)' "$out/probe.json" > "$out/bare.json"
  if badge_logo_check "$out/bare.json" 2>/dev/null; then
    echo "conformance-badges: self-test: a badge without a logo passes" >&2
    exit 1
  fi
  printf '[![x](https://img.shields.io/badge/x-y-blue)](x)\n' > "$out/README.md"
  if badge_logo_readme "$out/README.md" 2>/dev/null; then
    echo "conformance-badges: self-test: a static badge without a logo passes" >&2
    exit 1
  fi
  badge_logo_readme README.md || exit 1
  printf '**Conformance and compliance**<br>\n%s<br>\n' "$badge_caption" > "$out/README.md"
  badge_caption_check "$out/README.md" || exit 1
  printf '**Conformance and compliance**<br>\n' > "$out/README.md"
  if badge_caption_check "$out/README.md" 2>/dev/null; then
    echo "conformance-badges: self-test: a README without the conformance caption passes" >&2
    exit 1
  fi
  badge_caption_check README.md || exit 1
  echo "conformance-badges: self-test OK"
  exit 0
fi

