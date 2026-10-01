#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# The crates.io upload of the `crates/*` members, and its read-back, as one
# implementation shared by the `crates` leg of release.yml (a `v*` tag,
# approval-gated) and publish-crates.yml (a manual dispatch: the dry-run and
# recovery lane). The two lanes cannot share a reusable workflow: crates.io
# Trusted Publishing matches the OIDC `workflow_ref` claim, and for a job inside
# a reusable workflow that claim names the calling workflow
# (<https://crates.io/docs/trusted-publishing>), so the shared thing is this
# script and each lane keeps its own job and identity.
#
# Per crate, in dependency order, because `cargo publish --workspace` refuses
# the whole run when any member version already exists while being non-atomic
# at the end: a partial publish could not be finished by re-running it. One
# crate at a time, "already exists" counted as done, makes the lane resumable
# and idempotent; the registry is read back before success is reported.
#
# Usage:
#   publish-crates.sh publish   # upload each crate in dependency order
#   publish-crates.sh verify    # read the registry back, with retries
#   publish-crates.sh version   # print the lockstep crate version
#
# Requires cargo, curl, and jq; `publish` also needs CARGO_REGISTRY_TOKEN.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Dependency order: a crate is uploaded only after every sibling it depends on
# is on the index.
readonly CRATES=(
  rf2
  concept-graph
  concept-store
  designation-index
  sct-ecl
  sct-scg
  loinc
  classification
  dhd-thesaurus
  gstandaard
  labcodeset
  icd11
  rxnorm-rrf
  fhir-terminology
)

# The crate line moves in lockstep (.claude/rules/crates-publishing.md), so one
# manifest answers for the set: the `[package]` table's own `version`.
manifest_version() {
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' crates/rf2/Cargo.toml
}

# cargo colours the status word, so "Uploaded" is followed by a reset sequence
# before the crate name; strip the colour before matching.
readonly ESC=$'\033'
strip_ansi() {
  sed -E "s/${ESC}\\[[0-9;]*m//g"
}

do_publish() {
  local crate out plain failed=""
  for crate in "${CRATES[@]}"; do
    echo "::group::$crate"
    out="$(cargo publish -p "$crate" --locked 2>&1)" || true
    printf '%s\n' "$out"
    echo "::endgroup::"
    plain="$(printf '%s' "$out" | strip_ansi)"
    case "$plain" in
    *"already exists on crates.io index"* | *"already uploaded"*)
      echo "$crate: already published at this version, nothing to do"
      ;;
    *)
      if printf '%s' "$plain" | grep -q "Uploaded $crate"; then
        echo "$crate: uploaded"
      else
        failed="$failed $crate"
      fi
      ;;
    esac
  done
  [[ -z "$failed" ]] || {
    echo "::error::failed to publish:$failed"
    return 1
  }
  echo "publish-crates: every crate is at $(manifest_version) or was already there"
}

# A split set is worse than an unpublished one: while the line is 0.x, cargo
# treats every 0.x as its own compatibility set, so one straggler makes its
# siblings' internal requirements unresolvable. Read the registry, never the
# exit code alone.
do_verify() {
  local want crate got body bad=""
  want="$(manifest_version)"
  echo "publish-crates: expecting every crate at $want"
  for crate in "${CRATES[@]}"; do
    # The index is eventually consistent right after an upload: a miss is
    # retried, and a failed request counts as "not seen yet", not as a miss.
    got=""
    for _ in 1 2 3 4 5 6; do
      if body="$(curl --proto '=https' --tlsv1.2 -sSL --fail -H 'User-Agent: ferroterm-publish-verify (https://github.com/FerroHEALTH/FerroTERM)' \
        "https://crates.io/api/v1/crates/$crate/versions" 2>/dev/null)"; then
        got="$(printf '%s' "$body" |
          jq -r --arg v "$want" '.versions[]? | select(.num == $v) | .num' |
          head -1)" || got=""
      fi
      [[ -n "$got" ]] && break
      sleep 10
    done
    printf '%-20s %s\n' "$crate" "${got:-MISSING}"
    [[ -n "$got" ]] || bad="$bad $crate"
  done
  [[ -z "$bad" ]] || {
    echo "::error::the published set is split; not at $want:$bad"
    return 1
  }
  echo "publish-crates: confirmed on crates.io, all ${#CRATES[@]} crates at $want"
}

case "${1:-}" in
publish) do_publish ;;
verify) do_verify ;;
version) manifest_version ;;
*)
  echo "publish-crates: expected 'publish', 'verify', or 'version', got '${1:-<none>}'" >&2
  exit 2
  ;;
esac
