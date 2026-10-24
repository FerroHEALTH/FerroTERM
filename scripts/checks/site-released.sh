#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The published site names only released versions (no specification governs
# this: our own design). The release PR moves every version in the book and
# the landing page to the coming release before its tag exists, so a site
# deployed from main between that merge and the published release would send
# a reader to an image, a tarball and a compose file that are not there.
#
#   site-released.sh
#       Exit 0 when every FerroTERM version the book or the landing page names
#       is a published GitHub release that carries its compose.yaml. Exit 1
#       naming each version that is not. The versions read are the workspace
#       version wherever the site names it, and every version an image tag, a
#       release download or a release asset name carries.
#   site-released.sh --self-test
#       Proves the check against a stub gh over a synthetic site.
#
# A release is published only after release.yml's asset verification and
# image scan have passed, so a published release with its compose.yaml means
# the image tags and the tarballs exist too. The repository is
# GITHUB_REPOSITORY, or the one gh resolves from the checkout; gh
# authenticates with GH_TOKEN. Exit 2 on a usage error. Requires gh and jq.
set -euo pipefail

usage() {
  echo "usage: site-released.sh | --self-test" >&2
  exit 2
}

repo() {
  if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    printf '%s' "$GITHUB_REPOSITORY"
  else
    gh repo view --json nameWithOwner --jq .nameWithOwner
  fi
}

# workspace_version ROOT: the [workspace.package] version of ROOT/Cargo.toml.
workspace_version() {
  local root="$1"
  awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version[[:space:]]*=/{gsub(/[" ]/,""); sub(/^version=/,""); print; exit}' "$root/Cargo.toml"
}

# named_versions ROOT: one line per distinct version the site under ROOT
# names, sorted.
named_versions() {
  local root="$1" current dirs=()
  current="$(workspace_version "$root")"
  [[ -n "$current" ]] || {
    echo "::error::site-released: $root/Cargo.toml has no [workspace.package] version" >&2
    exit 1
  }
  [[ -d "$root/website/book/src" ]] && dirs+=("$root/website/book/src")
  [[ -d "$root/website/landing" ]] && dirs+=("$root/website/landing")
  [[ "${#dirs[@]}" -gt 0 ]] || return 0
  {
    if grep -rqF -- "$current" "${dirs[@]}"; then
      printf '%s\n' "$current"
    fi
    grep -rhoE -- '(ferroterm(-console|-sync)?:|ferroterm(-console)?-v|release download v)[0-9]+\.[0-9]+\.[0-9]+' "${dirs[@]}" |
      grep -oE '[0-9]+\.[0-9]+\.[0-9]+$' || true
  } | sort -uV
}

# released REPO VERSION: exit 0 when vVERSION is a published release with its
# compose.yaml; otherwise print the reason.
released() {
  local repository="$1" version="$2" json
  if ! json="$(gh release view "v$version" --repo "$repository" --json isDraft,assets 2>/dev/null)"; then
    echo "there is no release v$version"
    return 1
  fi
  if [[ "$(jq -r .isDraft <<<"$json")" == true ]]; then
    echo "the release v$version is still a draft"
    return 1
  fi
  if ! jq -e '.assets | map(.name) | index("compose.yaml")' <<<"$json" >/dev/null; then
    echo "the release v$version carries no compose.yaml"
    return 1
  fi
}

check() {
  local root="$1" repository version reason fails=0 count=0
  repository="$(repo)"
  while IFS= read -r version; do
    [[ -n "$version" ]] || continue
    count=$((count + 1))
    if ! reason="$(released "$repository" "$version")"; then
      echo "::error::site-released: the site names $version, but $reason" >&2
      fails=$((fails + 1))
    fi
  done < <(named_versions "$root")
  if [[ "$fails" -ne 0 ]]; then
    echo "site-released: $fails version(s) the site names are not released. Publish the release before the site, or keep the site on the last release" >&2
    return 1
  fi
  echo "site-released: every version the site names ($count) is a published release."
}

self_test() {
  local scratch fails=0 out
  scratch="$(mktemp -d)"
  trap 'rm -rf "$scratch"' RETURN
  export GITHUB_REPOSITORY=o/r
  mkdir -p "$scratch/website/book/src" "$scratch/website/landing"
  printf '[workspace.package]\nversion = "1.2.0"\n' >"$scratch/Cargo.toml"
  # The stub answers `gh release view` from STUB_RELEASES, a JSON object keyed
  # by tag; a missing key is a release that does not exist.
  # shellcheck disable=SC2329 # the functions under test call it in place of gh
  gh() {
    if [[ "$1 $2" == "release view" && "$4 $5" == "--repo o/r" ]]; then
      jq -e --arg t "$3" '.[$t] // empty' <<<"$STUB_RELEASES" || return 1
    else
      echo "stub gh: unexpected call: $*" >&2
      return 1
    fi
  }
  # expect NAME WANT_RC WANT_TEXT
  expect() {
    local name="$1" want_rc="$2" want_text="$3" rc
    if out="$(check "$scratch" 2>&1)"; then rc=0; else rc=$?; fi
    if [[ "$rc" != "$want_rc" || "$out" != *"$want_text"* ]]; then
      echo "FAIL: $name (exit $rc): $out" >&2
      fails=$((fails + 1))
    else
      echo "ok: $name"
    fi
  }
  local good='{"isDraft":false,"assets":[{"name":"compose.yaml"}]}'
  echo "Run docker run ghcr.io/ferrohealth/ferroterm:1.2.0." >"$scratch/website/book/src/install.md"
  echo '<p>v1.2.0 released.</p>' >"$scratch/website/landing/index.html"
  STUB_RELEASES="{\"v1.2.0\":$good}"
  expect "a released version passes" 0 "every version the site names (1)"
  STUB_RELEASES='{}'
  expect "an unreleased version fails" 1 "names 1.2.0, but there is no release v1.2.0"
  STUB_RELEASES='{"v1.2.0":{"isDraft":true,"assets":[{"name":"compose.yaml"}]}}'
  expect "a draft release fails" 1 "is still a draft"
  STUB_RELEASES='{"v1.2.0":{"isDraft":false,"assets":[]}}'
  expect "a release without its compose file fails" 1 "carries no compose.yaml"
  echo 'gh release download v1.1.0 -p ferroterm-v1.1.0-x86_64.tar.gz' >>"$scratch/website/book/src/install.md"
  STUB_RELEASES="{\"v1.2.0\":$good}"
  expect "an older release download is checked too" 1 "names 1.1.0, but there is no release v1.1.0"
  STUB_RELEASES="{\"v1.2.0\":$good,\"v1.1.0\":$good}"
  expect "every named release published passes" 0 "every version the site names (2)"
  : >"$scratch/website/book/src/install.md"
  : >"$scratch/website/landing/index.html"
  STUB_RELEASES='{}'
  expect "a site that names no version passes" 0 "every version the site names (0)"
  unset -f gh
  if [[ "$fails" -ne 0 ]]; then
    echo "site-released: self-test FAILED ($fails)" >&2
    return 1
  fi
  echo "site-released: self-test OK."
}

case "${1:-}" in
  "")
    check "$(cd "$(dirname "$0")/../.." && pwd)"
    ;;
  --self-test)
    [[ $# -eq 1 ]] || usage
    self_test
    ;;
  *) usage ;;
esac
