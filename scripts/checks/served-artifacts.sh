#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every artifact a run named is being served, checked before the run measures
# anything.
#
#   scripts/checks/served-artifacts.sh --server URL --index DIRS
#
# --server is a versioned FHIR root (http://127.0.0.1:8098/r4b). --index is the
# FERROTERM_INDEX the server was given: artifact directories separated by the
# platform's path separator.
#
# The server refuses an artifact it cannot read, logs the reason, and carries
# on serving the rest. That is the right contract for an operator with nine
# good artifacts and one stale one, and the wrong one for a run: a conformance
# count or a latency figure taken over a system the server is not serving is a
# result about nothing, and nobody reads the log of a green run (#493).
#
# So the run asks. Each named directory's manifest.json says which system it
# holds; the server's TerminologyCapabilities says which systems it serves. A
# system named here and absent there was refused, and this exits non-zero
# naming it. A system nobody named is never looked for, so an artifact simply
# absent from a run's configuration reads differently from one that was named
# and refused.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

server=""
index=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --server) server=$2; shift 2 ;;
    --index) index=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$server" ]]; then
  echo "served-artifacts: --server names the versioned FHIR root to ask" >&2
  exit 2
fi

# A run that named no artifact has nothing to check: the server is serving
# whatever it was configured with, and this script has no claim to test.
if [[ -z "$index" ]]; then
  echo "served-artifacts: no artifact was named, so none can have been refused"
  exit 0
fi

capabilities="$(curl -sf "$server/metadata?mode=terminology")" || {
  echo "served-artifacts: $server answered no TerminologyCapabilities" >&2
  exit 1
}

served="$(printf '%s' "$capabilities" | jq -r '.codeSystem[]?.uri')"

missing=0
# The separator is the platform's, which is what std::env::split_paths reads
# on the server side, so the two agree on what one entry is.
while IFS= read -r directory || [[ -n "$directory" ]]; do
  [[ -n "$directory" ]] || continue
  manifest="$directory/manifest.json"
  if [[ ! -f "$manifest" ]]; then
    echo "served-artifacts: $directory holds no manifest.json, so it is not an artifact" >&2
    missing=1
    continue
  fi
  system="$(jq -r '.system // empty' "$manifest")"
  if [[ -z "$system" ]]; then
    echo "served-artifacts: $manifest names no system" >&2
    missing=1
    continue
  fi
  if ! printf '%s\n' "$served" | grep -qxF "$system"; then
    layout="$(jq -r '.storeLayout // "unstated"' "$manifest")"
    echo "served-artifacts: $directory holds $system and $server serves none of it." >&2
    echo "  The server read the artifact and refused it, then carried on. Its log says why;" >&2
    echo "  a stale store layout is the usual reason (this manifest says layout $layout)." >&2
    echo "  Rebuild the artifact with tools/ferroterm-build before measuring anything." >&2
    missing=1
  fi
# The final entry carries no newline of its own, so the read that returns it
# also reports end of input; without the second test the last artifact named
# would never be checked.
done < <(printf '%s\n' "$index" | tr ':' '\n')

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

echo "served-artifacts: every named artifact is being served"
