#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# `addons/*` is optional by design. A source add-on is code the sync service
# compiles in and nothing else links, so the tree stays whole when an add-on is
# dropped and an add-on stays replaceable. Two edges would end that silently,
# and this is the check that fails instead:
#
#   1. a workspace member outside `app/ferroterm-sync` depending on an add-on,
#      which makes that add-on load-bearing for the server, the viewer, or a
#      library;
#   2. an add-on depending on a workspace crate other than the syndication
#      client and the leaf crates listed below, which ties one service's add-on
#      to the engine, to the server, or to another add-on.
#
#   scripts/checks/addon-boundary.sh [--self-test]
#
# Declared edges are what is judged. Normal and build dependencies count; a dev
# dependency is test scaffolding and is left alone. Rule 1 needs no closure
# walk: every workspace member is judged, so a member reaching an add-on
# through another member is caught at that other member's own edge.
#
# `--self-test` runs the rules over synthetic metadata instead of the tree: a
# clean workspace passes and each violation fails. It is how the local gate
# battery proves the guard still bites, and CI runs it beside the real check.
#
# Exit 0 when the tree is clean, 1 on a violation, 2 on a usage error.
set -euo pipefail

# The one crate an add-on always depends on, plus the leaf workspace crates it
# may also take. That list is deliberately empty: a need shared between add-ons
# belongs in the syndication client, so adding a name here is a decision that
# states its reason.
readonly SYNDICATION_CRATE=terminology-syndication
readonly ALLOWED_WORKSPACE_DEPS=("$SYNDICATION_CRATE")

usage() {
  echo "usage: $0 [--self-test]" >&2
  exit 2
}

# The synthetic workspace the self-test judges: a server, a sync binary, two
# add-ons, the syndication client, and the engine. $1 is the resolve graph.
self_test_metadata() {
  cat <<JSON
{
  "workspace_root": "/w",
  "workspace_members": ["server", "sync", "a", "b", "synd", "engine"],
  "packages": [
    {"id": "server", "name": "ferroterm-server", "manifest_path": "/w/app/ferroterm-server/Cargo.toml"},
    {"id": "sync", "name": "ferroterm-sync", "manifest_path": "/w/app/ferroterm-sync/Cargo.toml"},
    {"id": "a", "name": "addon-a", "manifest_path": "/w/addons/a/Cargo.toml"},
    {"id": "b", "name": "addon-b", "manifest_path": "/w/addons/b/Cargo.toml"},
    {"id": "synd", "name": "$SYNDICATION_CRATE", "manifest_path": "/w/crates/$SYNDICATION_CRATE/Cargo.toml"},
    {"id": "engine", "name": "fhir-terminology", "manifest_path": "/w/crates/fhir-terminology/Cargo.toml"}
  ],
  "resolve": {"nodes": $1}
}
JSON
}

# The clean graph: the server takes the engine, the sync binary takes both
# add-ons, and each add-on takes the syndication client and nothing else. $1 is
# appended to the first add-on's dependencies, which is how a case adds an edge.
self_test_nodes() {
  cat <<JSON
[
  {"id": "server", "deps": [{"pkg": "engine", "dep_kinds": [{"kind": null}]}]},
  {"id": "sync", "deps": [{"pkg": "a", "dep_kinds": [{"kind": null}]},
                          {"pkg": "b", "dep_kinds": [{"kind": null}]}]},
  {"id": "a", "deps": [{"pkg": "synd", "dep_kinds": [{"kind": null}]}${1:-}]},
  {"id": "b", "deps": [{"pkg": "synd", "dep_kinds": [{"kind": null}]}]},
  {"id": "synd", "deps": []},
  {"id": "engine", "deps": []}
]
JSON
}

# Give the server one more dependency, of the kind named in $1, on add-on `a`.
self_test_server_takes_addon() {
  local kind="$1"
  self_test_nodes | sed "s|\"deps\": \[{\"pkg\": \"engine\"|\"deps\": [{\"pkg\": \"a\", \"dep_kinds\": [{\"kind\": $kind}]}, {\"pkg\": \"engine\"|"
}

self_test() {
  local self="$1" dir
  dir="$(mktemp -d)"
  # shellcheck disable=SC2064  # $dir is expanded now, while it is still set.
  trap "rm -rf '$dir'" EXIT

  run_case() {
    local name="$1" nodes="$2" expect="$3" file rc
    file="$dir/$name.json"
    self_test_metadata "$nodes" >"$file"
    rc=0
    ADDON_BOUNDARY_METADATA_FILE="$file" "$self" >/dev/null 2>&1 || rc=$?
    if [[ "$rc" -ne "$expect" ]]; then
      echo "self-test: case '$name' exited $rc, expected $expect" >&2
      exit 1
    fi
  }

  run_case clean "$(self_test_nodes)" 0
  run_case dev-dependency "$(self_test_server_takes_addon '"dev"')" 0
  run_case member-takes-addon "$(self_test_server_takes_addon null)" 1
  run_case build-dependency "$(self_test_server_takes_addon '"build"')" 1
  run_case addon-takes-engine \
    "$(self_test_nodes ', {"pkg": "engine", "dep_kinds": [{"kind": null}]}')" 1
  run_case addon-takes-addon \
    "$(self_test_nodes ', {"pkg": "b", "dep_kinds": [{"kind": null}]}')" 1

  echo "ok: addon-boundary self-test (a clean workspace and a dev dependency pass; a member taking an add-on, an add-on taking the engine, and an add-on taking an add-on fail)"
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
self_path="$root/scripts/checks/$(basename "${BASH_SOURCE[0]}")"
cd "$root"

case "${1-}" in
  "") ;;
  --self-test) [[ "$#" -eq 1 ]] || usage; self_test "$self_path"; exit 0 ;;
  *) usage ;;
esac

# ADDON_BOUNDARY_METADATA_FILE is the self-test's way in and nothing else sets
# it. With it unset the tree itself is read, which is what CI and the local gate
# battery do.
if [[ -n "${ADDON_BOUNDARY_METADATA_FILE:-}" ]]; then
  metadata="$(cat "$ADDON_BOUNDARY_METADATA_FILE")"
else
  metadata="$(cargo metadata --format-version 1 --locked)"
fi

# Every declared workspace-to-workspace edge, as
# `from<TAB>from-role<TAB>to<TAB>to-role`, where a role is `addon`, `sync`, or
# `other`. A role is read from the manifest path, so a renamed package keeps it.
edges="$(printf '%s' "$metadata" | jq -r '
  def role($p; $root):
    if ($p.manifest_path | startswith($root + "/addons/")) then "addon"
    elif ($p.manifest_path | startswith($root + "/app/ferroterm-sync/")) then "sync"
    else "other" end;

  .workspace_root as $root
  | .workspace_members as $members
  | (.packages | map({key: .id, value: .}) | from_entries) as $pkgs
  | .resolve.nodes[]
  | select(.id as $id | $members | index($id))
  | .id as $from
  | .deps[]
  | select((.dep_kinds // []) | (length == 0) or any(.kind == null or .kind == "build"))
  | select(.pkg as $to | $members | index($to))
  | [$pkgs[$from].name, role($pkgs[$from]; $root),
     $pkgs[.pkg].name, role($pkgs[.pkg]; $root)]
  | @tsv
')"

# The add-ons the workspace holds, named whether or not they have an edge.
addons="$(printf '%s' "$metadata" | jq -r '
  .workspace_root as $root
  | .workspace_members as $members
  | .packages[]
  | select(.id as $id | $members | index($id))
  | select(.manifest_path | startswith($root + "/addons/"))
  | .name
' | sort)"

violations=()
while IFS=$'\t' read -r from from_role to to_role; do
  [[ -z "$from" ]] && continue
  if [[ "$to_role" == addon ]]; then
    case "$from_role" in
      sync) ;;
      addon) violations+=("the add-on $from depends on the add-on $to") ;;
      *) violations+=("$from depends on the add-on $to") ;;
    esac
    continue
  fi
  if [[ "$from_role" == addon ]]; then
    allowed=no
    for name in "${ALLOWED_WORKSPACE_DEPS[@]}"; do
      [[ "$to" == "$name" ]] && allowed=yes
    done
    [[ "$allowed" == yes ]] || violations+=("the add-on $from depends on the workspace crate $to")
  fi
done <<<"$edges"

if [[ "${#violations[@]}" -gt 0 ]]; then
  echo "::error::the add-on dependency boundary is broken:" >&2
  printf '  %s\n' "${violations[@]}" >&2
  echo >&2
  echo "Only app/ferroterm-sync links an add-on, and an add-on links" >&2
  echo "$SYNDICATION_CRATE and the leaf crates ALLOWED_WORKSPACE_DEPS names." >&2
  echo "Move the shared code into $SYNDICATION_CRATE, or name the new leaf" >&2
  echo "crate and its reason in this script." >&2
  exit 1
fi

if [[ -z "$addons" ]]; then
  echo "ok: addon-boundary (the workspace holds no add-on yet)."
  exit 0
fi

echo "ok: addon-boundary, every add-on inside the boundary: $(printf '%s' "$addons" | tr '\n' ' ')"
