#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The conformance badges: one shields.io endpoint JSON per served FHIR version,
# derived from the committed pass lists of the HL7 terminology ecosystem suite
# (conformance/tx-ecosystem/, the lists CI gates on every push) and the suite
# total the suite script records, and the EHDS badges over conformance/ehds/. The numbers are never typed by hand: a badge
# says what the pass list says. The docs lane writes them into the site under
# /conformance/, and the README badges read them through
# https://img.shields.io/endpoint (https://shields.io/badges/endpoint-badge).
# Every badge, and every static badge in the README, carries a logo from
# scripts/checks/badge-logos/: the EU emblem on EU blue for an EU act, else a
# pictogram. A badge without one fails.
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
  # Colour by share: grey below a quarter, orange to three quarters, green above.
  share=$((passed * 100 / total))
  if [[ "$share" -lt 25 ]]; then colour=lightgrey
  elif [[ "$share" -lt 75 ]]; then colour=orange
  else colour=brightgreen
  fi
  label="tx-ecosystem $suite $(printf '%s' "$version" | tr '[:lower:]' '[:upper:]')"
  jq -cn --arg label "$label" --arg message "$passed / $total" --arg color "$colour" \
    --argjson logo "$(badge_logo_fields shield-check)" \
    '{schemaVersion: 1, label: $label, message: $message, color: $color} + $logo' > "$out/$version.json"
  echo "conformance-badges: $version $passed / $total ($colour)"
done

# The EHDS badges, self-tested: each counts a committed corpus that a test or
# scripts/checks/ehds-matrix.sh holds true, and names the exact version it
# measures. A corpus that does not exist yet writes no badge.
badge() {
  local file="$1" label="$2" message="$3" colour="$4" logo="$5"
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

if [[ -n "$self_test" ]]; then
  for claim in "FHIR R4 certified" "EHDS compliant" "Conformant terminology"; do
    if (badge probe.json "$claim" "1/1" blue eu) 2>/dev/null; then
      echo "conformance-badges: self-test: '$claim' was written" >&2
      exit 1
    fi
  done
  badge probe.json "EEHRxF draft 2026-11-23 Art 6 code systems" "1/16 loaders, self-tested" blue eu > /dev/null
  badge_logo_check "$out"/*.json || exit 1
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
  echo "conformance-badges: self-test OK"
  exit 0
fi

count_rows() {
  grep -cvE '^(#|[[:space:]]*$)' "$1" || true
}

mvc=conformance/ehds/mvc-synthetic.tsv
if [[ -f "$mvc" ]]; then
  listed="$(count_rows "$mvc")"
  badge ehds-mvc.json "MyHealth@EU MVC 10.1.0 (CI) import" \
    "$listed/$listed value sets, synthetic fixture, self-tested" blue eu
fi

matrix=conformance/ehds/art6-code-systems.tsv
if [[ -f "$matrix" ]]; then
  systems="$(count_rows "$matrix")"
  proven="$(grep -vE '^(#|[[:space:]]*$)' "$matrix" | awk -F'\t' '$3 != "-"' | wc -l | tr -d ' ')"
  badge ehds-art6.json "EEHRxF draft 2026-11-23 Art 6 code systems" \
    "$proven/$systems loaders, self-tested" blue eu
fi

badge_logo_check "$out"/*.json
