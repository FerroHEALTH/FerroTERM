#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every vendored-input and tool pin no ecosystem watches, compared with its
# newest upstream release (#965; no specification governs this: our own
# design). Dependabot reads the Cargo manifests and the `uses:` references;
# it does not read a grammar tag a vendor script fetches, a commit a corpus is
# pinned to, the validator jar the ecosystem suite runs on, or a browser
# version handed to an installer. scripts/checks/versions.sh catches drift
# between two files we control and cannot tell that upstream published a
# newer release, which is what this reads.
#
#   scripts/checks/pin-freshness.sh
#   scripts/checks/pin-freshness.sh --self-test
#
# Reads each pin from docs/VERSIONS.md: a release pin against the upstream
# project's newest GitHub release, a commit pin against the newest commit of
# the branch it follows, the ecosystem test cases against the `latest`
# version the FHIR package registry names for hl7.fhir.uv.tx-ecosystem, and
# Chrome for Testing against the newest Stable version its JSON endpoint
# lists. Needs an authenticated `gh`, curl and jq. `--self-test` runs the
# readers over fixtures and needs no network.
#
# A pin cell that ends in `held: <reason>` records why its pin stays behind
# the newest release; such a pin is reported HELD with its reason and never
# counts as stale.
#
# The FerroPULSE report envelope is not read: its repository is private, the
# workflow token cannot see it, and its pin is the family's ruling, moved by
# hand with the other FerroHEALTH products.
#
# Exit 0 when every pin is current or held, 1 when at least one is behind
# (each such line starts with STALE), 2 when a release could not be read, so a
# network failure never reads as a fresh pin.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly MATRIX=docs/VERSIONS.md

# One "matrix label<TAB>upstream repository" record per line: a pin that names
# a GitHub release by its tag. The label is the first cell of the row in
# docs/VERSIONS.md with its backticks stripped.
readonly WATCHED_RELEASES="\
ECL	IHTSDO/snomed-expression-constraint-language
FHIR Validator (txTests runner)	hapifhir/org.hl7.fhir.core
Unicode CLDR	unicode-org/cldr-json
UCUM	ucum-org/ucum
jscpd	kucherenko/jscpd"

# One "matrix label<TAB>upstream repository<TAB>branch" record per line: a
# corpus pinned by commit, read against the newest commit of its branch.
readonly WATCHED_COMMITS="\
SCG	IHTSDO/SNOMEDCT-Languages	master"

readonly SUITE_LABEL="HL7 terminology ecosystem test cases"
readonly SUITE_PACKAGE=https://packages.fhir.org/hl7.fhir.uv.tx-ecosystem
readonly R6_LABEL="FHIR R6 terminology bundles"
readonly R6_PACKAGE=https://packages2.fhir.org/packages/hl7.fhir.r6.core
readonly CHROME_LABEL="Chrome for Testing"
readonly CHROME_FEED=https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions.json

# The second cell of the matrix row whose first cell is $1, both with their
# backticks stripped.
matrix_cell() {
  local wanted_label="$1"
  awk -F'|' -v want="$wanted_label" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      gsub(/`/, "", value); gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (label == want) { print value; exit }
    }' "$MATRIX"
}

# The first word of a pin cell on stdin, without the comma or semicolon that
# ends it.
cell_version() {
  awk '{ v = $1; sub(/[,;]$/, "", v); print v; exit }'
}

# The first 40-hex commit of a pin cell on stdin.
cell_commit() {
  grep -o -E '[0-9a-f]{40}' | head -n 1
}

# The package version a pin cell on stdin names as `(test cases X)`.
cell_suite_version() {
  sed -n 's/.*(test cases \([0-9][0-9A-Za-z.-]*\)).*/\1/p' | head -n 1
}

# The reason a pin cell on stdin records for a pin held behind the newest
# release, the text after `held:` up to the end of the cell, or nothing.
cell_held() {
  sed -n 's/.*held: *\(.*\)$/\1/p' | sed 's/[[:space:]]*$//'
}

# A release tag without the `v` that prefixes a version number.
tag_version() {
  local tag=$1
  [[ $tag =~ ^v[0-9] ]] && tag="${tag#v}"
  printf '%s\n' "$tag"
}

# The `latest` version a FHIR package registry document on stdin names.
registry_latest() {
  jq -er '."dist-tags".latest | select(type == "string" and length > 0)'
}

# The Stable version in a Chrome for Testing last-known-good document on stdin.
chrome_stable() {
  jq -er '.channels.Stable.version | select(type == "string" and length > 0)'
}

stale=0
unreadable=0

# The line one pin LABEL at PINNED gets against the newest upstream LATEST,
# read from SOURCE, with the reason HELD gives for keeping it behind.
report() {
  local label=$1 pinned=$2 latest=$3 source=$4 held=$5
  if [[ "$pinned" = "$latest" ]]; then
    printf 'current    %s %s (%s)\n' "$label" "$pinned" "$source"
  elif [[ -n "$held" ]]; then
    printf 'HELD       %s: pinned %s, newest upstream %s, held: %s (%s)\n' "$label" "$pinned" "$latest" "$held" "$source"
  else
    printf 'STALE      %s: pinned %s, newest upstream %s (%s)\n' "$label" "$pinned" "$latest" "$source"
    stale=1
  fi
}

unreadable_pin() {
  printf 'UNREADABLE %s: %s\n' "$1" "$2"
  unreadable=1
}

self_test() {
  local failures=0 got
  expect() {
    local wanted="$1" got="$2" what="$3"
    if [[ "$got" != "$wanted" ]]; then
      printf 'pin-freshness: self-test: %s gave "%s", wanted "%s"\n' "$what" "$got" "$wanted" >&2
      failures=$((failures + 1))
    fi
  }
  expect "2.3" "$(cell_version <<< '2.3 (the tag of the official grammar repository)')" "the first word of a cell"
  expect "7.0.4" "$(cell_version <<< '7.0.4, validator_cli.jar sha256 43e2')" "a first word that ends in a comma"
  expect "23d3812cf17ac8459fc9f1a0068041b95bacd3e1" \
    "$(cell_commit <<< '23d3812cf17ac8459fc9f1a0068041b95bacd3e1 (the commit of IHTSDO/SNOMEDCT-Languages)')" "the commit of a cell"
  expect "1.10.0" "$(cell_suite_version <<< 'HL7/fhir-tx-ecosystem-ig at f158ade (test cases 1.10.0), tests/ only')" "the suite version of a cell"
  expect "snapshot builds are not releases" "$(cell_held <<< '6.0.0-ballot5 (the R6 core package), held: snapshot builds are not releases')" "the reason of a held pin"
  expect "" "$(cell_held <<< '49.0.0 (the release tag)')" "a pin with no reason"
  expect "5.4.2" "$(tag_version v5.4.2)" "a v-prefixed tag"
  expect "version_133" "$(tag_version version_133)" "a tag that is no version"
  expect "1.10.0" "$(registry_latest <<< '{"dist-tags":{"latest":"1.10.0"},"versions":{"1.9.3":{},"1.10.0":{}}}')" "the latest tag of a package"
  if got="$(registry_latest <<< '{"versions":{"1.0.0":{}}}' 2>/dev/null)"; then
    expect "a failure" "$got" "a package with no latest tag"
  fi
  expect "155.0.8059.84" "$(chrome_stable <<< '{"channels":{"Stable":{"version":"155.0.8059.84"},"Beta":{"version":"156.0.1.2"}}}')" "the Stable channel of a feed"
  if got="$(chrome_stable <<< 'not json' 2>/dev/null)"; then
    expect "a failure" "$got" "a feed that is no JSON"
  fi
  got="$(matrix_cell "$CHROME_LABEL" | cell_version)"
  case "$got" in
    [0-9]*.[0-9]*.[0-9]*.[0-9]*) ;;
    *) expect "a four-part version" "$got" "the Chrome for Testing row of $MATRIX" ;;
  esac
  for label in "$SUITE_LABEL" "$R6_LABEL"; do
    [[ -n "$(matrix_cell "$label")" ]] || expect "a row" "" "the $label row of $MATRIX"
  done
  while IFS=$'\t' read -r label _; do
    [[ -n "$(matrix_cell "$label")" ]] || expect "a row" "" "the $label row of $MATRIX"
  done <<< "$WATCHED_COMMITS"
  while IFS=$'\t' read -r label _; do
    [[ -n "$(matrix_cell "$label")" ]] || expect "a row" "" "the $label row of $MATRIX"
  done <<< "$WATCHED_RELEASES"
  if [[ "$failures" -gt 0 ]]; then
    echo "pin-freshness: self-test: $failures check(s) failed" >&2
    exit 1
  fi
  echo "pin-freshness: self-test passed"
}

case "${1:-}" in
  "") ;;
  --self-test)
    self_test
    exit 0
    ;;
  *)
    echo "usage: scripts/checks/pin-freshness.sh [--self-test]" >&2
    exit 2
    ;;
esac

while IFS=$'\t' read -r label repo; do
  [[ -n "$label" ]] || continue
  cell="$(matrix_cell "$label")"
  pinned="$(cell_version <<< "$cell")"
  if [[ -z "$pinned" ]]; then
    unreadable_pin "$label" "no pin row in $MATRIX"
    continue
  fi
  if ! tag="$(gh api "repos/$repo/releases/latest" --jq '.tag_name' 2>&1)"; then
    unreadable_pin "$label" "could not read the newest release of $repo ($tag)"
    continue
  fi
  report "$label" "$(tag_version "$pinned")" "$(tag_version "$tag")" "https://github.com/$repo/releases/tag/$tag" "$(cell_held <<< "$cell")"
done <<< "$WATCHED_RELEASES"

while IFS=$'\t' read -r label repo branch; do
  [[ -n "$label" ]] || continue
  cell="$(matrix_cell "$label")"
  pinned="$(cell_commit <<< "$cell" || true)"
  if [[ -z "$pinned" ]]; then
    unreadable_pin "$label" "no commit pin in $MATRIX"
    continue
  fi
  if ! head="$(gh api "repos/$repo/commits/$branch" --jq '.sha' 2>&1)"; then
    unreadable_pin "$label" "could not read the head of $repo $branch ($head)"
    continue
  fi
  report "$label" "$pinned" "$head" "https://github.com/$repo/commits/$branch" "$(cell_held <<< "$cell")"
done <<< "$WATCHED_COMMITS"

cell="$(matrix_cell "$SUITE_LABEL")"
pinned="$(cell_suite_version <<< "$cell")"
if [[ -z "$pinned" ]]; then
  unreadable_pin "$SUITE_LABEL" "no (test cases X) version in $MATRIX"
elif ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$SUITE_PACKAGE" | registry_latest 2>&1)"; then
  unreadable_pin "$SUITE_LABEL" "could not read the latest version at $SUITE_PACKAGE ($latest)"
else
  report "$SUITE_LABEL" "$pinned" "$latest" "$SUITE_PACKAGE" "$(cell_held <<< "$cell")"
fi

cell="$(matrix_cell "$R6_LABEL")"
pinned="$(cell_version <<< "$cell")"
if [[ -z "$pinned" ]]; then
  unreadable_pin "$R6_LABEL" "no pin row in $MATRIX"
elif ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$R6_PACKAGE" | registry_latest 2>&1)"; then
  unreadable_pin "$R6_LABEL" "could not read the latest version at $R6_PACKAGE ($latest)"
else
  report "$R6_LABEL" "$pinned" "$latest" "$R6_PACKAGE" "$(cell_held <<< "$cell")"
fi

cell="$(matrix_cell "$CHROME_LABEL")"
pinned="$(cell_version <<< "$cell")"
if [[ -z "$pinned" ]]; then
  unreadable_pin "$CHROME_LABEL" "no pin row in $MATRIX"
elif ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$CHROME_FEED" | chrome_stable 2>&1)"; then
  unreadable_pin "$CHROME_LABEL" "could not read the newest Stable release from $CHROME_FEED ($latest)"
else
  report "$CHROME_LABEL" "$pinned" "$latest" "$CHROME_FEED" "$(cell_held <<< "$cell")"
fi

[[ "$unreadable" -eq 0 ]] || exit 2
exit "$stale"
