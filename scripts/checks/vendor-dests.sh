#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every fetcher under scripts/vendor/ writes to a directory that exists in the
# tree (.claude/rules/vendored-inputs.md). A fetcher whose destination moved
# with a crate rename would, re-run, write a corpus nowhere reads, and the
# committed corpus would then drift from its fetcher without any signal. The
# destination is the first `dest=` or `data=` assignment in the script.
#
# Usage: scripts/checks/vendor-dests.sh   (no arguments)
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "$#" -ne 0 ]; then
  echo "usage: $0   (takes no arguments)" >&2
  exit 2
fi

status=0
for script in scripts/vendor/*.sh; do
  dest="$(grep -m1 -E '^(dest|data)=' "$script" | sed -E 's/^(dest|data)=//; s/^"//; s/"$//')"
  if [ -z "$dest" ]; then
    echo "error: $script names no dest= or data= destination" >&2
    status=1
    continue
  fi
  if [ ! -d "$dest" ]; then
    echo "error: $script writes to $dest, which is not a directory in the tree" >&2
    status=1
    continue
  fi
  echo "ok: $script -> $dest"
done
[ "$status" -eq 0 ] && echo "ok: vendor-dests (every fetcher writes into the tree)"
exit "$status"
