#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The resident memory of a built artifact, structure by structure (#322).
#
#   scripts/bench/residency.sh artifacts/nl [repeats]
#
# Runs `ferroterm-residency` once per structure per repeat, in its own process
# each time, and reports the median of the repeats.
#
# One process per structure, because two structures in one process cannot be
# told apart: the allocator does not give a structure back its own arena.
#
# The figure is the PEAK resident set of the process, read from `/usr/bin/time`
# the way this repository already measures an ingest. A reading taken after the
# load instead decays while it is being taken, because the allocator returns
# pages when it chooses: measured that way, loading the hierarchy AND
# transposing it read lower than loading the hierarchy alone, which cannot be
# true. The structure is read through a buffered reader, so the peak is the
# structure and not a copy of the file beside it.
#
# Several repeats, because the first read of a file behaves differently from
# the ones after it.
#
# Build the binary first:
#   cargo build --release -p ferroterm-bench --bin ferroterm-residency
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly BINARY=target/release/ferroterm-residency
readonly STRUCTURES=(baseline hierarchy children text members attributes refsets identifiers store edition)

artifact=${1:-}
repeats=${2:-5}

if [[ -z "$artifact" ]]; then
  echo "usage: scripts/bench/residency.sh <artifact directory> [repeats]" >&2
  exit 2
fi
if [[ ! -x "$BINARY" ]]; then
  echo "residency: build it first: cargo build --release -p ferroterm-bench --bin ferroterm-residency" >&2
  exit 1
fi
if [[ ! -f "$artifact/manifest.json" ]]; then
  echo "residency: $artifact holds no manifest.json" >&2
  exit 1
fi

# The baseline is the cost of the process itself, which every other figure is
# reported against.
baseline=""

printf '%-12s %14s %14s %14s %7s\n' structure serialized 'peak resident' 'over baseline' factor
for structure in "${STRUCTURES[@]}"; do
  readings=""
  serialized=""
  for _ in $(seq 1 "$repeats"); do
    measured=$(/usr/bin/time -l "$BINARY" --artifact "$artifact" --structure "$structure" 2>&1 || true)
    printf '%s' "$measured" | grep -q '"structure"' || continue
    peak=$(printf '%s\n' "$measured" | awk '/maximum resident set size/ { print $1 }')
    [[ -n "$peak" ]] || continue
    readings+="$peak"$'\n'
    serialized=$(printf '%s\n' "$measured" | grep '"structure"' | sed -E 's/.*"serialized_bytes":(null|[0-9]+).*/\1/')
  done
  if [[ -z "$readings" ]]; then
    printf '%-12s %14s %14s %14s %7s\n' "$structure" "-" "unreadable" "-" "-"
    continue
  fi
  resident=$(printf '%s' "$readings" | grep -v '^$' | sort -n | awk '{ v[NR] = $1 } END { print v[int((NR + 1) / 2)] }')
  if [[ "$structure" == "baseline" ]]; then
    baseline=$resident
  fi
  awk -v s="$structure" -v ser="$serialized" -v res="$resident" -v base="${baseline:-0}" '
    function mb(bytes) { return sprintf("%.1f MB", bytes / 1048576) }
    BEGIN {
      over = res - base
      printf "%-12s %14s %14s %14s %7s\n", s,
        (ser == "null" || ser == "") ? "-" : mb(ser),
        mb(res),
        (s == "baseline") ? "-" : mb(over),
        (ser == "null" || ser == "" || ser + 0 < 1048576 || s == "baseline" || over <= 0) \
          ? "-" : sprintf("%.2fx", over / ser)
    }
  '
done
