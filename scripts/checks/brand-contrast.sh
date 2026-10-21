#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The brand palette's contrast against the two grounds it is drawn on, measured
# from assets/brand/tokens.css and checked against what that file claims.
#
#   scripts/checks/brand-contrast.sh          # check the claims
#   scripts/checks/brand-contrast.sh --table  # print the table for the README
#
# WCAG 2.2 asks 4.5:1 for body text and 3:1 for a graphical object or large
# text (https://www.w3.org/TR/WCAG22/#contrast-minimum and #non-text-contrast).
# A token safe for the mark is not therefore safe for a label, which is the
# distinction this check exists to keep true: every token carries a `safe:`
# comment in tokens.css naming what it may be used for on which ground, and a
# comment that stops matching the measurement fails here.
#
# The ratio is the WCAG formula over relative luminance, computed in awk
# because the repository's tooling languages are bash and Rust.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly TOKENS=assets/brand/tokens.css

# The two grounds the palette is drawn on, from the same file.
LIGHT=$(awk -F'[:;]' '/--ferroterm-surface:/ {gsub(/ /,"",$2); print $2; exit}' "$TOKENS")
DARK=$(awk -F'[:;]' '/--ferroterm-tile:/ {gsub(/ /,"",$2); print $2; exit}' "$TOKENS")

if [[ -z "$LIGHT" || -z "$DARK" ]]; then
  echo "brand-contrast: $TOKENS names no --ferroterm-surface or --ferroterm-tile" >&2
  exit 1
fi

# Every core token: name, value, and the `safe:` claim its comment carries.
# Read into one string rather than an array, because `mapfile` is bash 4 and
# macOS ships bash 3.2.
ROWS=$(awk '
  /--ferroterm-[a-z0-9-]+:[[:space:]]*#/ {
    split($0, parts, ":")
    name = parts[1]; gsub(/[[:space:]]*--ferroterm-/, "", name); gsub(/[[:space:]]/, "", name)
    value = parts[2]; sub(/;.*/, "", value); gsub(/[[:space:]]/, "", value)
    claim = ""
    if (match($0, /safe:[^*]*/)) {
      claim = substr($0, RSTART + 5, RLENGTH - 5)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", claim)
    }
    print name "\t" value "\t" claim
  }
' "$TOKENS")

measure() {
  awk -v a="$1" -v b="$2" '
    function hexval(pair,   digits, i, c, n, v) {
      digits = "0123456789abcdef"; v = 0
      for (i = 1; i <= length(pair); i++) {
        c = tolower(substr(pair, i, 1)); n = index(digits, c) - 1; v = v * 16 + n
      }
      return v
    }
    function chan(v,   s) { s = v / 255; return (s <= 0.03928) ? s / 12.92 : ((s + 0.055) / 1.055) ^ 2.4 }
    function lum(hex) {
      return 0.2126 * chan(hexval(substr(hex, 2, 2))) \
           + 0.7152 * chan(hexval(substr(hex, 4, 2))) \
           + 0.0722 * chan(hexval(substr(hex, 6, 2)))
    }
    BEGIN {
      la = lum(a); lb = lum(b)
      hi = (la > lb) ? la : lb; lo = (la > lb) ? lb : la
      printf "%.2f\n", (hi + 0.05) / (lo + 0.05)
    }
  '
}

# What a measured ratio permits, as the words a `safe:` comment uses.
verdict() {
  awk -v r="$1" 'BEGIN { print (r >= 4.5) ? "text" : (r >= 3) ? "graphics" : "neither" }'
}

# A pipeline-free loop still runs in this shell, but the here-document below is
# read by a subshell in bash 3.2, so the two tallies come back through files.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
failures=0
table=""
while IFS="	" read -r name value claim; do
  [ -n "$name" ] || continue
  light=$(measure "$value" "$LIGHT")
  dark=$(measure "$value" "$DARK")
  measured="light: $(verdict "$light"), dark: $(verdict "$dark")"
  table+="| \`--ferroterm-$name\` | \`$value\` | $light | $dark | $measured |"$'\n'
  if [[ -z "$claim" ]]; then
    echo "brand-contrast: --ferroterm-$name carries no 'safe:' comment; it measures $measured" >&2
    failures=$((failures + 1))
  elif [[ "$claim" != "$measured" ]]; then
    echo "brand-contrast: --ferroterm-$name claims '$claim' and measures '$measured'" >&2
    failures=$((failures + 1))
  fi
  printf '%s' "$table" > "$scratch/table"
  echo "$failures" > "$scratch/failures"
done <<EOF
$ROWS
EOF
table=$(cat "$scratch/table" 2>/dev/null || true)
failures=$(cat "$scratch/failures" 2>/dev/null || echo 0)

if [[ "${1:-}" == "--table" ]]; then
  printf '| Token | Value | On %s | On %s | Safe for |\n' "$LIGHT" "$DARK"
  printf '|---|---|---|---|---|\n'
  printf '%s' "$table"
  exit 0
fi

# The README carries the same figures, so a reader who never opens the CSS gets
# a table that was measured rather than remembered.
readonly DOC=assets/brand/README.md
while IFS=$'\t' read -r name value _; do
  light=$(measure "$value" "$LIGHT")
  dark=$(measure "$value" "$DARK")
  if ! grep -qF "| \`$value\` | $light | $dark |" "$DOC"; then
    echo "brand-contrast: $DOC does not carry $value at $light and $dark" >&2
    failures=$((failures + 1))
  fi
done <<EOF
$ROWS
EOF

if ((failures > 0)); then
  echo "brand-contrast: $failures claims do not match the measurement" >&2
  echo "  re-run with --table for the figures, and fix the claim rather than the threshold" >&2
  exit 1
fi

echo "brand-contrast: every token's claim matches what it measures ($(printf '%s\n' "$ROWS" | grep -c .) tokens)."
