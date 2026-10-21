#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every project extension the server writes resolves on the website. FHIR asks
# that an extension's URL be the canonical of the StructureDefinition that
# defines it, and that consumers can read that definition, preferably at the
# URL itself (https://hl7.org/fhir/R4B/extensibility.html#defining).
#
#   scripts/checks/extension-definitions.sh
#
# Every https://ferroterm.eu/fhir/StructureDefinition/<name> URL in the Rust
# sources under crates/ and app/ must have, under
# website/landing/fhir/StructureDefinition, a <name>.json StructureDefinition
# whose url is that canonical, and a <name>.html page. The site serves the
# landing tree at its root (scripts/site/assemble.sh), and GitHub Pages
# answers the extensionless canonical with the .html page, which links the
# JSON beside it. Exit 0 when every canonical has both.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly PREFIX="https://ferroterm.eu/fhir/StructureDefinition/"
readonly SITE="website/landing/fhir/StructureDefinition"

command -v jq >/dev/null 2>&1 ||
  { echo "extension-definitions: jq is not installed" >&2; exit 1; }

# The canonicals the sources name, one per line, deduplicated. The test trees
# are left out: a test repeats a canonical the source emits, and a canonical
# only a test names is not on the wire.
canonicals="$(grep -rhoE --include='*.rs' --exclude-dir=tests \
  "${PREFIX//./\\.}[A-Za-z0-9._-]+" crates app | sort -u || true)"

if [[ -z "$canonicals" ]]; then
  echo "extension-definitions: no source names a project extension; nothing to check."
  exit 0
fi

failed=0
count=0
while IFS= read -r canonical; do
  count=$((count + 1))
  name="${canonical#"$PREFIX"}"
  json="$SITE/$name.json"
  page="$SITE/$name.html"
  if [[ ! -f "$json" ]]; then
    echo "extension-definitions: $canonical is emitted, but $json does not exist" >&2
    failed=1
    continue
  fi
  if ! jq -e --arg url "$canonical" \
    '.resourceType == "StructureDefinition" and .url == $url and .type == "Extension"' \
    "$json" >/dev/null 2>&1; then
    echo "extension-definitions: $json is not an Extension StructureDefinition with url $canonical" >&2
    failed=1
  fi
  if [[ ! -f "$page" ]]; then
    echo "extension-definitions: $canonical is emitted, but $page does not exist" >&2
    failed=1
  fi
done <<< "$canonicals"

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
echo "extension-definitions: OK ($count canonicals, each defined under $SITE)."
