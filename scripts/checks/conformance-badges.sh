#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The conformance badges: one shields.io endpoint JSON per served FHIR version,
# derived from the committed pass lists of the HL7 terminology ecosystem suite
# (conformance/tx-ecosystem/, the lists CI gates on every push) and the suite
# total the suite script records. The numbers are never typed by hand: a badge
# says what the pass list says. The docs lane writes them into the site under
# /conformance/, and the README badges read them through
# https://img.shields.io/endpoint (https://shields.io/badges/endpoint-badge).
#
#   conformance-badges.sh <out-dir>
set -euo pipefail
cd "$(dirname "$0")/../.."

out="${1:?usage: conformance-badges.sh <out-dir>}"
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
  printf '{"schemaVersion":1,"label":"%s","message":"%s / %s","color":"%s"}\n' \
    "$label" "$passed" "$total" "$colour" > "$out/$version.json"
  echo "conformance-badges: $version $passed / $total ($colour)"
done
