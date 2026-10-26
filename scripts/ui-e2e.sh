#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The console's browser journeys: headless Chrome, driven over WebDriver
# (https://www.w3.org/TR/webdriver2/), against the console and the server
# this checkout builds, all on loopback.
#
#   scripts/ui-e2e.sh [-- NEXTEST_ARGS...]
#   scripts/ui-e2e.sh --docs-shots [-- NEXTEST_ARGS...]
#   scripts/ui-e2e.sh --review DIR [-- NEXTEST_ARGS...]
#   scripts/ui-e2e.sh --review DIR --pseudo-locale [-- NEXTEST_ARGS...]
#
# Whatever follows `--` goes to `cargo nextest run` after the run's own
# filter, so a name filter narrows the battery to one journey and
# `--stress-count` runs it in a loop:
#
#   scripts/ui-e2e.sh -- composer::the_preview_and_the_save_are_reached_from_the_keyboard --stress-count 20
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
#   18090  the stub sync listener (e2e/src/bin/stub_sync.rs), the
#          signed-in console's synchronisation service
#   13000  the console over the first server, offering no sign-in, started
#          with an empty configuration file, which Configuration names
#   13001  the console over the second, signing a reader in at the issuer
#   4444   chromedriver
#
# Every address is loopback, so the console's cookies may go without
# `Secure` (the console refuses that anywhere else) and no certificate is
# needed. The browser is Chrome for Testing with its chromedriver, named by
# FERROTERM_UI_E2E_CHROME and FERROTERM_UI_E2E_CHROMEDRIVER, or found on PATH;
# CI installs both at the release docs/VERSIONS.md pins. Without either, the
# script fetches that release into target/journey-tools, and the site build's
# wasm-opt into target/console-tools, so a fresh checkout runs
# with nothing installed by hand beyond cargo-nextest and cargo-leptos.
#
# The battery must not change a tracked file. The documentation capture pass
# is the one exception, and it runs alone when --docs-shots asks for it: it
# writes one PNG per screen the book embeds into
# website/book/src/operate/img/console (FERROTERM_UI_E2E_SHOTS_DIR sends it
# elsewhere). --review DIR runs only the capture, writing every screen at
# desktop and phone width in both themes, and the hover, focus and open
# states, into DIR for a visual review. With --pseudo-locale the console is
# built with one more catalog, `en-XA`, which scripts/pseudo-locale.sh
# writes from the English one with every message 40 percent longer, and the
# review draws every screen in it, so the capture shows where a layout has no
# room for a longer translation. The catalog is written into the console's
# `locales/` for the build alone (git ignores it) and deleted when the run
# ends.
#
# The image the release lane builds is not what the journeys drive; CI builds
# docker/console/Dockerfile from a staged tree on its own (no specification
# governs this: our own design).
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

docs_shots=""
review=""
pseudo_locale=""
nextest_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --)
      shift
      nextest_args=("$@")
      break
      ;;
    --docs-shots)
      docs_shots=1
      shift
      ;;
    --review)
      [[ $# -ge 2 ]] || { echo "usage: scripts/ui-e2e.sh --review DIR [-- NEXTEST_ARGS...]" >&2; exit 2; }
      review="$2"
      shift 2
      ;;
    --pseudo-locale)
      pseudo_locale=1
      shift
      ;;
    *)
      echo "usage: scripts/ui-e2e.sh [--docs-shots | --review DIR [--pseudo-locale]] [-- NEXTEST_ARGS...]" >&2
      exit 2
      ;;
  esac
done

die() {
  echo "ui-e2e: $*" >&2
  exit 1
}

if [[ -n "$pseudo_locale" && -z "$review" ]]; then
  echo "usage: --pseudo-locale draws the --review capture, so it needs --review DIR" >&2
  exit 2
fi

need() {
  local tool="$1" install="$2"
  command -v "$tool" > /dev/null || die "$tool is not on PATH; $install"
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

# The site build's tool, wasm-opt: the pinned one on PATH, else the one in
# target/console-tools, fetched there on first use.
if ! bash scripts/release/console-tools.sh --check > /dev/null 2>&1; then
  tools="$root/target/console-tools"
  export PATH="$tools/bin:$PATH"
  if ! bash scripts/release/console-tools.sh --check > /dev/null 2>&1; then
    echo "ui-e2e: the pinned wasm-opt is not on PATH; fetching it into $tools"
    bash scripts/release/console-tools.sh "$tools"
  fi
fi

# How many journeys drive the browser at once: each opens its own Chrome under
# the one chromedriver, and more sessions than the processors (four on the
# ubuntu-latest runner) starve the browsers into timeouts. The same number is
# the test-thread count below.
readonly BROWSER_SESSIONS=4

work="$(mktemp -d)"
readonly PSEUDO_TAG=en-XA
readonly PSEUDO_CATALOG="$root/app/ferroterm-console/locales/$PSEUDO_TAG/ferroterm-console.ftl"
FAILURES_DIR="${FERROTERM_UI_E2E_FAILURES:-$root/target/ui-e2e-failures}"
mkdir -p "$FAILURES_DIR"
pids=()

# Prints the process ids of every process below PID, deepest first: the
# browsers chromedriver started, and the helpers each browser started.
descendants() {
  local parent="$1" child
  for child in $(pgrep -P "$parent" 2> /dev/null || true); do
    descendants "$child"
    echo "$child"
  done
}

# Stops everything this run started, by the process id it recorded, and the
# browsers below them first, so no Chrome outlives the run to hold a port the
# next run's sessions need. A failing run first keeps the logs of the servers,
# the consoles, the issuer and chromedriver in FAILURES_DIR/logs, once every
# process has stopped and written its last line, so the failure artifact
# carries what the processes said beside what the browser saw.
cleanup() {
  local status=$? pid child log
  for pid in ${pids[@]+"${pids[@]}"}; do
    for child in $(descendants "$pid"); do
      kill "$child" 2> /dev/null || true
    done
    kill "$pid" 2> /dev/null || true
  done
  for pid in ${pids[@]+"${pids[@]}"}; do
    wait "$pid" 2> /dev/null || true
  done
  if [[ "$status" -ne 0 ]]; then
    for log in "$work"/*.log; do
      [[ -f "$log" ]] || continue
      mkdir -p "$FAILURES_DIR/logs"
      cp -- "$log" "$FAILURES_DIR/logs/" || true
    done
    if [[ -d "$FAILURES_DIR/logs" ]]; then
      echo "ui-e2e: the run failed; its process logs are in $FAILURES_DIR/logs" >&2
    fi
  fi
  rm -f -- "$work"/*.log "$work"/*.redb
  rmdir -- "$work" 2> /dev/null || true
  if [[ -n "$pseudo_locale" ]]; then
    rm -f -- "$PSEUDO_CATALOG"
    rmdir -- "$(dirname "$PSEUDO_CATALOG")" 2> /dev/null || true
    touch "$root/app/ferroterm-console/src/locale.rs"
  fi
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
for port in 18080 18081 18443 18090 13000 13001 4444; do
  if curl --silent --output /dev/null "http://127.0.0.1:$port/"; then
    die "127.0.0.1:$port already answers; stop whatever holds it first"
  fi
done

# The console's two halves are built the way the release lane builds them:
# a debug build of cargo-leptos's front end is compiled with a cfg the plain
# server build is not, and two halves built apart that way do not hydrate.
if [[ -n "$pseudo_locale" ]]; then
  echo "ui-e2e: writing the pseudo-locale catalog $PSEUDO_TAG"
  mkdir -p "$(dirname "$PSEUDO_CATALOG")"
  bash scripts/pseudo-locale.sh app/ferroterm-console/locales/en/ferroterm-console.ftl "$PSEUDO_CATALOG"
  # The catalogs are embedded when locale.rs compiles, and cargo tracks the
  # files it embedded, not the directory, so a new catalog needs a rebuild.
  touch app/ferroterm-console/src/locale.rs
fi
echo "ui-e2e: building the server, the console and the stub issuer"
cargo build --locked -p ferroterm-server
cargo build --locked --release -p ferroterm-console
cargo build --locked --manifest-path e2e/Cargo.toml --bin stub-issuer --bin stub-sync
echo "ui-e2e: building the console's site bundle"
bash scripts/release/console-site.sh --release

issuer="http://127.0.0.1:18443"
e2e/target/debug/stub-issuer --plain --listen 127.0.0.1:18443 --issuer "$issuer" \
  --ca "$work/ca.pem" > "$work/issuer.log" 2>&1 &
pids+=("$!")
await "$issuer/.well-known/openid-configuration" "$work/issuer.log"

e2e/target/debug/stub-sync --listen 127.0.0.1:18090 > "$work/sync.log" 2>&1 &
pids+=("$!")
await "http://127.0.0.1:18090/health" "$work/sync.log"

serve() {
  local port="$1" log="$2"
  shift 2
  env FERROTERM__USAGE_REPORT__ENABLED=false FERROTERM__SERVER__LISTEN="127.0.0.1:$port" \
    FERROTERM__TERMINOLOGY__CODE_SYSTEMS=e2e/fixtures/codesystems "$@" \
    target/debug/ferroterm > "$log" 2>&1 &
  pids+=("$!")
  await "http://127.0.0.1:$port/health" "$log"
}
# The anonymous console writes through a server with [auth] off; the signed-in
# one through a server that trusts the stub issuer, whose access tokens name
# the audience `ferroterm` when the request names none.
serve 18080 "$work/server.log" FERROTERM__TERMINOLOGY__RESOURCES="$work/resources.redb" \
  FERROTERM__AUTH__ENABLED=false
serve 18081 "$work/server-smart.log" FERROTERM__TERMINOLOGY__RESOURCES="$work/resources-smart.redb" \
  FERROTERM__AUTH__ISSUER="$issuer" FERROTERM__AUTH__AUDIENCE='["ferroterm"]'

console() {
  local port="$1" upstream="$2" log="$3"
  shift 3
  env FERROTERM_CONSOLE__SERVER__LISTEN="127.0.0.1:$port" \
    FERROTERM_CONSOLE__SERVER__SITE_ROOT=target/site \
    FERROTERM_CONSOLE__FERROTERM__BASE_URL="$upstream" \
    FERROTERM_CONSOLE__SESSION__SECURE_COOKIE=false "$@" \
    target/release/ferroterm-console serve > "$log" 2>&1 &
  pids+=("$!")
  await "http://127.0.0.1:$port/health" "$log"
}
: > "$work/console.toml"
console 13000 http://127.0.0.1:18080/ "$work/console.log" \
  FERROTERM_CONSOLE_CONFIG="$work/console.toml"
console 13001 http://127.0.0.1:18081/ "$work/console-signed-in.log" \
  FERROTERM_CONSOLE__OIDC__ISSUER="$issuer" \
  FERROTERM_CONSOLE__OIDC__AUTHORIZATION_ENDPOINT="$issuer/authorize" \
  FERROTERM_CONSOLE__OIDC__TOKEN_ENDPOINT="$issuer/token" \
  FERROTERM_CONSOLE__OIDC__JWKS_URI="$issuer/jwks" \
  FERROTERM_CONSOLE__OIDC__REVOCATION_ENDPOINT="$issuer/revoke" \
  FERROTERM_CONSOLE__OIDC__CLIENT_ID=ferroterm-console \
  FERROTERM_CONSOLE__OIDC__REDIRECT_URI=http://127.0.0.1:13001/auth/callback \
  FERROTERM_CONSOLE__OIDC__SCOPES='["openid", "fhirUser", "user/CodeSystem.cud", "user/ValueSet.cud", "user/ConceptMap.cud", "__ferroterm.sync"]' \
  FERROTERM_CONSOLE__FERROTERM__SYNC_URL=http://127.0.0.1:18090/

"$chromedriver" --port=4444 > "$work/chromedriver.log" 2>&1 &
pids+=("$!")
await "http://127.0.0.1:4444/status" "$work/chromedriver.log"

# One session opened and closed before any journey runs: a driver that cannot
# start a browser (a Chrome left from an earlier run holding what the new one
# needs, a browser the driver does not match) fails here with the driver's own
# words, instead of as every journey's timeout.
capabilities='{"capabilities":{"alwaysMatch":{"browserName":"chrome","goog:chromeOptions":{"args":["--headless=new","--no-sandbox","--disable-dev-shm-usage"]}}}}'
if [[ -n "${FERROTERM_UI_E2E_CHROME:-}" ]]; then
  capabilities="$(jq -c --arg binary "$FERROTERM_UI_E2E_CHROME" \
    '.capabilities.alwaysMatch["goog:chromeOptions"].binary = $binary' <<< "$capabilities")"
fi
opened="$(curl --silent --show-error --max-time 60 -H 'Content-Type: application/json' \
  -d "$capabilities" http://127.0.0.1:4444/session || true)"
session="$(jq -r '.value.sessionId // empty' <<< "$opened" 2> /dev/null || true)"
if [[ -z "$session" ]]; then
  reason="$(jq -r '.value.message // empty' <<< "$opened" 2> /dev/null || true)"
  die "chromedriver could not start a browser session: ${reason:-no answer}; a browser or driver left from an earlier run may still be running"
fi
curl --silent --output /dev/null -X DELETE "http://127.0.0.1:4444/session/$session" || true

journeys() {
  FERROTERM_UI_E2E_BASE_URL=http://127.0.0.1:13000 \
    FERROTERM_UI_E2E_SIGNED_IN_BASE_URL=http://127.0.0.1:13001 \
    FERROTERM_UI_E2E_ISSUER="$issuer" \
    FERROTERM_UI_E2E_WEBDRIVER=http://127.0.0.1:4444 \
    FERROTERM_UI_E2E_FAILURES="$FAILURES_DIR" \
    cargo nextest run --manifest-path e2e/Cargo.toml --locked \
    --test-threads "$BROWSER_SESSIONS" "$@" ${nextest_args[@]+"${nextest_args[@]}"}
}

if [[ -n "$review" ]]; then
  mkdir -p "$review"
  echo "ui-e2e: capturing every screen into $review"
  FERROTERM_UI_E2E_REVIEW_DIR="$(cd "$review" && pwd)" \
    FERROTERM_UI_E2E_REVIEW_LANGUAGE="${pseudo_locale:+$PSEUDO_TAG}" \
    journeys -E 'test(/^docs_shots::/)'
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
