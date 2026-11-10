#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The SemVer judge of the published crates (.claude/rules/crates-publishing.md,
# #1208): cargo-semver-checks compares each crate's public API with a baseline
# and refuses a breaking change that the version does not answer with a minor
# bump while the line is 0.x (0.1.148 to 0.2.0), the rule of The Cargo Book,
# "SemVer Compatibility" (https://doc.rust-lang.org/cargo/reference/semver.html).
# A public dependency (scripts/checks/lib/public-deps.sh) moved to another
# compatibility line breaks a crate where the judge cannot see it, so a crate
# that stays on its line while one moves is refused too. Two passes judge each
# crate: the judge's default feature selection, and --only-explicit-features,
# what a dependent built with default-features = false compiles against. No
# specification governs where the judge runs: our own design.
# crate-version-guard.sh names the crates to judge on a pull request and in the
# merge queue, against the target branch, and the release plan judges every
# crate it publishes against crates.io (scripts/release/publish-crates.sh
# semver).
#
#   semver.sh [--baseline-rev <rev>] [<crate>...]
#       Judge the named crates, or the published list, against <rev> or,
#       without it, against each crate's newest release on crates.io. A crate
#       at its baseline's version, or new since it, is left out.
#   semver.sh --pin
#       Print the cargo-semver-checks version docs/VERSIONS.md pins.
#
# The installed cargo-semver-checks must be the pinned version. Exit 0 when
# every crate's version answers its change, 1 when one does not, 2 on a usage
# or setup error.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. scripts/checks/lib/public-deps.sh

usage() {
  echo "usage: semver.sh [--baseline-rev <rev>] [<crate>...] | --pin" >&2
  exit 2
}

# The Pin cell of the cargo-semver-checks row, its backticks stripped.
pin="$(awk -F'|' '{ k = $2; gsub(/[` ]/, "", k) } k == "cargo-semver-checks" { v = $3; gsub(/[^0-9.]/, "", v); print v; exit }' docs/VERSIONS.md)"
if [[ -z "$pin" ]]; then
  echo "semver: docs/VERSIONS.md pins no cargo-semver-checks version" >&2
  exit 2
fi
if [[ "${1:-}" == "--pin" ]]; then
  [[ "$#" -eq 1 ]] || usage
  echo "$pin"
  exit 0
fi

baseline_rev=""
if [[ "${1:-}" == "--baseline-rev" ]]; then
  [[ "$#" -ge 2 ]] || usage
  baseline_rev="$2"
  shift 2
fi

if ! installed="$(cargo semver-checks --version 2>/dev/null)"; then
  echo "semver: cargo-semver-checks is not installed; install $pin (cargo binstall cargo-semver-checks@$pin)" >&2
  exit 2
fi
if [[ "$installed" != "cargo-semver-checks $pin" ]]; then
  echo "semver: found '$installed', docs/VERSIONS.md pins $pin" >&2
  exit 2
fi

# The `[package]` table's own `version` in the manifest on stdin.
package_version() {
  awk -F'"' '/^\[package\]/{p=1; next} /^\[/{p=0} p && /^version[[:space:]]*=/{print $2; exit}'
}

# The crates.io API document at path $1 under /api/v1/crates/.
crates_io() {
  curl --proto '=https' --tlsv1.2 -sSL --fail \
    -H 'User-Agent: ferroterm-semver (scripts/checks/semver.sh)' \
    "https://crates.io/api/v1/crates/$1" 2>/dev/null
}

# The newest stable release of crate $1 on crates.io; empty when it has none.
newest_release() {
  crates_io "$1" | jq -r '.crate.max_stable_version // empty' || true
}

# The version crate $1 has at the baseline: its manifest at the baseline
# revision, or its newest stable release on crates.io; empty when it has none.
baseline_version() {
  local crate="$1"
  if [[ -n "$baseline_rev" ]]; then
    git show "$baseline_rev:crates/$crate/Cargo.toml" 2>/dev/null | package_version || true
  else
    newest_release "$crate"
  fi
}

# Whether $1 and $2 share their compatibility line: the major, and the minor
# too while the major is 0 (the Cargo caret rule).
same_line() {
  local a="$1" b="$2"
  if [[ "${a%%.*}" == "0" ]]; then
    [[ "$(cut -d. -f1-2 <<<"$a")" == "$(cut -d. -f1-2 <<<"$b")" ]]
  else
    [[ "${a%%.*}" == "${b%%.*}" ]]
  fi
}

# The public dependency lines of crate $1 in the tree, at revision $2, and of
# release $2 on crates.io.
tree_lines() {
  manifest_requirements Cargo.toml "crates/$1/Cargo.toml" | public_lines
}
revision_lines() {
  manifest_requirements <(git show "$2:Cargo.toml") <(git show "$2:crates/$1/Cargo.toml") | public_lines
}
release_lines() {
  crates_io "$1/$2/dependencies" |
    jq -r '.dependencies[] | select(.kind == "normal") | "\(.crate_id) \(.req)"' | public_lines
}

# The public dependencies of crate $1 that moved to another compatibility line
# since version $2 at the baseline. Against a revision, the newest release on
# crates.io counts too while it shares the line of $2: a move that landed
# under an unpublished patch still breaks the consumers of that release.
moved_since() {
  local crate="$1" previous="$2" current published moved=""
  current="$(tree_lines "$crate")"
  if [[ -n "$baseline_rev" ]]; then
    moved="$(moved_lines "$(revision_lines "$crate" "$baseline_rev")" "$current")"
    published="$(newest_release "$crate")"
    if [[ -n "$published" ]] && ! same_line "$published" "$previous"; then published=""; fi
  else
    published="$previous"
  fi
  if [[ -z "$moved" ]] && [[ -n "$published" ]]; then
    moved="$(moved_lines "$(release_lines "$crate" "$published")" "$current")"
  fi
  echo "$moved"
}

crates=("$@")
if [[ "${#crates[@]}" -eq 0 ]]; then
  while IFS= read -r crate; do crates+=("$crate"); done < <(bash scripts/release/publish-crates.sh crates)
fi

# The baselines build in a target directory of their own, so no other cargo
# run of this checkout reads or writes them while they build.
export CARGO_TARGET_DIR="$root/target/semver"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
against="${baseline_rev:-crates.io}"
failed=""
for crate in "${crates[@]}"; do
  [[ -f "crates/$crate/Cargo.toml" ]] || { echo "semver: no crate crates/$crate" >&2; exit 2; }
  current="$(package_version < "crates/$crate/Cargo.toml")"
  previous="$(baseline_version "$crate")"
  if [[ -z "$previous" ]]; then
    echo "semver: $crate has no release at $against to compare with"
    continue
  fi
  if [[ "$previous" == "$current" ]]; then
    echo "semver: $crate is at $current at $against too, nothing to judge"
    continue
  fi
  echo "semver: $crate $previous -> $current against $against"
  args=(check-release --package "$crate")
  [[ -z "$baseline_rev" ]] || args+=(--baseline-rev "$baseline_rev")
  for pass in default --only-explicit-features; do
    status=0
    if [[ "$pass" == default ]]; then
      cargo semver-checks "${args[@]}" > "$log" 2>&1 || status=$?
    else
      cargo semver-checks "${args[@]}" "$pass" > "$log" 2>&1 || status=$?
    fi
    cat "$log"
    # Only a summary the judge printed counts as a judgement; a build that
    # fails is named apart from a break.
    if [[ "$status" -ne 0 ]] && grep -q 'semver requires new' "$log"; then
      echo "::error::semver: $crate $current breaks its public API against $previous ($pass features); a breaking change bumps the minor while the line is 0.x (.claude/rules/crates-publishing.md)." >&2
      failed="$failed $crate"
      continue 2
    fi
    if [[ "$status" -ne 0 ]] || ! grep -q 'Summary' "$log"; then
      echo "::error::semver: cargo-semver-checks could not judge $crate $current against $previous ($pass features, exit $status); see its output above." >&2
      failed="$failed $crate"
      continue 2
    fi
  done
  moved="$(moved_since "$crate" "$previous")"
  if [[ -n "$moved" ]] && same_line "$previous" "$current"; then
    echo "::error::semver: $crate moves from $previous to $current while a public dependency moves to another compatibility line ($moved); that breaks its consumers, so it bumps the minor (.claude/rules/crates-publishing.md)." >&2
    failed="$failed $crate"
  fi
done

if [[ -n "$failed" ]]; then
  echo "semver: refused:$failed" >&2
  exit 1
fi
echo "semver: every judged version answers its change"
