#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The console is a FHIR client and links NO crate from this workspace, so that
# anything the console can do a client can do. One dependency edge into the
# engine would break that property silently, and this is the check that fails
# instead.
#
#   scripts/checks/console-boundary.sh [--package NAME]
#
# The whole resolved dependency closure is walked, not just the manifest, so a
# transitive edge is caught too. Exit 0 when the closure is clean.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

package=ferroterm-console

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package) package=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

metadata="$(cargo metadata --format-version 1 --locked)"

if ! printf '%s' "$metadata" | jq -e --arg p "$package" '.packages[] | select(.name == $p)' >/dev/null; then
  echo "console-boundary: no package named $package — skipped."
  exit 0
fi

# Every workspace member except the console itself, by package id.
members="$(printf '%s' "$metadata" | jq -r --arg p "$package" '
  [.workspace_members[]] as $ids
  | .packages[]
  | select(.id as $id | $ids | index($id))
  | select(.name != $p)
  | .name
' | sort)"

# The console'"'"'s resolved closure: its own node plus everything reachable from it.
closure="$(printf '%s' "$metadata" | jq -r --arg p "$package" '
  (.packages[] | select(.name == $p) | .id) as $rootid
  | (.resolve.nodes | map({key: .id, value: [.deps[].pkg]}) | from_entries) as $edges
  | (.packages | map({key: .id, value: .name}) | from_entries) as $names
  | [$rootid]
  | until(
      (map(. as $id | $edges[$id] // []) | flatten | unique) as $next
      | ($next - .) | length == 0;
      (. + (map(. as $id | $edges[$id] // []) | flatten)) | unique
    )
  | map($names[.])
  | unique[]
' | sort)"

violations="$(comm -12 <(printf '%s\n' "$members") <(printf '%s\n' "$closure") | sed '/^$/d')"

if [[ -n "$violations" ]]; then
  echo "::error::$package depends on workspace crates, which breaks the FHIR-client boundary:" >&2
  printf '%s\n' "$violations" | sed 's/^/  /' >&2
  echo "The console reaches this server only over the FHIR API. Remove the dependency." >&2
  exit 1
fi

echo "console-boundary: $package links no workspace crate."
