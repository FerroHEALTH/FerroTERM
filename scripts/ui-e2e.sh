#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The console's browser journeys: headless Chrome, driven over WebDriver
# (https://www.w3.org/TR/webdriver2/), against the console and the server
# this checkout builds, all on loopback.
#
#   scripts/ui-e2e.sh
#   scripts/ui-e2e.sh --docs-shots
#   scripts/ui-e2e.sh --review DIR
#
# The script owns everything it drives and stops it again by process id. It
# builds the server, the console and its site bundle, and the stub identity
# provider (e2e/src/bin/stub_issuer.rs), then starts on 127.0.0.1:
#
#   18080  a server with no issuer: the read-only journeys' deployment
#   18081  a server that trusts the stub issuer, its resource store in a
#          temporary directory: the signed-in journeys' deployment
#   18443  the stub issuer, plain HTTP, which both the server and the
#          console admit on loopback
#   13000  the console over the first server, offering no sign-in
#   13001  the console over the second, signing a reader in at the issuer
#   4444   chromedriver
#
# Every address is loopback, so the console's cookies may go without
# `Secure` (the console refuses that anywhere else) and no certificate is
# needed. The browser is Chrome for Testing with its chromedriver, named by
# FERROTERM_UI_E2E_CHROME and FERROTERM_UI_E2E_CHROMEDRIVER, or found on PATH;
# CI installs both at the release docs/VERSIONS.md pins. Without either, the
# script fetches that release into target/journey-tools, and the site build's
# tailwindcss and wasm-opt into target/viewer-tools, so a fresh checkout runs
# with nothing installed by hand beyond cargo-nextest and cargo-leptos.
#
# The battery must not change a tracked file. The documentation capture pass
# is the one exception, and it runs alone when --docs-shots asks for it: it
# writes one PNG per screen the book embeds into
# website/book/src/operate/img/viewer (FERROTERM_UI_E2E_SHOTS_DIR sends it
# elsewhere). --review DIR runs only the capture, writing every screen at
# desktop and phone width in both themes, and the hover, focus and open
# states, into DIR for a visual review.
#
# The image the release lane builds is not what the journeys drive; CI builds
# docker/viewer/Dockerfile from a staged tree on its own (no specification
# governs this: our own design).
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

docs_shots=""
review=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --docs-shots)
      docs_shots=1
      shift
      ;;
    --review)
      [[ $# -ge 2 ]] || { echo "usage: scripts/ui-e2e.sh --review DIR" >&2; exit 2; }
      review="$2"
      shift 2
      ;;
    *)
      echo "usage: scripts/ui-e2e.sh [--docs-shots | --review DIR]" >&2
      exit 2
      ;;
  esac
done

die() {
  echo "ui-e2e: $*" >&2
  exit 1
}

need() {
  command -v "$1" > /dev/null || die "$1 is not on PATH; $2"
}

need cargo-nextest "cargo install cargo-nextest --locked"
need cargo-leptos "cargo install cargo-leptos --locked --version 0.3.11"

# The browser: the one FERROTERM_UI_E2E_CHROMEDRIVER and FERROTERM_UI_E2E_CHROME
# name, else a chromedriver on PATH, else the pinned Chrome for Testing in
# target/journey-tools, fetched there on first use.
chromedriver="${FERROTERM_UI_E2E_CHROMEDRIVER:-$(command -v chromedriver || true)}"
if [[ -z "$chromedriver" ]]; then
  tools="$root/target/journey-tools"
  chromedriver="$(bash scripts/release/journey-tools.sh --paths "$tools" | sed -n 1p)"
  if [[ ! -x "$chromedriver" ]]; then
    echo "ui-e2e: no chromedriver named or on PATH; fetching the pinned Chrome for Testing into $tools"
    bash scripts/release/journey-tools.sh "$tools"
  fi
  export FERROTERM_UI_E2E_CHROME="${FERROTERM_UI_E2E_CHROME:-$(bash scripts/release/journey-tools.sh --paths "$tools" | sed -n 2p)}"
fi
[[ -x "$chromedriver" ]] || die "$chromedriver is not an executable chromedriver"

# The site build's tools, tailwindcss and wasm-opt: the pinned ones on PATH,
# else the ones in target/viewer-tools, fetched there on first use.
if ! bash scripts/release/viewer-tools.sh --check > /dev/null 2>&1; then
  tools="$root/target/viewer-tools"
  export PATH="$tools/bin:$PATH"
  if ! bash scripts/release/viewer-tools.sh --check > /dev/null 2>&1; then
    echo "ui-e2e: the pinned tailwindcss and wasm-opt are not on PATH; fetching them into $tools"
    bash scripts/release/viewer-tools.sh "$tools"
  fi
fi

# How many journeys drive the browser at once: each opens its own Chrome under
# the one chromedriver, and more sessions than the processors (four on the
# ubuntu-latest runner) starve the browsers into timeouts. The same number is
# the test-thread count below.
readonly BROWSER_SESSIONS=4

work="$(mktemp -d)"
FAILURES_DIR="${FERROTERM_UI_E2E_FAILURES:-$root/target/ui-e2e-failures}"
mkdir -p "$FAILURES_DIR"
pids=()

# Stops everything this run started, by the process id it recorded.
cleanup() {
  local pid
  for pid in ${pids[@]+"${pids[@]}"}; do
    kill "$pid" 2> /dev/null || true
  done
  for pid in ${pids[@]+"${pids[@]}"}; do
    wait "$pid" 2> /dev/null || true
  done
  rm -f -- "$work"/*.log "$work"/*.redb
  rmdir -- "$work" 2> /dev/null || true
}
trap cleanup EXIT

# Waits until URL answers 200, or fails with the log LOG.
await() {
  local url="$1" log="$2" tries=0
  until curl --silent --fail --output /dev/null "$url"; do
    tries=$((tries + 1))
    if [[ "$tries" -gt 300 ]]; then
      echo "ui-e2e: $url never answered; its log:" >&2
      cat "$log" >&2 || true
      exit 1
    fi
    sleep 0.2
  done
}

# Every port must be free before anything starts: a process left on one
# answers the readiness probe, and the journeys would drive it instead of
# this checkout.
for port in 18080 18081 18443 13000 13001 4444; do
  if curl --silent --output /dev/null "http://127.0.0.1:$port/"; then
    die "127.0.0.1:$port already answers; stop whatever holds it first"
  fi
done

# The console's two halves are built the way the release lane builds them:
# a debug build of cargo-leptos's front end is compiled with a cfg the plain
# server build is not, and two halves built apart that way do not hydrate.
echo "ui-e2e: building the server, the console and the stub issuer"
cargo build --locked -p ferroterm-server
cargo build --locked --release -p ferroterm-viewer
cargo build --locked --manifest-path e2e/Cargo.toml --bin stub-issuer
echo "ui-e2e: building the console's site bundle"
bash scripts/release/viewer-site.sh --release

issuer="http://127.0.0.1:18443"
e2e/target/debug/stub-issuer --plain --listen 127.0.0.1:18443 --issuer "$issuer" \
  --ca "$work/ca.pem" > "$work/issuer.log" 2>&1 &
pids+=("$!")
await "$issuer/.well-known/openid-configuration" "$work/issuer.log"

serve() {
  local port="$1" log="$2"
  shift 2
  env FERROTERM_USAGE_REPORT=off FERROTERM_LISTEN="127.0.0.1:$port" \
    FERROTERM_CODESYSTEMS=e2e/fixtures/codesystems "$@" \
    target/debug/ferroterm > "$log" 2>&1 &
  pids+=("$!")
  await "http://127.0.0.1:$port/health" "$log"
}
# The anonymous console writes through a server with [auth] off; the signed-in
# one through a server that trusts the stub issuer, whose access tokens name
# the audience `ferroterm` when the request names none.
serve 18080 "$work/server.log" FERROTERM_RESOURCES="$work/resources.redb" \
  FERROTERM_AUTH__ENABLED=false
serve 18081 "$work/server-smart.log" FERROTERM_RESOURCES="$work/resources-smart.redb" \
  FERROTERM_AUTH__ISSUER="$issuer" FERROTERM_AUTH__AUDIENCE='["ferroterm"]'

console() {
  local port="$1" upstream="$2" log="$3"
  shift 3
  env FERROTERM_VIEWER__SERVER__LISTEN="127.0.0.1:$port" \
    FERROTERM_VIEWER__SERVER__SITE_ROOT=target/site \
    FERROTERM_VIEWER__FERROTERM__BASE_URL="$upstream" \
    FERROTERM_VIEWER__SESSION__SECURE_COOKIE=false "$@" \
    target/release/ferroterm-viewer serve > "$log" 2>&1 &
  pids+=("$!")
  await "http://127.0.0.1:$port/health" "$log"
}
console 13000 http://127.0.0.1:18080/ "$work/console.log"
console 13001 http://127.0.0.1:18081/ "$work/console-signed-in.log" \
  FERROTERM_VIEWER__OIDC__ISSUER="$issuer" \
  FERROTERM_VIEWER__OIDC__AUTHORIZATION_ENDPOINT="$issuer/authorize" \
  FERROTERM_VIEWER__OIDC__TOKEN_ENDPOINT="$issuer/token" \
  FERROTERM_VIEWER__OIDC__JWKS_URI="$issuer/jwks" \
  FERROTERM_VIEWER__OIDC__REVOCATION_ENDPOINT="$issuer/revoke" \
  FERROTERM_VIEWER__OIDC__CLIENT_ID=ferroterm-viewer \
  FERROTERM_VIEWER__OIDC__REDIRECT_URI=http://127.0.0.1:13001/auth/callback \
  FERROTERM_VIEWER__OIDC__SCOPES='["openid", "fhirUser", "user/CodeSystem.cud", "user/ValueSet.cud", "user/ConceptMap.cud"]'

"$chromedriver" --port=4444 > "$work/chromedriver.log" 2>&1 &
pids+=("$!")
await "http://127.0.0.1:4444/status" "$work/chromedriver.log"

journeys() {
  FERROTERM_UI_E2E_BASE_URL=http://127.0.0.1:13000 \
    FERROTERM_UI_E2E_SIGNED_IN_BASE_URL=http://127.0.0.1:13001 \
    FERROTERM_UI_E2E_ISSUER="$issuer" \
    FERROTERM_UI_E2E_WEBDRIVER=http://127.0.0.1:4444 \
    FERROTERM_UI_E2E_FAILURES="$FAILURES_DIR" \
    cargo nextest run --manifest-path e2e/Cargo.toml --locked \
    --test-threads "$BROWSER_SESSIONS" "$@"
}

if [[ -n "$review" ]]; then
  mkdir -p "$review"
  echo "ui-e2e: capturing every screen into $review"
  FERROTERM_UI_E2E_REVIEW_DIR="$(cd "$review" && pwd)" journeys -E 'test(/^docs_shots::/)'
  exit 0
fi
if [[ -n "$docs_shots" ]]; then
  echo "ui-e2e: capturing the documentation screenshots"
  FERROTERM_UI_E2E_DOCS_SHOTS=1 journeys -E 'test(/^docs_shots::/)'
  exit 0
fi
# The capture pass is excluded by a nextest set difference, so it never runs
# in an ordinary run.
journeys -E 'binary(it) - test(/^docs_shots::/)'
echo "ui-e2e: every journey passed"
