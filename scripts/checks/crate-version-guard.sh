#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The crate-line rule (.claude/rules/crates-publishing.md): a change that
# alters the PACKAGED content of any `crates/*` member (what its `include`
# ships: `src/**`, `data/**`, `README.md`, `LICENSE`, `Cargo.toml`) leaves the
# lockstep crate line above its newest release on crates.io, because a
# published version is immutable. A root `[workspace.dependencies]` entry a
# published member consumes is packaged content too: `cargo package` renders
# the concrete requirement.
#
# The line steps once per publish, not once per merge: a base already above
# the release holds an unpublished version that every change until the next
# publish shares, so two pull requests never race on the manifests, and a base
# at the release takes the step. The release is the newest stable, unyanked
# version of any published member on the crates.io sparse index
# (https://doc.rust-lang.org/cargo/reference/registry-index.html), or on
# CRATE_GUARD_INDEX (a URL, or file:// for a tree in the same layout); an
# index that cannot be read fails the guard, never passes it.
#
# The step from the release follows The Cargo Book, "SemVer Compatibility"
# (https://doc.rust-lang.org/cargo/reference/semver.html): it moves upward, and
# a step to another compatibility line (the minor while the line is 0.x)
# resets what lies below it, so a breaking change moves 0.1.148 to 0.2.0. Which
# step a change needs is cargo-semver-checks' judgement (scripts/checks/semver.sh):
# the guard names the published crates to judge in `judged=` of
# $GITHUB_OUTPUT when CI sets it. A public dependency
# (scripts/checks/lib/public-deps.sh) required on another compatibility line
# breaks the crates, which the judge cannot see, so the guard refuses a patch
# step for it.
#
#   crate-version-guard.sh <base-ref> [head-ref]
#   crate-version-guard.sh --self-test
#
# The diff runs from the merge base of the base ref and the head, so a crate
# change the base gained after the head forked is not counted against the
# head, and the line never moves below the base ref's version.
#
# Exit 0 when no packaged content changed, or it changed and the line sits
# above its release (in every member and every internal requirement, and
# Cargo.lock followed). Exit 1 on a packaged change at the released version,
# a split line, a stale lock, a line below the base, a step SemVer does not
# allow, or an index that cannot be read.
# The `no-crate-bump` PR label is the CI escape for a diff that provably does
# not alter packaged bytes; this script does not read labels.
#
# `--self-test` runs the guard over synthetic repositories and a scratch
# index.
set -euo pipefail

self_test() {
  local self dir rc
  self="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
  dir="$(mktemp -d)"
  # shellcheck disable=SC2064  # $dir is expanded now, while it is still set.
  trap "rm -rf '$dir'" EXIT

  # A fresh repository: the published `rf2`, the unpublished `quiet`, and the
  # workspace dependencies `alpha` (rf2's) and `beta` (quiet's).
  export CRATE_GUARD_INDEX="file://$dir/index"
  # released VERSION...: the releases of rf2 the scratch index lists.
  released() {
    rm -rf "$dir/index"
    mkdir -p "$dir/index/3/r"
    : >"$dir/index/3/r/rf2"
    local v
    for v in "$@"; do printf '{"name":"rf2","vers":"%s","yanked":false}\n' "$v" >>"$dir/index/3/r/rf2"; done
  }
  fresh() {
    released 0.1.0
    rm -rf "$dir/repo"
    mkdir -p "$dir/repo/scripts/checks/lib" "$dir/repo/crates/rf2/src" "$dir/repo/crates/quiet/src"
    cp "$self" "$dir/repo/scripts/checks/crate-version-guard.sh"
    cp "$(dirname "$self")/lib/public-deps.sh" "$dir/repo/scripts/checks/lib/public-deps.sh"
    printf '[workspace.dependencies]\nalpha = "1.0.0"\nbeta = "1.0.0"\nfhir-types = "0.1.130"\nrf2 = { path = "crates/rf2", version = "0.1.0" }\nquiet = { path = "crates/quiet", version = "0.1.0" }\n' >"$dir/repo/Cargo.toml"
    printf '[package]\nname = "rf2"\nversion = "0.1.0"\n\n[dependencies]\nalpha.workspace = true\nfhir-types.workspace = true\n' >"$dir/repo/crates/rf2/Cargo.toml"
    printf '[package]\nname = "quiet"\nversion = "0.1.0"\npublish = false\n\n[dependencies]\nbeta.workspace = true\n' >"$dir/repo/crates/quiet/Cargo.toml"
    printf '[[package]]\nname = "quiet"\nversion = "0.1.0"\n\n[[package]]\nname = "rf2"\nversion = "0.1.0"\n' >"$dir/repo/Cargo.lock"
    : >"$dir/repo/crates/rf2/src/lib.rs"
    : >"$dir/repo/crates/quiet/src/lib.rs"
    git -C "$dir/repo" init -q
    git -C "$dir/repo" add -A
    git -C "$dir/repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit -qm base
  }
  # case NAME EXPECTED: commits the working tree and runs the guard on it.
  case_() {
    local name="$1" want_rc="$2"
    git -C "$dir/repo" add -A
    git -C "$dir/repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit -qm head
    rc=0
    "$dir/repo/scripts/checks/crate-version-guard.sh" HEAD~1 HEAD >/dev/null 2>&1 || rc=$?
    if [[ "$rc" -ne "$want_rc" ]]; then
      echo "self-test: case '$name' exited $rc, expected $want_rc" >&2
      exit 1
    fi
  }

  # bump VERSION: moves the whole line, the root requirements and the lock.
  bump() {
    local file
    for file in Cargo.toml crates/rf2/Cargo.toml crates/quiet/Cargo.toml Cargo.lock; do
      sed -i.bak -E "s/(version = \"?)0\.[0-9]+\.[0-9]+/\1$1/" "$dir/repo/$file"
      sed -i.bak -E "s/(path = \"crates\/[a-z0-9-]+\", version = \")[0-9.]+/\1$1/" "$dir/repo/$file"
      rm "$dir/repo/$file.bak"
    done
    sed -i.bak -E "/^name = \"(rf2|quiet)\"/{n;s/^version = \"[0-9.]+\"/version = \"$1\"/;}" "$dir/repo/Cargo.lock"
    rm "$dir/repo/Cargo.lock.bak"
  }
  change() { echo "// $1" >>"$dir/repo/crates/rf2/src/lib.rs"; }
  # rebase: commits the working tree as the next base.
  rebase() {
    git -C "$dir/repo" add -A
    git -C "$dir/repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit -qm base
  }

  fresh
  sed -i.bak 's/^beta = "1.0.0"/beta = "1.1.0"/' "$dir/repo/Cargo.toml" && rm "$dir/repo/Cargo.toml.bak"
  case_ unpublished-consumer 0
  fresh
  sed -i.bak 's/^alpha = "1.0.0"/alpha = "1.1.0"/' "$dir/repo/Cargo.toml" && rm "$dir/repo/Cargo.toml.bak"
  case_ published-consumer-without-bump 1
  fresh
  echo '// changed' >"$dir/repo/crates/quiet/src/lib.rs"
  case_ unpublished-source 0
  fresh
  change patch
  bump 0.1.1
  case_ a-patch-step 0
  fresh
  change break
  bump 0.2.0
  case_ a-minor-step-to-patch-zero 0
  fresh
  change break
  bump 0.3.0
  case_ a-skipped-minor 0
  fresh
  change break
  bump 0.2.1
  case_ a-minor-step-keeping-a-patch 1
  fresh
  sed -i.bak 's/^fhir-types = "0.1.130"/fhir-types = "0.2.0"/' "$dir/repo/Cargo.toml" && rm "$dir/repo/Cargo.toml.bak"
  bump 0.1.1
  case_ a-public-dependency-on-another-line-under-a-patch 1
  fresh
  sed -i.bak 's/^fhir-types = "0.1.130"/fhir-types = "0.2.0"/' "$dir/repo/Cargo.toml" && rm "$dir/repo/Cargo.toml.bak"
  bump 0.2.0
  case_ a-public-dependency-on-another-line-under-a-minor 0
  fresh
  sed -i.bak 's/^fhir-types = "0.1.130"/fhir-types = "0.1.131"/' "$dir/repo/Cargo.toml" && rm "$dir/repo/Cargo.toml.bak"
  bump 0.1.1
  case_ a-public-dependency-on-its-own-line-under-a-patch 0
  fresh
  change unbumped
  case_ a-change-at-the-released-version 1
  fresh
  bump 0.1.1
  rebase
  change shared
  case_ a-change-sharing-the-bases-unpublished-patch 0
  fresh
  bump 0.1.1
  rebase
  released 0.1.0 0.1.1
  change after-publish
  case_ a-change-at-a-version-published-since 1
  fresh
  bump 0.1.1
  rebase
  change break
  bump 0.2.0
  case_ a-minor-step-over-an-unpublished-patch 0
  fresh
  bump 0.2.0
  rebase
  change back
  bump 0.1.1
  case_ a-line-below-the-base 1
  fresh
  change split
  bump 0.1.1
  sed -i.bak -E '/^\[package\]/,/^$/s/^version = "0.1.1"/version = "0.1.2"/' "$dir/repo/crates/quiet/Cargo.toml" && rm "$dir/repo/crates/quiet/Cargo.toml.bak"
  case_ a-split-line 1
  fresh
  released
  change unreleased
  case_ a-line-never-released 0
  fresh
  change unread
  bump 0.1.1
  CRATE_GUARD_INDEX="file://$dir/missing" case_ an-index-that-cannot-be-read 1
  echo "crate-version-guard: self-test passed"
}

if [[ "${1:-}" == --self-test ]]; then
  self_test
  exit 0
fi
cd "$(dirname "$0")/../.."

base="${1:?usage: crate-version-guard.sh <base-ref> [head-ref]}"
head="${2:-HEAD}"
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. scripts/checks/lib/public-deps.sh
fork="$(git merge-base "$base" "$head")"

changed="$(git diff --name-only "$fork" "$head" --)"
packaged=0
# A here-string, never a pipe: `grep -q` stops at the first match, and a writer
# killed by SIGPIPE makes `set -o pipefail` report the whole test as false, so a
# large diff would silently answer "nothing packaged changed".
# A member that says `publish = false` ships nothing, so its files are not
# packaged content (its version still moves with the line, under crates/*),
# and neither are the workspace dependencies it alone consumes.
published=()
for manifest in $(git ls-tree -r --name-only "$head" -- crates | grep -E '^crates/[a-z0-9-]+/Cargo\.toml$'); do
  if git show "$head:$manifest" | grep -q '^publish = false'; then
    member="${manifest%/Cargo.toml}"
    changed="$(grep -v "^${member}/" <<<"$changed" || true)"
  else
    published+=("$manifest")
  fi
done
if grep -qE '^crates/[a-z0-9-]+/(src/|data/|README\.md$|LICENSE$|Cargo\.toml$)' <<<"$changed"; then
  packaged=1
fi
# Only the `[workspace.dependencies]` table renders into a packaged manifest;
# the `[workspace.package]` keys (the product `version`, edition, licence) are
# read through `key.workspace = true` and change no crate's packaged bytes.
workspace_dependencies() {
  awk '/^\[workspace\.dependencies\]/{p=1; next} /^\[/{p=0} p && /^[A-Za-z0-9_-]+[[:space:]]*=/{sub(/[[:space:]]*=.*/, ""); print}'
}
if [[ "$packaged" -eq 0 ]] && grep -qx 'Cargo.toml' <<<"$changed"; then
  diff_names="$(git diff "$fork" "$head" -- Cargo.toml | grep -E '^[+-][A-Za-z0-9_-]+[[:space:]]*=' | sed -E 's/^[+-]//; s/[[:space:]]*=.*//' | sort -u || true)"
  dependency_names="$( { git show "$head:Cargo.toml"; git show "$fork:Cargo.toml"; } | workspace_dependencies)"
  for name in $diff_names; do
    if ! grep -qx "$name" <<<"$dependency_names"; then
      continue
    fi
    if [[ "${#published[@]}" -gt 0 ]] && grep -lE "^${name}(\.workspace)?[[:space:]]*=" "${published[@]}" >/dev/null 2>&1; then
      echo "crate-version-guard: workspace dependency '$name' changed and published crates/* members consume it: packaged requirements move."
      packaged=1
      break
    fi
  done
fi

if [[ "$packaged" -eq 0 ]]; then
  echo "crate-version-guard: no packaged content of crates/* changed."
  exit 0
fi

package_version() {
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}'
}
old_ver="$(git show "$base:crates/rf2/Cargo.toml" 2>/dev/null | package_version || true)"
new_ver="$(package_version < crates/rf2/Cargo.toml)"

fail=0
for manifest in crates/*/Cargo.toml; do
  ver="$(package_version < "$manifest")"
  [[ "$ver" = "$new_ver" ]] || { echo "::error::$manifest is at $ver, the crate line is $new_ver (bumps are lockstep)." >&2; fail=1; }
  name="$(awk -F'"' '/^\[package\]/{p=1} p && /^name = /{print $2; exit}' "$manifest")"
  req="$(grep -E "^${name} = \{ path = \"crates/${name}\", version = \"" Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/' || true)"
  [[ "$req" = "$new_ver" ]] || { echo "::error::root Cargo.toml requires $name $req, the crate line is $new_ver." >&2; fail=1; }
  locked="$(awk -v n="\"$name\"" '$1 == "name" && $3 == n { hit = 1; next } hit && $1 == "version" { gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
  [[ "$locked" = "$new_ver" ]] || { echo "::error::Cargo.lock records $name ${locked:-nothing}, the crate line is $new_ver; run cargo update -w and commit the lock." >&2; fail=1; }
done
[[ "$fail" -eq 0 ]] || exit 1

# The compatibility line of version $1 (the Cargo caret rule).
line_of() { compat_line "$1"; }

# Whether version $1 sorts strictly above version $2.
version_above() {
  [[ "$1" != "$2" ]] && [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n 1)" = "$1" ]]
}

# The sparse index path of crate $1 (Cargo Book, "Registry Index", Index
# files): 1/, 2/, 3/<first char>/ or <first two>/<next two>/ before the name.
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

# The newest stable, unyanked release of crate $1 on the index; empty when the
# index has none. Exits the guard when the index cannot be read.
published_version() {
  local index="${CRATE_GUARD_INDEX:-https://index.crates.io}" path body status file
  path="$(index_path "$1")"
  if [[ "$index" == file://* ]]; then
    [[ -d "${index#file://}" ]] || { echo "::error::crate-version-guard: the index ${index} is not a directory." >&2; exit 1; }
    [[ -f "${index#file://}/$path" ]] || return 0
    body="$(cat "${index#file://}/$path")"
  else
    file="$(mktemp)"
    status="$(curl --proto '=https' --tlsv1.2 -sS -m 30 -A 'ferroterm-crate-version-guard' \
      -o "$file" -w '%{http_code}' "$index/$path" 2>/dev/null)" || status="000"
    case "$status" in
      200) body="$(cat "$file")"; rm -f "$file" ;;
      404) rm -f "$file"; return 0 ;;
      *) rm -f "$file"; echo "::error::crate-version-guard: the index answered $status for $1 at $index/$path; the newest release cannot be read." >&2; exit 1 ;;
    esac
  fi
  jq -r 'select(.yanked | not) | .vers | select(test("-") | not)' <<<"$body" | sort -V | tail -n 1
}

if [[ -n "$old_ver" ]] && version_above "$old_ver" "$new_ver"; then
  echo "::error::the crate line moves from the base's $old_ver down to $new_ver; a version only moves upward." >&2
  exit 1
fi

# The line's release: the newest release of any published member, since the
# line publishes them together.
pub_ver=""
for manifest in "${published[@]}"; do
  name="$(git show "$head:$manifest" | awk -F'"' '/^\[package\]/{p=1} p && /^name = /{print $2; exit}')"
  release="$(published_version "$name")"
  if [[ -n "$release" ]] && { [[ -z "$pub_ver" ]] || version_above "$release" "$pub_ver"; }; then
    pub_ver="$release"
  fi
done

if [[ -n "$pub_ver" ]]; then
  if ! version_above "$new_ver" "$pub_ver"; then
    echo "::error::packaged content of crates/* changed but the crate line $new_ver is not above its newest release on crates.io, $pub_ver. Move every crates/*/Cargo.toml 'version' and every internal 'version =' requirement in the root Cargo.toml one step above $pub_ver, refresh Cargo.lock, or apply the 'no-crate-bump' label when the diff provably does not alter packaged bytes." >&2
    exit 1
  fi
  IFS=. read -r o_major o_minor _ <<<"$pub_ver"
  IFS=. read -r n_major n_minor n_patch <<<"$new_ver"
  if [[ "$(line_of "$pub_ver")" != "$(line_of "$new_ver")" ]]; then
    if [[ "$n_major" != "$o_major" && ( "$n_minor" != 0 || "$n_patch" != 0 ) ]] ||
      [[ "$n_major" == 0 && "$n_minor" != "$o_minor" && "$n_patch" != 0 ]]; then
      echo "::error::the crate line leaves its release $pub_ver for another compatibility line at $new_ver without resetting what lies below it; a breaking change moves 0.y.z to 0.(y+1).0 (The Cargo Book, SemVer Compatibility)." >&2
      exit 1
    fi
  else
    # Every published crate's public dependencies, at the merge base and here.
    moved=""
    for manifest in "${published[@]}"; do
      crate="$(basename "$(dirname "$manifest")")"
      before="$(manifest_requirements <(git show "$fork:Cargo.toml") <(git show "$fork:$manifest" 2>/dev/null || true) | public_lines)"
      after="$(manifest_requirements Cargo.toml "$manifest" | public_lines)"
      step="$(moved_lines "$before" "$after")"
      [[ -z "$step" ]] || moved="$moved $crate ($step)"
    done
    if [[ -n "$moved" ]]; then
      echo "::error::a public dependency moves to another compatibility line:$moved; that breaks the crates' consumers, so the line moves its minor ($pub_ver to $o_major.$((o_minor + 1)).0), never the patch (The Cargo Book, SemVer Compatibility)." >&2
      exit 1
    fi
  fi
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  judged=""
  for manifest in "${published[@]}"; do
    judged="$judged $(basename "$(dirname "$manifest")")"
  done
  echo "judged=${judged# }" >> "$GITHUB_OUTPUT"
fi
echo "crate-version-guard: packaged content changed and the crate line is $new_ver, above its release ${pub_ver:-<none>}, lockstep and locked."
