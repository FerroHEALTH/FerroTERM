#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
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
# Per crate, because `cargo publish --workspace` refuses the whole run when any
# member version already exists while being non-atomic at the end: a partial
# publish could not be finished by re-running it. One crate at a time, "already
# exists" counted as done, makes the lane resumable and idempotent; the
# registry is read back before success is reported.
#
# The upload order is derived from the workspace's dependency graph
# (`cargo metadata`), whatever order the list below is written in, and before a
# crate is uploaded every published sibling it depends on must be visible at
# the crate-line version in the crates.io sparse index
# (<https://doc.rust-lang.org/cargo/reference/registry-index.html>).
#
# Usage:
#   publish-crates.sh publish     # upload each crate in dependency order
#   publish-crates.sh verify      # read the registry back, with retries
#   publish-crates.sh version     # print the lockstep crate version
#   publish-crates.sh order       # print the derived upload order
#   publish-crates.sh crates      # print the published list, one per line
#   publish-crates.sh semver      # judge each crate against its newest
#                                 # release on crates.io (scripts/checks/semver.sh)
#   publish-crates.sh self-test   # check the order and the index wait offline
#
# Exit status: 0 done; 1 a crate failed to upload, or the read-back found a
# split set; 2 a usage error; 3 no upload order exists (a cycle, or a published
# crate that needs an unpublished member); 4 a dependency never showed on the
# sparse index within the bounded wait.
#
# Environment, with defaults: PUBLISH_CRATES_INDEX, the sparse index root
# (https://index.crates.io); PUBLISH_CRATES_INDEX_ATTEMPTS (30) and
# PUBLISH_CRATES_INDEX_DELAY, the seconds between attempts (10);
# PUBLISH_CRATES_METADATA, a `cargo metadata` JSON file read in place of
# running cargo.
#
# Requires cargo, curl, and jq; `publish` also needs CARGO_REGISTRY_TOKEN.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The published list (`.claude/rules/crates-publishing.md`), which
# scripts/checks/versions.sh also reads. It names the members; the upload order
# is derived below.
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

readonly EXIT_UPLOAD=1
readonly EXIT_USAGE=2
readonly EXIT_ORDER=3
readonly EXIT_INDEX=4

INDEX_ROOT="${PUBLISH_CRATES_INDEX:-https://index.crates.io}"
INDEX_ATTEMPTS="${PUBLISH_CRATES_INDEX_ATTEMPTS:-30}"
INDEX_DELAY="${PUBLISH_CRATES_INDEX_DELAY:-10}"

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

metadata() {
  if [[ -n "${PUBLISH_CRATES_METADATA:-}" ]]; then
    cat "$PUBLISH_CRATES_METADATA"
  else
    cargo metadata --format-version 1 --no-deps
  fi
}

# One "<crate> <dependency>" line per edge the upload order must respect, and
# an "unpublished <crate> <dependency>" line per edge no order can satisfy. A
# path dependency on a published member is an edge, dev-dependencies included,
# since `cargo package` keeps a versioned one in the uploaded manifest; on an
# unpublished member it is refused, except a dev-dependency with no version,
# which `cargo package` strips
# (<https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#development-dependencies>).
edges() {
  local published
  published="$(printf '%s\n' "${CRATES[@]}" | jq -R . | jq -sc .)"
  metadata | jq -r --argjson pub "$published" '
    (.packages | map(.name)) as $ws
    | .packages[]
    | .name as $crate
    | select($pub | index($crate))
    | .dependencies[]
    | .name as $dep
    | select(.path != null and ($ws | index($dep)))
    | if ($pub | index($dep)) then "\($crate) \($dep)"
      elif .kind == "dev" and .req == "*" then empty
      else "unpublished \($crate) \($dep)" end'
}

# The upload order: each round takes, in list order, every crate whose
# published dependencies are all placed. A round that places nothing is a cycle.
derive_order() {
  local graph line crate dep from ok progress bad="" placed=" "
  local remaining=("${CRATES[@]}") next
  graph="$(edges)" || return "$EXIT_ORDER"
  while IFS= read -r line; do
    [[ "$line" == unpublished\ * ]] && bad="$bad ${line#unpublished }"
  done <<<"$graph"
  if [[ -n "$bad" ]]; then
    echo "::error::order: a published crate depends on an unpublished member:$bad" >&2
    return "$EXIT_ORDER"
  fi
  while ((${#remaining[@]})); do
    next=()
    progress=""
    for crate in "${remaining[@]}"; do
      ok=1
      while read -r from dep; do
        [[ "$from" == "$crate" && "$placed" != *" $dep "* ]] && ok=0
      done <<<"$graph"
      if ((ok)); then
        echo "$crate"
        placed="$placed$crate "
        progress=1
      else
        next+=("$crate")
      fi
    done
    if [[ -z "$progress" ]]; then
      echo "::error::order: the published crates depend on each other in a cycle: ${next[*]}" >&2
      return "$EXIT_ORDER"
    fi
    remaining=(${next[@]+"${next[@]}"})
  done
}

# The sparse index file of a crate name, lower-cased: 1/, 2/, 3/<first>/, or
# <first two>/<next two>/.
index_path() {
  local name
  name="$(tr '[:upper:]' '[:lower:]' <<<"$1")"
  case "${#name}" in
  1) echo "1/$name" ;;
  2) echo "2/$name" ;;
  3) echo "3/${name:0:1}/$name" ;;
  *) echo "${name:0:2}/${name:2:2}/$name" ;;
  esac
}

# Wait until <crate> at <version> is a line of its sparse index file. A failed
# request counts as "not seen yet": the index is eventually consistent right
# after an upload.
wait_for_index() {
  local crate="$1" version="$2" url attempt body
  url="$INDEX_ROOT/$(index_path "$crate")"
  for ((attempt = 1; attempt <= INDEX_ATTEMPTS; attempt++)); do
    if body="$(curl --proto '=https,file' --tlsv1.2 -sSL --fail -H 'Cache-Control: no-cache' \
      -H 'User-Agent: ferroterm-publish (https://github.com/FerroHEALTH/FerroTERM)' "$url" 2>/dev/null)" &&
      jq -se --arg v "$version" 'any(.[]; .vers == $v)' <<<"$body" >/dev/null 2>&1; then
      echo "$crate $version: on the sparse index (attempt $attempt)"
      return 0
    fi
    if ((attempt < INDEX_ATTEMPTS)); then sleep "$INDEX_DELAY"; fi
  done
  echo "::error::index-wait: $crate $version is not on the sparse index at $url after $INDEX_ATTEMPTS attempts, ${INDEX_DELAY}s apart" >&2
  return "$EXIT_INDEX"
}

do_publish() {
  local order version graph crate dep from out plain failed="" blocked
  order="$(derive_order)" || exit "$EXIT_ORDER"
  graph="$(edges)"
  version="$(manifest_version)"
  for crate in $order; do
    blocked=""
    while read -r from dep; do
      [[ "$from" == "$crate" ]] || continue
      if [[ " $failed " == *" $dep "* ]]; then
        blocked="$dep"
        break
      fi
      wait_for_index "$dep" "$version" || exit "$EXIT_INDEX"
    done <<<"$graph"
    if [[ -n "$blocked" ]]; then
      echo "$crate: not attempted, its dependency $blocked failed to publish"
      failed="$failed $crate"
      continue
    fi
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
    echo "::error::failed to publish:$failed" >&2
    exit "$EXIT_UPLOAD"
  }
  echo "publish-crates: every crate is at $version or was already there"
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
    echo "::error::the published set is split; not at $want:$bad" >&2
    exit "$EXIT_UPLOAD"
  }
  echo "publish-crates: confirmed on crates.io, all ${#CRATES[@]} crates at $want"
}

# One package of a stub metadata document: <name> then <dep>:<kind>:<req>...
stub_package() {
  local name="$1" deps="" spec d k r kind
  shift
  for spec in "$@"; do
    IFS=: read -r d k r <<<"$spec"
    kind="\"$k\""
    [[ "$k" == normal ]] && kind=null
    deps="$deps{\"name\":\"$d\",\"kind\":$kind,\"req\":\"$r\",\"path\":\"/w/$d\"},"
  done
  printf '{"name":"%s","dependencies":[%s]}' "$name" "${deps%,}"
}

# Offline: the workspace order places every crate after each published sibling
# it depends on, a cycle and an unpublished dependency are refused, and the
# index wait passes on a listed version and fails, typed and bounded, on a
# missing one. The stubs live in a directory this run creates.
do_self_test() {
  local order crate dep from status placed=" " fails=0 work
  SELF_TEST_DIR="$(mktemp -d)"
  trap 'rm -rf "$SELF_TEST_DIR"' EXIT
  work="$SELF_TEST_DIR"
  fail() {
    echo "self-test FAILED: $*" >&2
    fails=$((fails + 1))
  }

  order="$(derive_order)" || fail "no upload order for the workspace"
  for crate in $order; do
    while read -r from dep; do
      [[ "$from" == "$crate" && "$placed" != *" $dep "* ]] && fail "$crate is placed before its dependency $dep"
    done < <(edges)
    placed="$placed$crate "
  done
  [[ "$(wc -w <<<"$order" | tr -d ' ')" == "${#CRATES[@]}" ]] || fail "the order does not place all ${#CRATES[@]} crates"
  echo "self-test: the workspace order is $(tr '\n' ' ' <<<"$order")"

  {
    printf '{"packages":['
    for crate in "${CRATES[@]}"; do
      case "$crate" in
      rf2) stub_package rf2 fhir-terminology:normal:^1 ;;
      fhir-terminology) stub_package fhir-terminology rf2:dev:^1 ;;
      *) stub_package "$crate" ;;
      esac
      [[ "$crate" == "${CRATES[${#CRATES[@]} - 1]}" ]] || printf ','
    done
    printf ']}'
  } >"$work/cycle.json"
  status=0
  PUBLISH_CRATES_METADATA="$work/cycle.json" derive_order >/dev/null 2>"$work/err" || status=$?
  if ! ((status == EXIT_ORDER)) || ! grep -q 'cycle: rf2 fhir-terminology' "$work/err"; then
    fail "a cycle through a dev-dependency was not refused with status $EXIT_ORDER (got $status)"
  fi

  printf '{"packages":[%s,%s,%s]}' "$(stub_package rf2 ferroterm-testkit:normal:*)" \
    "$(stub_package loinc ferroterm-testkit:dev:*)" "$(stub_package ferroterm-testkit)" >"$work/unpublished.json"
  status=0
  PUBLISH_CRATES_METADATA="$work/unpublished.json" derive_order >/dev/null 2>"$work/err" || status=$?
  if ! ((status == EXIT_ORDER)) || ! grep -q 'member: rf2 ferroterm-testkit$' "$work/err"; then
    fail "a normal dependency on an unpublished member was not refused alone with status $EXIT_ORDER (got $status)"
  fi

  mkdir -p "$work/index/sc/t-"
  printf '{"name":"sct-scg","vers":"0.1.1"}\n{"name":"sct-scg","vers":"0.1.2"}\n' >"$work/index/sc/t-/sct-scg"
  INDEX_ROOT="file://$work/index" INDEX_ATTEMPTS=2 INDEX_DELAY=0 wait_for_index sct-scg 0.1.2 >/dev/null ||
    fail "a version on the index was not seen"
  status=0
  INDEX_ROOT="file://$work/index" INDEX_ATTEMPTS=3 INDEX_DELAY=0 wait_for_index sct-scg 0.1.3 >/dev/null 2>"$work/err" || status=$?
  if ! ((status == EXIT_INDEX)) || ! grep -q 'index-wait: sct-scg 0.1.3 is not on the sparse index .* after 3 attempts' "$work/err"; then
    fail "a missing version was not refused with status $EXIT_INDEX after 3 attempts (got $status)"
  fi
  [[ "$(index_path a) $(index_path ab) $(index_path abc) $(index_path Rf2) $(index_path sct-ecl)" == "1/a 2/ab 3/a/abc 3/r/rf2 sc/t-/sct-ecl" ]] ||
    fail "a sparse index path is wrong"

  ((fails == 0)) || exit "$EXIT_UPLOAD"
  echo "self-test: passed"
}

case "${1:-}" in
publish) do_publish ;;
verify) do_verify ;;
version) manifest_version ;;
order) derive_order ;;
crates) printf '%s\n' "${CRATES[@]}" ;;
semver) bash scripts/checks/semver.sh "${CRATES[@]}" ;;
self-test) do_self_test ;;
*)
  echo "publish-crates: expected 'publish', 'verify', 'version', 'order', 'crates', 'semver', or 'self-test', got '${1:-<none>}'" >&2
  exit "$EXIT_USAGE"
  ;;
esac
