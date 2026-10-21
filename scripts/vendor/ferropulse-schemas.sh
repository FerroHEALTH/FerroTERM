#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferropulse-schemas.sh
#
# Vendors FerroPULSE's report envelope v2, the machine-readable half of the
# usage report contract: the JSON Schema and its valid and invalid examples
# (.claude/rules/vendored-inputs.md). It reads the pin from docs/VERSIONS.md
# (the FerroPULSE row, a commit), copies the files verbatim into
# app/ferroterm-server/tests/fixtures/ferropulse/, and writes a PROVENANCE.md
# beside them. The server's integration tests validate every report it renders
# against the schema. FerroPULSE owns the contract; never hand-edit the copy,
# move the pin and re-run.
#
# The repository is private, so the fetch goes through the authenticated GitHub
# API (`gh api`, raw media type).
#
# Usage: scripts/vendor/ferropulse-schemas.sh
# Requires: gh (authenticated with read access to FerroHEALTH/FerroPULSE).

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

repo="FerroHEALTH/FerroPULSE"
dest="app/ferroterm-server/tests/fixtures/ferropulse"
pins="docs/VERSIONS.md"

die() { printf 'ferropulse-schemas: %s\n' "$*" >&2; exit 1; }
command -v gh >/dev/null 2>&1 || die "missing required tool: gh"

# The pin: the first word of the second cell of the FerroPULSE row.
sha="$(awk -F'|' '$2 ~ /^[[:space:]]*FerroPULSE report v2[[:space:]]*$/ { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1]; exit }' "$pins")"
[[ -n "$sha" ]] || die "no FerroPULSE report v2 row in $pins"
case "$sha" in
  *[!0-9a-f]* | "") die "the FerroPULSE pin is not a commit: '$sha'" ;;
  *) ;;
esac
[[ "${#sha}" -eq 40 ]] || die "the FerroPULSE pin is not a 40-character commit: '$sha'"

fetch() {
  local path="$1"
  mkdir -p "$(dirname "$dest/$path")"
  gh api -H 'Accept: application/vnd.github.raw' "repos/$repo/contents/$path?ref=$sha" >"$dest/$path"
}

# The files of one directory at the pin, by name.
list() {
  local directory="$1"
  gh api "repos/$repo/contents/$directory?ref=$sha" --jq '.[] | select(.type == "file") | .name'
}

echo "== FerroPULSE report v2 ($sha)"
rm -rf "$dest"
fetch schemas/report-envelope.v2.json
for validity in valid invalid; do
  dir="schemas/examples/report-envelope.v2/$validity"
  while IFS= read -r name; do
    fetch "$dir/$name"
  done < <(list "$dir")
done

cat >"$dest/PROVENANCE.md" <<PROV
# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/$repo> (private; Cadasto B.V., the Licensor of
  this repository too)
- Commit: $sha
- Fetched: $(date -u +%Y-%m-%d) by \`scripts/vendor/ferropulse-schemas.sh\`
- Licence: upstream declares \`LicenseRef-Proprietary\`; Cadasto B.V. holds the
  copyright of both repositories and publishes this copy under this
  repository's BUSL-1.1
- Contents: \`schemas/report-envelope.v2.json\` (JSON Schema 2020-12) and
  \`schemas/examples/report-envelope.v2/\` (the contract's own valid and
  invalid examples), copied verbatim

The usage report tests in \`app/ferroterm-server/tests/it/usage_report.rs\`
check the validator against the examples, then validate every report the
server renders against the schema. Never hand-edit these files; change the pin
in \`docs/VERSIONS.md\` and re-run the script.
PROV
echo "vendored $(find "$dest/schemas" -type f | wc -l | tr -d ' ') files under $dest"
