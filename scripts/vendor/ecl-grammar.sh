#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ecl-grammar.sh
#
# Vendors the official SNOMED CT Expression Constraint Language grammar and
# its example corpus (.claude/rules/vendored-inputs.md). It reads the ECL pin
# from docs/VERSIONS.md (the tag of the IHTSDO grammar repository), downloads
# that tag's archive, copies the syntax files, the examples, the licence, and
# the README verbatim into crates/sct-ecl/vendor/, and writes a
# PROVENANCE.md beside them. Re-running with an unchanged pin reproduces the
# same tree (only the fetch date in PROVENANCE.md moves).
#
# Usage: scripts/vendor/ecl-grammar.sh
# Requires: curl, git, tar.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

repo="https://github.com/IHTSDO/snomed-expression-constraint-language"
dest="crates/sct-ecl/vendor"
pins="docs/VERSIONS.md"

die() { printf 'ecl-grammar: %s\n' "$*" >&2; exit 1; }
for tool in curl git tar; do
  command -v "$tool" >/dev/null 2>&1 || die "missing required tool: $tool"
done

# The pin: the first word of the second cell of the ECL row.
tag="$(awk -F'|' '$2 ~ /^[[:space:]]*ECL[[:space:]]*$/ { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1]; exit }' "$pins")"
[[ -n "$tag" ]] || die "no ECL row in $pins"
case "$tag" in
  *[!0-9.]*) die "ECL pin is not a plain tag: '$tag'" ;;
  *) ;;
esac

sha="$(git ls-remote --tags "$repo" "refs/tags/$tag" | awk '{ print $1; exit }')"
[[ -n "$sha" ]] || die "tag $tag not found at $repo"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "== ECL $tag ($sha)"
curl --proto '=https' --tlsv1.2 -sfL "$repo/archive/$sha.tar.gz" -o "$tmp/ecl.tar.gz"
tar -xzf "$tmp/ecl.tar.gz" -C "$tmp"
src="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n1)"

rm -rf "$dest"
mkdir -p "$dest"
cp -R "$src/syntax" "$dest/syntax"
cp -R "$src/examples" "$dest/examples"
cp "$src/LICENSE.md" "$src/README.md" "$dest/"
cat > "$dest/PROVENANCE.md" <<PROV
# Provenance: the SNOMED CT Expression Constraint Language grammar

- Source: <$repo>
- Tag: $tag
- Commit: $sha
- Fetched: $(date -u +%Y-%m-%d) by \`scripts/vendor/ecl-grammar.sh\`
- Licence: Apache License 2.0 (\`LICENSE.md\`, vendored verbatim)
- Contents: \`syntax/\` (the ANTLR grammar \`ECL.g4\` and the ABNF forms) and
  \`examples/\` (the valid example corpus), copied verbatim; \`README.md\`.

The parser in \`crates/sct-ecl\` mirrors \`syntax/ECL.g4\` rule for
rule; the corpus is the parse-conformance fixture. Never hand-edit these
files; change the pin in \`docs/VERSIONS.md\` and re-run the script.
PROV
echo "vendored $(find "$dest/examples" -type f | wc -l | tr -d ' ') examples under $dest"
