#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# assemble.sh: the site as GitHub Pages serves it, from the landing page and the
# book, into one directory.
#
#   scripts/site/assemble.sh OUT
#
# The landing page is served at the site root and the mdBook under /docs/
# (book.toml sets site-url = "/docs/"). The brand directory reaches the site
# root, where the pages and the book's theme address the marks and the
# favicons. Two markers of the landing pages are rendered on the way, and a
# marker nothing renders fails the assembly:
#
#   * {{version:ferroterm}} becomes the [workspace.package] version of the
#     root Cargo.toml, so the hero badge always names the release main
#     carries.
#   * {{hash:style.css}} becomes the first twelve hex digits of the
#     stylesheet's sha256, so a browser that cached an older stylesheet
#     fetches the new one with the new page.
#
# The book is rendered into a temporary directory, so the source tree stays
# untouched. No specification governs the assembly: our own design.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly OUT="${1:?usage: $0 OUT}"
readonly LANDING=website/landing

# product_version: the [workspace.package] version of the root Cargo.toml.
product_version() {
  awk '/^\[workspace\.package\]/ { p = 1; next } /^\[/ { p = 0 }
    p && $1 == "version" { if (match($0, /"[^"]*"/)) { print substr($0, RSTART + 1, RLENGTH - 2); exit } }' Cargo.toml
}

book="$(mktemp -d)"
trap 'rm -rf "$book"' EXIT

mdbook build website/book --dest-dir "$book"
rm -rf "${OUT:?}"
mkdir -p "$OUT/docs" "$OUT/assets"
cp -R "$LANDING"/. "$OUT/"
cp -R "$book"/. "$OUT/docs/"
cp -R assets/brand "$OUT/assets/"

version="$(product_version)"
if [[ -z "$version" ]]; then
  echo "assemble: Cargo.toml has no [workspace.package] version" >&2
  exit 1
fi
style_hash="$(shasum -a 256 "$OUT/style.css" | cut -c1-12)"
while IFS= read -r page; do
  VERSION="$version" STYLE_HASH="$style_hash" perl -pi -e \
    's/\{\{version:ferroterm\}\}/$ENV{VERSION}/g; s/\{\{hash:style\.css\}\}/$ENV{STYLE_HASH}/g' "$page"
  if grep -q '{{' "$page"; then
    echo "assemble: $page keeps a marker nothing renders" >&2
    exit 1
  fi
done < <(find "$OUT" -path "$OUT/docs" -prune -o -name '*.html' -print)

echo "assemble: site at $OUT"
