#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# The changelog guard (no specification governs it: our own design): a pull
# request records its user-visible effect in the changelog. It passes when the
# change adds a fragment under changelog.d/ (any file there but the README),
# or, while pull requests opened before the fragments still land, when it
# edits CHANGELOG.md itself. scripts/release/changelog.sh --check judges the
# fragment's form; this guard asks only whether there is one.
#
#   changelog-guard.sh <base-ref> [head-ref]
#   changelog-guard.sh --self-test
#
# The change is what the head added since it forked from the base, read from
# the diff against the merge base of the two, so a fragment that reached the
# base after the fork is never counted as the change's own.
#
# Exit 0 when the change carries an entry, 1 when it carries none, and 2 on a
# usage error or a base and head with no merge base. The `no-changelog`
# pull-request label is the CI escape for a change with no user-visible effect;
# this script does not read labels.
set -euo pipefail

# The self-test builds a stub repository in a temporary directory, with a copy
# of this script in it, and runs the guard there as CI does: the base is
# main's tip and the head is the branch.
self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  # git writes its objects read-only, so the tree is made writable before it
  # is removed.
  trap 'chmod -R u+w "$work" && rm -r -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  mkdir -p "$work/scripts/checks" "$work/changelog.d"
  cp "$script" "$work/scripts/checks/changelog-guard.sh"

  # A repository of its own, so no global hook, signing key or identity of the
  # caller's reaches it.
  stub_git() {
    git -C "$work" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  commit() {
    local message="$1"
    stub_git add -A
    stub_git commit -q -m "$message"
  }
  branch() {
    local name="$1"
    stub_git checkout -q main
    stub_git checkout -q -b "$name"
  }
  # expect NAME WANT BRANCH: the guard over BRANCH against main exits WANT.
  expect() {
    local name=$1 want=$2 branch=$3
    status=0
    (cd "$work" && bash scripts/checks/changelog-guard.sh main "$branch") > "$work/out" 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "changelog-guard: self-test failed: $name exited $status, wanted $want." >&2
      cat "$work/out" >&2
      exit 1
    fi
  }

  stub_git init -q -b main
  printf '# Changelog\n\n## [Unreleased]\n' > "$work/CHANGELOG.md"
  printf 'The fragment format.\n' > "$work/changelog.d/README.md"
  printf -- '- An entry already released.\n' > "$work/changelog.d/1-old.added.md"
  echo 'code' > "$work/code.txt"
  commit "the fork point"

  branch fragment
  echo 'more code' >> "$work/code.txt"
  printf -- '- An entry.\n' > "$work/changelog.d/2-new.added.md"
  commit "a change with a fragment"
  branch edited
  echo 'more code' >> "$work/code.txt"
  printf '\n### Added\n\n- An entry.\n' >> "$work/CHANGELOG.md"
  commit "a change that edits CHANGELOG.md"
  branch none
  echo 'more code' >> "$work/code.txt"
  commit "a change with no entry"
  branch readme
  echo 'A clearer note.' >> "$work/changelog.d/README.md"
  commit "a change to the README alone"
  branch reworded
  printf -- '- An entry, reworded.\n' > "$work/changelog.d/1-old.added.md"
  commit "a change that rewords a fragment it did not add"
  branch removed
  stub_git rm -q -- changelog.d/1-old.added.md
  commit "a change that removes a fragment"

  stub_git checkout -q main
  printf -- '- An entry from main.\n' > "$work/changelog.d/3-main.fixed.md"
  commit "main gains a fragment after every branch forked"

  expect "a change that adds a fragment" 0 fragment
  expect "a change that edits CHANGELOG.md" 0 edited
  expect "a change with no entry, behind a main that gained a fragment" 1 none
  grep -q 'no changelog entry' "$work/out" || {
    echo "changelog-guard: self-test failed: the refusal did not say why." >&2
    cat "$work/out" >&2
    exit 1
  }
  expect "a change to the README alone" 1 readme
  expect "a change that rewords a fragment it did not add" 1 reworded
  expect "a change that removes a fragment" 1 removed
  echo "changelog-guard: self-test passed."
}

if [[ "${1:-}" = "--self-test" && $# -eq 1 ]]; then
  self_test
  exit 0
fi

cd "$(dirname "$0")/../.."

if [[ $# -lt 1 || $# -gt 2 || -z "${1:-}" ]]; then
  echo "usage: changelog-guard.sh <base-ref> [head-ref] | --self-test" >&2
  exit 2
fi
base="$1"
head="${2:-HEAD}"

if ! fork="$(git merge-base "$base" "$head")"; then
  echo "changelog-guard: $base and $head have no merge base; check out the full history (fetch-depth: 0)." >&2
  exit 2
fi

# A rename is read as a removal and an addition, so a fragment renamed into
# place counts as added and one renamed away does not.
added="$(git diff --no-renames --name-only --diff-filter=A "$fork" "$head" -- 'changelog.d/*' ':!changelog.d/README.md')"
if [[ -n "$added" ]]; then
  echo "changelog-guard: the change adds the fragment(s):"
  while IFS= read -r path; do echo "  $path"; done <<<"$added"
  exit 0
fi
if [[ -n "$(git diff --name-only "$fork" "$head" -- CHANGELOG.md)" ]]; then
  echo "changelog-guard: the change edits CHANGELOG.md."
  exit 0
fi
echo "::error::no changelog entry: add a fragment, changelog.d/<issue>-<kebab-slug>.<section>.md, as changelog.d/README.md describes, or apply the 'no-changelog' label when the change has no user-visible effect." >&2
exit 1
