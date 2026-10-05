#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every workspace member is named in the repo map of CLAUDE.md and in the
# workspace-layout table of docs/architecture.md. The two lists are prose, so
# nothing else keeps them complete when a crate lands; this check does. A
# member is a directory with a Cargo.toml under crates/, addons/, app/, or
# tools/; a line naming it is any line containing `<dir>/<name>`.
#
# Usage: scripts/checks/repo-map.sh   (no arguments)
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "$#" -ne 0 ]; then
  echo "usage: $0   (takes no arguments)" >&2
  exit 2
fi

status=0
count=0
for manifest in crates/*/Cargo.toml addons/*/Cargo.toml app/*/Cargo.toml tools/*/Cargo.toml; do
  member="$(dirname "$manifest")"
  count=$((count + 1))
  for doc in CLAUDE.md docs/architecture.md; do
    if ! grep -qF "$member" "$doc"; then
      echo "error: $doc does not name the workspace member $member" >&2
      status=1
    fi
  done
done
[ "$status" -eq 0 ] && echo "ok: repo-map (every one of the $count workspace members is named in CLAUDE.md and docs/architecture.md)"
exit "$status"
