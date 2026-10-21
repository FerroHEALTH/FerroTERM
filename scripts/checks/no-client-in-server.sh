#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The server binary carries no general HTTP client. Its outbound calls are the
# authorization server [auth] names (its metadata and key set, read by the
# family resource-server crate `ferrohealth-smart`, whose `reqwest` is the one
# client the server links) and the usage report to FerroPULSE, over
# `hyper-util` plus `hyper-rustls`. The tree is read with `ferrohealth-smart`
# pruned, so a client any other edge brings in still fails the check.
# `reqwest` reaches the workspace only through the WHO ICD-API walker, which the
# offline build tool turns on with the `icd11/api` feature; a manifest edit that
# puts it back on a default path would restore the dependency with nothing
# saying so, and this is the check that fails instead.
#
#   scripts/checks/no-client-in-server.sh [--package NAME]
#
# The resolved normal dependency tree is read, so a transitive edge is caught
# too. `hyper-util` stays: it is the health probe (one GET to the server's own
# listener over loopback, since the container base has no shell to run a probe
# with) and the usage report. Exit 0 when the tree carries no client outside
# `ferrohealth-smart`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

package=ferroterm-server
# Every crate that speaks HTTP to somewhere else. `hyper` and `hyper-util` are
# the server's own listener and its health probe, so neither is listed.
clients=(reqwest ureq isahc curl surf awc)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package) package=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -f Cargo.toml ]]; then
  echo "no-client-in-server: no Cargo.toml yet, skipped."
  exit 0
fi

if ! cargo metadata --format-version 1 --no-deps --locked \
  | jq -e --arg p "$package" '.packages[] | select(.name == $p)' >/dev/null; then
  echo "no-client-in-server: no package named $package, skipped."
  exit 0
fi

# NOTE: no specification governs this: our own design; the resource-server
# crate reaches only the issuer [auth] names, so its client is the one allowed.
tree="$(cargo tree --package "$package" --edges normal --locked --prefix none --prune ferrohealth-smart)"

fail=0
for client in "${clients[@]}"; do
  if grep -qE "^${client} v" <<<"$tree"; then
    echo "::error::$package depends on $client, so the server binary carries an HTTP client." >&2
    fail=1
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "The server answers requests and reads its indexes; its outbound calls are the authorization server [auth] names, through ferrohealth-smart, and the usage report, over hyper-util." >&2
  echo "Put the client behind a cargo feature the server does not enable." >&2
  exit 1
fi

echo "ok: $package links no HTTP client outside ferrohealth-smart."
