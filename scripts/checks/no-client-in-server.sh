#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# The server binary carries no general HTTP client. Its outbound calls are the
# OIDC issuer's discovery document and JWKS, which the optional SMART gate
# makes, and the usage report to FerroPULSE, both over `hyper-util` plus
# `hyper-rustls`.
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
# with) and the SMART gate's read of the configured issuer. Exit 0 when the tree
# carries no client.
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

tree="$(cargo tree --package "$package" --edges normal --locked --prefix none)"

fail=0
for client in "${clients[@]}"; do
  if grep -qE "^${client} v" <<<"$tree"; then
    echo "::error::$package depends on $client, so the server binary carries an HTTP client." >&2
    fail=1
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "The server answers requests and reads its indexes; its outbound calls are the OIDC issuer and the usage report, over hyper-util." >&2
  echo "Put the client behind a cargo feature the server does not enable." >&2
  exit 1
fi

echo "ok: $package links no HTTP client."
