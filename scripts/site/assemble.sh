#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# assemble.sh: the site as GitHub Pages serves it, from the landing page and the
# book, into one directory.
#
#   scripts/site/assemble.sh OUT
#
# The landing page is served at the site root and the mdBook under /docs/
# (book.toml sets site-url = "/docs/").
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly OUT="${1:?usage: $0 OUT}"
readonly LANDING=website/landing

mdbook build website/book
rm -rf "$OUT"
mkdir -p "$OUT/docs"
cp -R "$LANDING"/. "$OUT/"
cp -R website/book/book/. "$OUT/docs/"

echo "assemble: site at $OUT"
