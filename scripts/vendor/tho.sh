#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Fetches the HL7 Terminology (THO) resources the server serves from, verbatim,
# into crates/fhir-terminology/data/tho/, from the package version and digest
# docs/VERSIONS.md pins, and rewrites the provenance stamp. Re-run to refresh;
# commit the result.
#
#   scripts/vendor/tho.sh
#
# Source: the hl7.terminology.r4 package on the FHIR package registry, under
# CC0 1.0 (the package's own `license`).
set -euo pipefail

cd "$(dirname "$0")/../.."
data=crates/fhir-terminology/data/tho
mkdir -p "$data"

# The pin of the docs/VERSIONS.md row named $1: the cell's first two words,
# the version and the sha256 of the package tarball.
row=$(awk -F'|' '{ label = $2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", label) }
  label == "HL7 Terminology (THO)" { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1], w[3]; exit }' docs/VERSIONS.md)
read -r version digest <<<"$row"
if [[ -z "${version:-}" || -z "${digest:-}" ]]; then
  echo "tho: docs/VERSIONS.md has no HL7 Terminology (THO) pin" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -r "$work"' EXIT
curl --proto '=https' --tlsv1.2 -sSL -o "$work/package.tgz" "https://packages.fhir.org/hl7.terminology.r4/$version"
echo "$digest  $work/package.tgz" | shasum -a 256 -c - >/dev/null
tar -xzf "$work/package.tgz" -C "$work" package/ValueSet-Languages.json package/package.json
[[ "$(jq -r .version "$work/package/package.json")" == "$version" ]] || {
  echo "tho: the package is not version $version" >&2
  exit 1
}
cp "$work/package/ValueSet-Languages.json" "$data/ValueSet-Languages.json"

cat > "$data/PROVENANCE.md" <<PROV
# Provenance: HL7 Terminology (THO)

Vendored verbatim; never edit a file here, re-run \`scripts/vendor/tho.sh\`.

- File: \`ValueSet-Languages.json\`, the common languages value set
  (\`http://terminology.hl7.org/ValueSet/Languages\`), which an expansion of
  all of \`urn:ietf:bcp:47\` returns, marked unclosed (the terminology
  ecosystem IG, \`requirements.md\`, IETF Language Codes)
- Source: \`hl7.terminology.r4\` $version, <https://packages.fhir.org/hl7.terminology.r4/$version>
- Package sha256: $digest
- Fetched: $(date -u +%Y-%m-%d)
- License: CC0 1.0 (the package's \`license\`)
PROV
echo "tho: hl7.terminology.r4 $version vendored into $data"
