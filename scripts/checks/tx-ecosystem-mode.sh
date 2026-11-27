#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# One tx-ecosystem mode that CI runs (conformance/tx-ecosystem/ci-modes.tsv),
# with the content it needs fetched from its public source for the run alone.
#
#   scripts/checks/tx-ecosystem-mode.sh --mode NAME [--fhir r4|r4b|r5]
#
# Build the release binaries first: cargo build --release -p ferroterm-server
# -p ferroterm-build. The content is checked against the sha256 pinned below,
# built into an artifact, and served for the run. Content, artifact, report and
# server log all live in one temporary directory outside the repository, which
# is removed when the script exits: nothing licensed reaches the tree, the
# build cache (which saves target/), or an uploaded artifact.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The suite's SNOMED CT test edition, tx-source/snomed of the suite repository
# at the commit scripts/checks/tx-ecosystem.sh pins. The digest is the sha256
# of the `shasum -a 256` listing of every file, sorted by path.
SUITE_REPO=https://github.com/HL7/fhir-tx-ecosystem-ig
SUITE_COMMIT=9c2bbce57366319d6a5265cbfbf6697f094822e3
SNOMED_SHA256=b70fb3a4f7e565b34c832670171133d3d69a8844eb35deb29d6e60d0f0bd29b5

mode=""
fhir=r4b
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) mode=$2; shift 2 ;;
    --fhir) fhir=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$mode" ]] || { echo "usage: tx-ecosystem-mode.sh --mode NAME [--fhir r4|r4b|r5]" >&2; exit 2; }
if ! awk -F'\t' -v s="$fhir" -v m="$mode" '!/^#/ && $1 == s && $2 == m { found = 1 } END { exit !found }' \
  conformance/tx-ecosystem/ci-modes.tsv; then
  echo "tx-ecosystem-mode: conformance/tx-ecosystem/ci-modes.tsv has no row for $mode on /$fhir" >&2
  exit 2
fi
if [[ "$(grep -c "^SUITE_COMMIT=$SUITE_COMMIT\$" scripts/checks/tx-ecosystem.sh)" != 1 ]]; then
  echo "tx-ecosystem-mode: the content pin differs from the suite commit of scripts/checks/tx-ecosystem.sh" >&2
  exit 1
fi

scratch="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/tx-ecosystem-mode.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# The sha256 of a directory: the sorted listing of its files' digests, hashed.
tree_digest() {
  (cd "$1" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 | shasum -a 256 | cut -d' ' -f1)
}

index=""
case "$mode" in
  snomed)
    ig="$scratch/ig"
    git init -q "$ig"
    git -C "$ig" remote add origin "$SUITE_REPO"
    git -C "$ig" sparse-checkout set tx-source/snomed
    git -C "$ig" fetch -q --depth 1 origin "$SUITE_COMMIT"
    git -C "$ig" checkout -q FETCH_HEAD
    digest="$(tree_digest "$ig/tx-source/snomed")"
    if [[ "$digest" != "$SNOMED_SHA256" ]]; then
      echo "tx-ecosystem-mode: the SNOMED CT test edition hashes to $digest, not $SNOMED_SHA256" >&2
      exit 1
    fi
    # The edition ships Terminology/ and Refset/ with no Snapshot/ above them,
    # and is imported under the unpublished xsct URI its readme names.
    mkdir -p "$scratch/edition/Snapshot"
    mv "$ig/tx-source/snomed/Terminology" "$ig/tx-source/snomed/Refset" "$scratch/edition/Snapshot/"
    index="$scratch/index/snomed"
    target/release/ferroterm-build --rf2 "$scratch/edition" --rf2-unpublished --out "$index"
    rm -rf "$ig" "$scratch/edition"
    ;;
  *)
    echo "tx-ecosystem-mode: no content recipe for $mode" >&2
    exit 2
    ;;
esac

scripts/checks/tx-ecosystem.sh --mode "$mode" --fhir "$fhir" --index "$index" --out "$scratch/out"
