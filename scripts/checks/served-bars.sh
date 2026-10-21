#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The bar a served read holds across every code system, checked against the
# committed record set (#512, #304).
#
#   scripts/checks/served-bars.sh [--records DIR] [--bars FILE]
#
# A read costs a fixed amount plus the serialising of its answer, at a rate
# that is the same on every system, so the bar has that shape:
#
#   max_us = fixed_us + rate_ns_per_byte * answer_bytes / 1000
#
# A flat bar across systems would instead ask a 63 KB answer to serialise four
# times faster than a 1.5 KB one, which is a bar on how much a system has to
# say about a concept rather than on the server's speed.
#
# This reads records rather than running benches, because the claim is about
# what a real code system answers with, and the synthetic edition
# `bench-bars.sh` measures has no such answers. A record set taken before the
# harness recorded answer sizes is skipped loudly rather than passed silently.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

bars=bench/bars.json
records=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --records) records=$2; shift 2 ;;
    --bars) bars=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -f "$bars" ]]; then
  echo "served-bars: no bars at $bars" >&2
  exit 2
fi

# The newest committed record set, which is the one the published table renders
# from, unless one is named.
if [[ -z "$records" ]]; then
  records=$(find bench/records -mindepth 1 -maxdepth 1 -type d | sort | tail -1)
fi
if [[ -z "$records" || ! -d "$records" ]]; then
  echo "served-bars: no record set under bench/records" >&2
  exit 2
fi

fixed=$(jq -r '.served_bars[] | select(.operation == "lookup") | .fixed_us' "$bars")
rate=$(jq -r '.served_bars[] | select(.operation == "lookup") | .rate_ns_per_byte' "$bars")
if [[ -z "$fixed" || -z "$rate" ]]; then
  echo "served-bars: $bars declares no served bar for lookup" >&2
  exit 2
fi

echo "served-bars: $records, lookup bar = ${fixed} us + ${rate} ns/byte"

breached=0
checked=0
skipped=0
for record in "$records"/*.json; do
  [[ -f "$record" ]] || continue
  system=$(jq -r '.system' "$record")
  read -r bytes p50 < <(jq -r '
    (.latency[] | select(.[0] == "lookup") | .[1]) as $l
    | "\($l.answer_bytes // "none") \($l.p50_ms * 1000)"
  ' "$record" 2>/dev/null || echo "none 0")
  if [[ "$bytes" == "none" || -z "$bytes" ]]; then
    printf 'skip   %-40s the record carries no answer size\n' "$system"
    skipped=$((skipped + 1))
    continue
  fi
  checked=$((checked + 1))
  if awk -v f="$fixed" -v r="$rate" -v b="$bytes" -v p="$p50" '
      BEGIN { exit !(p > f + r * b / 1000) }
    '; then
    breached=$((breached + 1))
    awk -v s="$system" -v f="$fixed" -v r="$rate" -v b="$bytes" -v p="$p50" '
      BEGIN { printf "BREACH %-40s %8.1f us > %8.1f us for %d bytes\n", s, p, f + r * b / 1000, b }
    ' >&2
  else
    awk -v s="$system" -v f="$fixed" -v r="$rate" -v b="$bytes" -v p="$p50" '
      BEGIN { printf "ok     %-40s %8.1f us <= %8.1f us for %d bytes\n", s, p, f + r * b / 1000, b }
    '
  fi
done

if ((skipped > 0 && checked == 0)); then
  echo "served-bars: every record predates the answer size, so the bar was not checked" >&2
  echo "  take a fresh set with ferroterm-bench; the harness records it now" >&2
  exit 0
fi

echo "served-bars: $((checked - breached)) of $checked systems hold the bar"
if ((breached > 0)); then
  echo "served-bars: a breach is a regression to fix or a claim to withdraw, never a bar to raise" >&2
  exit 1
fi
