#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# bench-table.sh: every public speed and footprint figure, rendered from a
# committed set of benchmark records, never typed by hand.
#
#   scripts/checks/bench-table.sh render <target> [DIR]  # print one target's rendering
#   scripts/checks/bench-table.sh check [DIR]            # fail when any target's file differs from its rendering
#
# The targets and the files that carry them between `<!-- <marker>:begin -->`
# and `<!-- <marker>:end -->`:
#   readme      README.md                                  (marker bench-table)
#   book        website/book/src/evaluate/benchmarks.md   (marker bench-table)
#   figures     website/landing/index.html        (marker bench-figures)
#   benchmarks  website/landing/benchmarks.html   (marker bench-table)
#
# A record set is one directory under bench/records, named by the date and the
# machine (bench/records/2026-09-04-apple-m2), holding the records one
# ferroterm-bench run wrote (copied there by hand from bench/results). Every
# record in a set must come from the same machine and the same FerroTERM
# version; the newest record per system is rendered.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly RECORDS=bench/records

target_file() {
  case "$1" in
    readme) echo README.md ;;
    book) echo website/book/src/evaluate/benchmarks.md ;;
    figures) echo website/landing/index.html ;;
    benchmarks) echo website/landing/benchmarks.html ;;
    *) echo "bench-table: unknown target $1" >&2; return 2 ;;
  esac
}

target_marker() {
  case "$1" in
    figures) echo bench-figures ;;
    *) echo bench-table ;;
  esac
}

newest_set() {
  local dir
  dir="$(find "$RECORDS" -mindepth 1 -maxdepth 1 -type d | sort | tail -1)"
  if [[ -z "$dir" ]]; then
    echo "bench-table: no record set under $RECORDS" >&2
    return 1
  fi
  printf '%s\n' "$dir"
}

render() {
  local target="$1" dir="${2:-$(newest_set)}"
  local files
  files="$(find "$dir" -maxdepth 1 -name '*.json' | sort)"
  if [[ -z "$files" ]]; then
    echo "bench-table: no records under $dir" >&2
    return 1
  fi
  # shellcheck disable=SC2086 # the file list is newline-separated paths
  jq -r -n --arg target "$target" -f scripts/checks/bench-table.jq $files
}

check_one() {
  local target="$1" dir="$2" file marker rendered current
  file="$(target_file "$target")"
  marker="$(target_marker "$target")"
  rendered="$(render "$target" "$dir")" || { echo "bench-table: rendering $target failed" >&2; return 1; }
  current="$(awk -v m="$marker" '$0 ~ "<!-- " m ":begin -->" {p=1; next} $0 ~ "<!-- " m ":end -->" {p=0} p' "$file")"
  if [[ "$rendered" != "$current" ]]; then
    echo "bench-table: $file differs from the records; run: scripts/checks/bench-table.sh render $target" >&2
    diff <(printf '%s\n' "$current") <(printf '%s\n' "$rendered") >&2 || true
    return 1
  fi
}

check() {
  local dir="${1:-$(newest_set)}" status=0 target
  for target in readme book figures benchmarks; do
    check_one "$target" "$dir" || status=1
  done
  [[ "$status" -eq 0 ]] && echo "bench-table: OK (the README, the book, the landing page, and the benchmarks page match the records)."
  return "$status"
}

case "${1:-}" in
  render) render "${2:?target}" "${3:-}" ;;
  check) check "${2:-}" ;;
  *) echo "usage: $0 render <readme|book|figures|benchmarks> [DIR] | check [DIR]" >&2; exit 2 ;;
esac
