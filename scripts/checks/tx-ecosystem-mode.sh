#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# One tx-ecosystem mode that CI runs (conformance/tx-ecosystem/ci-modes.tsv),
# with the content it needs fetched from its public source for the run alone.
#
#   scripts/checks/tx-ecosystem-mode.sh --mode NAME [--fhir r4|r4b|r5]
#   scripts/checks/tx-ecosystem-mode.sh --self-test   # a line outside the allow-list is withheld
#
# Build the release binaries first: cargo build --release -p ferroterm-server
# -p ferroterm-build. The content is checked against the sha256 pinned below,
# built into an artifact, and served for the run. Content, artifact, report and
# server log all live in one temporary directory outside the repository, which
# is removed when the script exits: nothing licensed reaches the tree, the
# build cache (which saves target/), or an uploaded artifact. A public
# workflow's log is public, so the build and the run write to files there, and
# only the lines of the allow-list below are printed: case names, counts and
# the scripts' own messages. Any other line is withheld and fails the run.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The suite's SNOMED CT test edition, tx-source/snomed of the suite repository
# at the commit scripts/checks/tx-ecosystem.sh pins. The digest is the sha256
# of the `shasum -a 256` listing of every file, sorted by path.
SUITE_REPO=https://github.com/HL7/fhir-tx-ecosystem-ig
SUITE_COMMIT=9c2bbce57366319d6a5265cbfbf6697f094822e3
SNOMED_SHA256=b70fb3a4f7e565b34c832670171133d3d69a8844eb35deb29d6e60d0f0bd29b5
# The ICF releases 2001a and 2017a, as the CodeSystem resources of the
# fhir.tx.support package (CC0-1.0, publisher WHO) on the FHIR package registry.
ICF_PACKAGE=https://packages-cdn.fhir.org/web/fhir.tx.support-1.0.5.tgz
ICF_SHA256=1de150b9c1d277280309f94ae7bea9e1b57c20685c898c00e10e2938cac46a1a

# The lines a run may print. None can carry a term: a case name is the suite's
# identifier, and the scripts' messages name counts, paths and system URIs.
ALLOWED='^(tx-ecosystem: |tx-ecosystem-mode: |served-artifacts: |fetching (validator_cli\.jar [0-9.]+|the suite at [0-9a-f]{40})$|newly passing \(add them to [A-Za-z0-9._/-]+\):$|REGRESSION: on the pass list but failing now:$|  [+-] [A-Za-z0-9._/-]+$|[a-z]+://[^ ]+: [0-9]+ concepts, [0-9]+ designations, [0-9]+ is-a edges, [0-9]+ words, written to [^ ]+$)'

# Prints the lines of FILE the allow-list admits, and fails naming how many it
# withheld.
print_allowed() {
  local file=$1 withheld
  grep -E "$ALLOWED" "$file" || true
  withheld="$(grep -cvE "$ALLOWED" "$file" || true)"
  if [[ "$withheld" != 0 ]]; then
    echo "tx-ecosystem-mode: $withheld line(s) of output withheld, since they may quote licensed content" >&2
    return 1
  fi
}

if [[ "${1:-}" == "--self-test" ]]; then
  probe="$(mktemp)"
  trap 'rm -f "$probe"' EXIT
  printf '%s\n' "tx-ecosystem: 283 of 378 snomed tests pass on /r4b (tests v1.9.3, runner 7.0.6)" \
    "REGRESSION: on the pass list but failing now:" "  - snomed/lookup-de" > "$probe"
  print_allowed "$probe" > /dev/null || { echo "tx-ecosystem-mode: self-test: a run's own lines are withheld" >&2; exit 1; }
  for leak in "    The display 'Liver problem' is not valid" "  - snomed/lookup with a display" \
    '{"level":"WARN","message":"code 300338006"}' "Exception in thread main"; do
    printf '%s\n' "$leak" > "$probe"
    if print_allowed "$probe" > /dev/null 2>&1; then
      echo "tx-ecosystem-mode: self-test: '$leak' is printed" >&2
      exit 1
    fi
  done
  echo "tx-ecosystem-mode: self-test OK"
  exit 0
fi

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
    status=0
    target/release/ferroterm-build --rf2 "$scratch/edition" --rf2-unpublished --out "$index" > "$scratch/build.log" 2>&1 || status=$?
    print_allowed "$scratch/build.log" || status=1
    [[ "$status" == 0 ]] || { echo "tx-ecosystem-mode: the build of the SNOMED CT test edition failed" >&2; exit 1; }
    rm -rf "$ig" "$scratch/edition"
    ;;
  mimetypes)
    # BCP 13 is served from the vendored IANA media types registry.
    ;;
  omop)
    # The server loads no OMOP vocabulary, so the run measures what it serves.
    ;;
  icf)
    curl --proto '=https' --tlsv1.2 -sSfL -o "$scratch/package.tgz" "$ICF_PACKAGE"
    digest="$(shasum -a 256 "$scratch/package.tgz" | cut -d' ' -f1)"
    if [[ "$digest" != "$ICF_SHA256" ]]; then
      echo "tx-ecosystem-mode: the fhir.tx.support package hashes to $digest, not $ICF_SHA256" >&2
      exit 1
    fi
    mkdir -p "$scratch/code-systems"
    tar -xzf "$scratch/package.tgz" -C "$scratch/code-systems" --strip-components 1 \
      package/CodeSystem-icf-2001a.json package/CodeSystem-icf-2017a.json
    rm "$scratch/package.tgz"
    export FERROTERM__TERMINOLOGY__CODE_SYSTEMS="$scratch/code-systems"
    ;;
  *)
    echo "tx-ecosystem-mode: no content recipe for $mode" >&2
    exit 2
    ;;
esac

status=0
scripts/checks/tx-ecosystem.sh --mode "$mode" --fhir "$fhir" --index "$index" --out "$scratch/out" --names-only \
  > "$scratch/run.log" 2>&1 || status=$?
print_allowed "$scratch/run.log" || status=1
exit "$status"
