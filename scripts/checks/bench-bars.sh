#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The latency bars, measured over the synthetic edition and compared to the
# claims in bench/bars.json.
#
#   scripts/checks/bench-bars.sh [--bars FILE] [--measurement-time SECONDS]
#                                [--sample-size N] [--run|--no-run]
#
# The benches build their own edition (ferroterm-testkit's generated one), so
# this runs anywhere, needs no licensed release, and gates CI. A bar is the
# claim the project makes, never a machine's measurement: it does not move to
# match a slower run. A breach is a regression to fix or a claim to withdraw,
# never a bar to raise (docs/architecture.md, issues #77 and #129).
set -euo pipefail

bars=bench/bars.json
measurement=3
samples=20
run=yes

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bars) bars=$2; shift 2 ;;
    --measurement-time) measurement=$2; shift 2 ;;
    --sample-size) samples=$2; shift 2 ;;
    --run) run=yes; shift ;;
    --no-run) run=no; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -f "$bars" ]]; then
  echo "bench-bars: no bars at $bars" >&2
  exit 2
fi

if [[ "$run" == yes ]]; then
  cargo bench -p fhir-terminology --bench operations -- \
    --warm-up-time 1 --measurement-time "$measurement" --sample-size "$samples"
  cargo bench -p ferroterm-server --bench http -- \
    --warm-up-time 1 --measurement-time "$measurement" --sample-size "$samples"
fi

target=${CARGO_TARGET_DIR:-target}
breached=0
measured=0

while IFS=$'\t' read -r bench max claim; do
  estimates="$target/criterion/$bench/new/estimates.json"
  if [[ ! -f "$estimates" ]]; then
    echo "bench-bars: $bench recorded nothing at $estimates" >&2
    breached=$((breached + 1))
    continue
  fi
  # Criterion records the median in nanoseconds.
  median=$(jq -r '.median.point_estimate / 1000' "$estimates")
  measured=$((measured + 1))
  if jq -e -n --argjson m "$median" --argjson b "$max" '$m > $b' >/dev/null; then
    breached=$((breached + 1))
    printf 'BREACH %-28s %8.1f us > %6.1f us  (%s)\n' "$bench" "$median" "$max" "$claim" >&2
  else
    printf 'ok     %-28s %8.1f us <= %6.1f us\n' "$bench" "$median" "$max"
  fi
done < <(jq -r '.bars[] | [.bench, .max_us, .claim] | @tsv' "$bars")

echo "bench-bars: $((measured - breached)) of $measured bars hold"
if [[ "$breached" -gt 0 ]]; then
  echo "bench-bars: a breach is a regression to fix or a claim to withdraw, never a bar to raise" >&2
  exit 1
fi
