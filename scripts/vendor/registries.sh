#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Fetches the public registries the registry code systems are built from,
# verbatim, into crates/fhir-terminology/data/, and rewrites the
# provenance stamp. Re-run to refresh; commit the result.
#
#   scripts/vendor/registries.sh
#
# Sources (all freely redistributable):
#   IANA Language Subtag Registry (BCP 47) and the IANA Media Types registry
#   (BCP 13): IANA dedicates the protocol registries to the public domain
#   (https://www.iana.org/help/licensing-terms).
#   Unicode CLDR (ISO 3166-1 codes and English territory names): the Unicode
#   License v3 (https://github.com/unicode-org/cldr-json/blob/main/LICENSE).
set -euo pipefail

cd "$(dirname "$0")/../.."
data=crates/fhir-terminology/data
mkdir -p "$data/iana/media-types" "$data/cldr"

CLDR_REF=${CLDR_REF:-main}

# Every fetch refuses a protocol other than HTTPS and a TLS version below 1.2.
https_only=(--proto '=https' --tlsv1.2)

curl "${https_only[@]}" -sSL -o "$data/iana/language-subtag-registry" \
  https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry
for top in application audio font haptics image message model multipart text video; do
  curl "${https_only[@]}" -sSL -o "$data/iana/media-types/$top.csv" "https://www.iana.org/assignments/media-types/$top.csv"
done
curl "${https_only[@]}" -sSL -o "$data/cldr/territories.json" \
  "https://raw.githubusercontent.com/unicode-org/cldr-json/$CLDR_REF/cldr-json/cldr-localenames-full/main/en/territories.json"
curl "${https_only[@]}" -sSL -o "$data/cldr/codeMappings.json" \
  "https://raw.githubusercontent.com/unicode-org/cldr-json/$CLDR_REF/cldr-json/cldr-core/supplemental/codeMappings.json"
curl "${https_only[@]}" -sSL -o "$data/cldr/LICENSE" "https://raw.githubusercontent.com/unicode-org/cldr-json/$CLDR_REF/LICENSE"

mkdir -p "$data/ucum"
UCUM_REF=${UCUM_REF:-main}
curl "${https_only[@]}" -sSL -o "$data/ucum/ucum-essence.xml" "https://raw.githubusercontent.com/ucum-org/ucum/$UCUM_REF/ucum-essence.xml"
curl "${https_only[@]}" -sSL -o "$data/ucum/LICENSE.md" "https://raw.githubusercontent.com/ucum-org/ucum/$UCUM_REF/LICENSE.md"
ucum_version=$(grep -o 'ucum-essence" version="[0-9.]*"' "$data/ucum/ucum-essence.xml" | sed 's/.*version="//; s/"$//')
ucum_commit=$(curl "${https_only[@]}" -sSL "https://api.github.com/repos/ucum-org/ucum/commits/$UCUM_REF" | jq -r '.sha')

registry_date=$(sed -n 's/^File-Date: //p' "$data/iana/language-subtag-registry" | head -n1)
cldr_version=$(jq -r '.supplemental.version._cldrVersion' "$data/cldr/codeMappings.json")
cldr_commit=$(curl "${https_only[@]}" -sSL "https://api.github.com/repos/unicode-org/cldr-json/commits/$CLDR_REF" | jq -r '.sha')

today=$(date -u +%Y-%m-%d)

cat > "$data/PROVENANCE.md" <<PROV
# Provenance: registry data

Vendored verbatim as the input of the registry code systems (BCP 47, BCP 13,
ISO 3166); never edit a file here, re-run \`scripts/vendor/registries.sh\`.

## IANA Language Subtag Registry (BCP 47)

- File: \`iana/language-subtag-registry\` (record-jar, RFC 5646 §3.1)
- Source: <https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry>
- Registry File-Date: $registry_date
- Fetched: $today
- License: public domain (IANA protocol registries, <https://www.iana.org/help/licensing-terms>)

## IANA Media Types registry (BCP 13)

- Files: \`iana/media-types/<type>.csv\` for the ten top-level types
- Source: <https://www.iana.org/assignments/media-types/media-types.xhtml> (the per-type CSV exports)
- Fetched: $today
- License: public domain (IANA protocol registries, <https://www.iana.org/help/licensing-terms>)

## Unicode CLDR (ISO 3166-1)

- Files: \`cldr/codeMappings.json\` (alpha-2 to alpha-3 and numeric), \`cldr/territories.json\` (English names), \`cldr/LICENSE\`
- Source: <https://github.com/unicode-org/cldr-json>, ref \`$CLDR_REF\` at commit \`$cldr_commit\`
- CLDR version: $cldr_version
- Fetched: $today
- License: Unicode License v3 (\`cldr/LICENSE\`)

## UCUM essence

- Files: \`ucum/ucum-essence.xml\` (the unit definitions), \`ucum/LICENSE.md\`
- Source: <https://github.com/ucum-org/ucum>, ref \`$UCUM_REF\` at commit \`$ucum_commit\`
- UCUM version: $ucum_version
- Fetched: $today
- License: the UCUM licence (\`ucum/LICENSE.md\`), verbatim redistribution with the notice
PROV
echo "fetched: registry $registry_date, CLDR $cldr_version ($cldr_commit), UCUM $ucum_version ($ucum_commit)"
