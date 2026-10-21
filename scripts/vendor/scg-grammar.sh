#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/scg-grammar.sh
#
# Vendors the official SNOMED CT Compositional Grammar syntax and its example
# corpus (.claude/rules/vendored-inputs.md). It reads the SCG pin from
# docs/VERSIONS.md (the commit of the IHTSDO SNOMEDCT-Languages repository),
# downloads that commit's archive, copies the normative ABNF, the examples,
# the licence, and the README verbatim into crates/sct-scg/vendor/, and writes
# a PROVENANCE.md beside them. Re-running with an unchanged pin reproduces the
# same tree (only the fetch date in PROVENANCE.md moves).
#
# The repository publishes no tags, so the pin is a commit.
#
# Usage: scripts/vendor/scg-grammar.sh
# Requires: curl, tar.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

repo="https://github.com/IHTSDO/SNOMEDCT-Languages"
dest="crates/sct-scg/vendor"
pins="docs/VERSIONS.md"

die() { printf 'scg-grammar: %s\n' "$*" >&2; exit 1; }
for tool in curl tar; do
  command -v "$tool" >/dev/null 2>&1 || die "missing required tool: $tool"
done

# The pin: the first word of the second cell of the SCG row.
sha="$(awk -F'|' '$2 ~ /^[[:space:]]*SCG[[:space:]]*$/ { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1]; exit }' "$pins")"
[[ -n "$sha" ]] || die "no SCG row in $pins"
case "$sha" in
  *[!0-9a-f]* | "") die "SCG pin is not a commit: '$sha'" ;;
esac
[[ "${#sha}" -eq 40 ]] || die "SCG pin is not a 40-character commit: '$sha'"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "== SCG ($sha)"
curl --proto '=https' --tlsv1.2 -sfL "$repo/archive/$sha.tar.gz" -o "$tmp/scg.tar.gz"
tar -xzf "$tmp/scg.tar.gz" -C "$tmp"
src="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n1)"
grammar="$src/SnomedCTCompositionalGrammar"
[[ -d "$grammar" ]] || die "the archive carries no SnomedCTCompositionalGrammar directory"

rm -rf "$dest"
mkdir -p "$dest/syntax" "$dest/examples"
cp "$grammar/CG Syntax/"*.txt "$dest/syntax/"
# The example file names carry spaces and parentheses; the corpus is read by a
# test that globs the directory, so each file keeps its content and takes a
# name a glob and a manifest can carry.
for file in "$grammar/CG Examples/"*.txt; do
  name="$(basename "$file" .txt)"
  name="${name#CGv2 example (}"
  name="${name%)}"
  cp "$file" "$dest/examples/$name.txt"
done
cp "$src/LICENSE.md" "$src/README.md" "$dest/"
cat > "$dest/PROVENANCE.md" <<PROV
# Provenance: the SNOMED CT Compositional Grammar syntax

- Source: <$repo>
- Commit: $sha
- Fetched: $(date -u +%Y-%m-%d) by \`scripts/vendor/scg-grammar.sh\`
- Licence: Apache License 2.0 (\`LICENSE.md\`, vendored verbatim)
- Contents: \`syntax/\` (the normative ABNF) and \`examples/\` (the valid
  example corpus), copied verbatim from \`SnomedCTCompositionalGrammar/\`;
  \`README.md\`. Each example keeps its content and is renamed from
  \`CGv2 example (<name>).txt\` to \`<name>.txt\` so a glob and a test can
  carry it.

The parser in \`crates/sct-scg\` mirrors \`syntax/\` rule for rule; the corpus
is the parse-conformance fixture. Never hand-edit these files; change the pin
in \`docs/VERSIONS.md\` and re-run the script.
PROV
echo "vendored $(find "$dest/examples" -type f | wc -l | tr -d ' ') examples under $dest"
