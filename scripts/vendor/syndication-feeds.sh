#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/syndication-feeds.sh
#
# Vendors the public terminology syndication listings the
# `terminology-syndication` parser is tested against
# (.claude/rules/vendored-inputs.md). Each listing is fetched
# verbatim into crates/terminology-syndication/vendor/feeds/ and a
# PROVENANCE.md is written beside them with the URL, the fetch date, the
# SHA-256, and the byte size.
#
# Run it by hand to refresh the corpus and commit the result. It is never run
# in CI: an operator republishes its listing on its own cadence, so an
# automatic refresh would rewrite the fixtures the corpus test pins.
#
# Usage: scripts/vendor/syndication-feeds.sh
# Requires: curl, sha256sum or shasum.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

dest="crates/terminology-syndication/vendor/feeds"
mkdir -p "$dest"

die() { printf 'syndication-feeds: %s\n' "$*" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || die "missing required tool: curl"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{ print $1 }'
  else
    die "missing required tool: sha256sum or shasum"
  fi
}

size_of() { wc -c <"$1" | tr -d '[:space:]'; }

# name|url|operator
readonly FEEDS=(
  "ncts|https://api.healthterminologies.gov.au/syndication/v1/syndication.xml|Australian Digital Health Agency, National Clinical Terminology Service"
  "nhs-england|https://ontology.nhs.uk/production1/synd/syndication.xml|NHS England, Ontology Server"
  "mlds|https://mlds.ihtsdotools.org/api/feed|SNOMED International, Member Licensing and Distribution Service"
)

for spec in "${FEEDS[@]}"; do
  IFS='|' read -r name url _operator <<<"$spec"
  echo "== $name"
  curl --proto '=https' --tlsv1.2 -sSL --fail -o "$dest/$name.xml" "$url" ||
    die "fetch failed for $name ($url)"
done

fetched="$(date -u +%Y-%m-%d)"
# A literal backtick, so the markdown code spans below stay out of the printf
# format string, where shellcheck reads a backtick pair as a substitution.
tick='`'
{
  cat <<'HEADER'
# Provenance: public terminology syndication listings

Fetched verbatim by `scripts/vendor/syndication-feeds.sh`, never hand-edited.
To refresh or extend the corpus, change the script, re-run it, and commit the
result. The fetcher is run by hand and never in CI: every operator republishes
its listing on its own cadence, so an automatic refresh would rewrite the
fixtures the corpus test pins, and a corpus test that fails after a refresh is
read before it is re-pinned.

Each file is a listing document: titles, dates, canonical URLs, checksums, and
byte sizes of downloadable packages. None of them carries terminology content.
No SNOMED CT concept, description, relationship, or reference set member
appears here, so the SNOMED CT content rule
(`.claude/rules/vendored-inputs.md`) holds by construction. The copyright of
each listing belongs to the operator named in the table, and the listings are
served without authentication for exactly this purpose: to be read by a
syndication client.

The MLDS listing stamps its feed-level `updated` at request time, so its
SHA-256 moves on every fetch even when no entry has changed. Compare the
entries, not the digest, when judging whether a refresh brought new content.

HEADER
  printf '| File | Source | Operator | Fetched | Bytes | SHA-256 |\n'
  printf '|---|---|---|---|---|---|\n'
  for spec in "${FEEDS[@]}"; do
    IFS='|' read -r name url operator <<<"$spec"
    printf '| %s%s.xml%s | <%s> | %s | %s | %s | %s%s%s |\n' \
      "$tick" "$name" "$tick" "$url" "$operator" "$fetched" \
      "$(size_of "$dest/$name.xml")" "$tick" "$(sha256_of "$dest/$name.xml")" "$tick"
  done
  cat <<'FOOTER'

## What is not vendored, and why

The gap in the corpus is recorded by address, so a later refresh knows what was
tried. The crate itself (`crates/terminology-syndication/src/`) names no
operator, country, or code system.

- <https://nzhts.digital.health.nz/synd/syndication.xml> is open, and about 4 MB
  of it is one FHIR ValueSet entry after another. The corpus buys no parser
  coverage for that weight, so it stays out of the tree.
- <https://terminologieserver.nl/synd/syndication.xml> and
  <https://apps.health.belgium.be/ontoserver/synd/syndication.xml> both answer
  `401` with a `WWW-Authenticate: Bearer` challenge on the listing itself, so
  neither is vendorable without an account.
FOOTER
} >"$dest/PROVENANCE.md"

echo "== wrote $dest/PROVENANCE.md"
