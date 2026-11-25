#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The post-quantum inventory (conformance/post-quantum.tsv): its shape, read
# by the shared badge helper's badge_post_quantum, every repository path a
# row's location names, the table of docs/post-quantum.md rewritten from its
# rows, and the README badge that reads the count conformance-badges.sh
# publishes. No specification governs the inventory's form: our own design.
#
#   scripts/checks/post-quantum.sh               rewrite the table
#   scripts/checks/post-quantum.sh --check       fail on drift (CI)
#   scripts/checks/post-quantum.sh --self-test   the helper's cases
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/badge-logos/logo.sh
source scripts/checks/badge-logos/logo.sh

tsv=conformance/post-quantum.tsv
page=docs/post-quantum.md
badge_url='https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fpost-quantum.json'

rows() {
  grep -vE '^(#|$)' "$1" | tail -n +2
}

# The markdown table of the inventory.
table() {
  printf '| Use | Algorithm | Safe | Post-quantum standard | Location |\n|---|---|---|---|---|\n'
  rows "$tsv" | awk -F'\t' '{ printf "| %s | %s | %s | %s | `%s` |\n", $1, $2, $3, $4, $5 }'
}

# The page with its table rewritten between the two markers.
rendered() {
  PQ_TABLE="$(table)" awk '
    /^<!-- post-quantum table -->$/ { print; print ENVIRON["PQ_TABLE"]; skip = 1; next }
    /^<!-- end of the post-quantum table -->$/ { skip = 0 }
    !skip { print }
  ' "$page"
}

# Fails for each repository path a location names that the tree lacks; an
# item without a slash names a family crate.
locations() {
  local failures=0 location item path items
  while IFS=$'\t' read -r _ _ _ _ location; do
    IFS=',' read -ra items <<< "$location"
    for item in "${items[@]}"; do
      path="$(awk '{ print $1 }' <<< "$item")"
      [[ "$path" == */* ]] || continue
      if [[ ! -e "$path" ]]; then
        echo "post-quantum: $tsv names $path, which the tree does not hold" >&2
        failures=$((failures + 1))
      fi
    done
  done < <(rows "$tsv")
  [[ "$failures" -eq 0 ]]
}

case "${1:-}" in
  --self-test)
    badge_post_quantum_self_test
    ;;
  --check)
    badge_post_quantum "$tsv" > /dev/null
    locations
    if ! diff -u "$page" <(rendered) > /dev/null; then
      echo "post-quantum: the table of $page disagrees with $tsv; run scripts/checks/post-quantum.sh" >&2
      exit 1
    fi
    if ! grep -qF "$badge_url" README.md; then
      echo "post-quantum: README.md carries no post-quantum badge" >&2
      exit 1
    fi
    if grep -F "$badge_url" README.md | grep -qiE 'certified|compliant|conformant'; then
      echo "post-quantum: the README's post-quantum badge claims a certification" >&2
      exit 1
    fi
    echo "post-quantum: $(badge_post_quantum "$tsv" | jq -r .message) safe uses, table and locations agree"
    ;;
  '')
    badge_post_quantum "$tsv" > /dev/null
    locations
    rendered > "$page.new"
    mv "$page.new" "$page"
    ;;
  *)
    echo "usage: post-quantum.sh [--check | --self-test]" >&2
    exit 2
    ;;
esac
