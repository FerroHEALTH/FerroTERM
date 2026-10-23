#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Measures the console's release site bundle (app/ferroterm-console)
# and fails when its brotli-compressed WebAssembly is over its byte budget
# (no specification governs this: our own design).
#
# It reads the bundle scripts/release/console-site.sh --release writes, prints
# the WebAssembly and its JavaScript glue raw, gzip-compressed (gzip -9) and
# brotli-compressed (brotli -q 11) as a Markdown table, and appends the table
# to $GITHUB_STEP_SUMMARY when that is set. The budget is read from the budget
# table of the console's Leptos rule file, beside its reasoning, so the number
# lives in one place.
#
# Usage: scripts/checks/console-bundle.sh [pkg-dir]
#   pkg-dir defaults to target/site/pkg.
#
# Exit 0 = within the budget. Exit 1 = over the budget, a bundle file missing,
# a compressor missing, or the budget unreadable. Exit 2 = a usage error.
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rule="$root/.claude/rules/leptos-ui.md"
name="ferroterm-console"

case "$#" in
  0) pkg="$root/target/site/pkg" ;;
  1) pkg="$1" ;;
  *)
    echo "usage: scripts/checks/console-bundle.sh [pkg-dir]" >&2
    exit 2
    ;;
esac

for tool in gzip brotli; do
  if ! command -v "$tool" > /dev/null; then
    echo "console-bundle: $tool is not on PATH; the bundle cannot be measured." >&2
    exit 1
  fi
done

for file in "$name.wasm" "$name.js"; do
  if [[ ! -s "$pkg/$file" ]]; then
    echo "console-bundle: $pkg/$file is missing; run scripts/release/console-site.sh --release first." >&2
    exit 1
  fi
done

# The one row of the budget table whose first cell names the gating measure.
budgets="$(awk -F'|' '
  $2 ~ /^[[:space:]]*WebAssembly, brotli-compressed[[:space:]]*$/ {
    value = $3
    sub(/^[[:space:]]*/, "", value)
    sub(/[[:space:]]*bytes.*$/, "", value)
    print value
  }
' "$rule")"
if ! [[ "$budgets" =~ ^[0-9]+$ ]]; then
  echo "console-bundle: $rule has no single \"| WebAssembly, brotli-compressed | N bytes |\" budget row." >&2
  exit 1
fi
budget="$budgets"

raw() {
  local file="$1"
  wc -c < "$file" | tr -d '[:space:]'
}
gz() {
  local file="$1"
  gzip -9 -c "$file" | wc -c | tr -d '[:space:]'
}
br() {
  local file="$1"
  brotli -q 11 -c "$file" | wc -c | tr -d '[:space:]'
}

wasm="$pkg/$name.wasm"
js="$pkg/$name.js"
wasm_br="$(br "$wasm")"
percent=$((wasm_br * 100 / budget))

table="$(
  echo "### FerroTERM Console bundle"
  echo
  echo "| File | Raw | gzip -9 | brotli -q 11 |"
  echo "|---|---:|---:|---:|"
  echo "| \`$name.wasm\` | $(raw "$wasm") | $(gz "$wasm") | $wasm_br |"
  echo "| \`$name.js\` | $(raw "$js") | $(gz "$js") | $(br "$js") |"
  echo
  echo "Sizes in bytes. Budget: $budget bytes of brotli-compressed WebAssembly; this build is $wasm_br bytes, $percent% of it."
)"

echo "$table"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  echo "$table" >> "$GITHUB_STEP_SUMMARY"
fi

if [[ "$wasm_br" -gt "$budget" ]]; then
  echo "::error file=.claude/rules/leptos-ui.md::The brotli-compressed WebAssembly is $wasm_br bytes, over the $budget-byte budget of .claude/rules/leptos-ui.md § Bundle. Shrink the bundle, or raise the budget there with the reason." >&2
  exit 1
fi
echo "console-bundle: $wasm_br of $budget bytes, within the budget"
