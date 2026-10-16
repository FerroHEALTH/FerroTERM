#!/usr/bin/env bash
# The HL7 terminology ecosystem suite against a FerroTERM, with a committed
# pass list: a test on the list that fails is a regression, a test that
# newly passes is reported so it can be added.
#
#   scripts/checks/tx-ecosystem.sh [--server URL] [--out DIR] [--mode NAME] [--fhir r4|r4b|r5] [--index DIRS] [--port N]
#
# Without --server the script starts target/release/ferroterm on the first free
# port at or above 8098, or the one --port names
# (build it first: cargo build --release -p ferroterm-server), serving the
# artifact directories --index names (the FERROTERM_INDEX form). --mode picks
# the suite mode (general by default; icd-11 needs the three ICD-11 artifacts)
# and its pass list conformance/tx-ecosystem/passing[-<fhir>][-<mode>].txt; --fhir
# picks the served FHIR version (r4b by default; the runner reads the version
# from the server's CapabilityStatement). The validator
# jar and the suite are fetched into target/tx-ecosystem/ once and pinned by
# digest and commit. A JVM runs here only, never in the server.
set -euo pipefail
# target/ and conformance/ are named relative to the repository root, so run there.
cd "$(dirname "$0")/../.."

VALIDATOR_VERSION=6.10.4
VALIDATOR_SHA256=1106b9d58f9e363e47bea7c4fc065841e5fc91fe9d062775c3bfdd212bd653cc
SUITE_REPO=https://github.com/HL7/fhir-tx-ecosystem-ig
SUITE_COMMIT=eaec771d82fba4eac596c14963546f39b4ecffe7
# Seconds to wait for the started server to answer /health. Generous because it
# covers reading an artifact, and safe because the wait ends the moment the
# server exits.
READY_TIMEOUT=${FERROTERM_READY_TIMEOUT:-300}

server=""
port=""
out=target/tx-ecosystem/out
mode=general
fhir=r4b
index=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --server) server=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --mode) mode=$2; shift 2 ;;
    --index) index=$2; shift 2 ;;
    --fhir) fhir=$2; shift 2 ;;
    --port) port=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$fhir" in
  r4|r4b|r5) ;;
  *) echo "--fhir must be r4, r4b, or r5, not '$fhir'" >&2; exit 2 ;;
esac
PASSING=conformance/tx-ecosystem/passing
if [[ "$fhir" != r4b ]]; then
  PASSING=$PASSING-$fhir
fi
if [[ "$mode" != general ]]; then
  PASSING=$PASSING-$mode
fi
PASSING=$PASSING.txt

work=target/tx-ecosystem
mkdir -p "$work"
jar=$work/validator_cli-$VALIDATOR_VERSION.jar
if [[ ! -f "$jar" ]]; then
  echo "fetching validator_cli.jar $VALIDATOR_VERSION"
  curl --proto '=https' --tlsv1.2 -sSL -o "$jar.tmp" "https://github.com/hapifhir/org.hl7.fhir.core/releases/download/$VALIDATOR_VERSION/validator_cli.jar"
  mv "$jar.tmp" "$jar"
fi
echo "$VALIDATOR_SHA256  $jar" | shasum -a 256 -c - >/dev/null

suite=$work/suite
# A checkout counts only at the pinned commit and with the file the runner
# opens first. The build cache prunes target/ on save and can restore this
# directory as an empty skeleton, which a bare directory test accepts (#563).
if [[ "$(git -C "$suite" rev-parse --verify HEAD 2>/dev/null || true)" != "$SUITE_COMMIT" \
  || ! -f "$suite/tests/test-cases.json" ]]; then
  echo "fetching the suite at $SUITE_COMMIT"
  rm -rf "$suite"
  git init -q "$suite"
  git -C "$suite" remote add origin "$SUITE_REPO"
  git -C "$suite" sparse-checkout set tests
  git -C "$suite" fetch -q --depth 1 origin "$SUITE_COMMIT"
  git -C "$suite" checkout -q FETCH_HEAD
fi

# Whether anything is listening on a local port. The runner talks to whatever
# answers, so a port another process holds must never be measured (#425).
port_taken() {
  (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null || return 1
  exec 3<&-
  return 0
}

started=""
if [[ -z "$server" ]]; then
  if [[ -n "$port" ]]; then
    if port_taken "$port"; then
      echo "tx-ecosystem: port $port is already in use, so a run would measure another server" >&2
      exit 1
    fi
  else
    for candidate in $(seq 8098 8137); do
      if ! port_taken "$candidate"; then
        port=$candidate
        break
      fi
    done
    if [[ -z "$port" ]]; then
      echo "tx-ecosystem: no free port in 8098..8137" >&2
      exit 1
    fi
  fi
  server=http://127.0.0.1:$port/$fhir
  if [[ -n "$index" ]]; then
    export FERROTERM_INDEX="$index"
  fi
  FERROTERM_USAGE_REPORT=off FERROTERM_LISTEN=127.0.0.1:"$port" FERROTERM_LOG_FORMAT=json target/release/ferroterm > "$work/server.log" 2>&1 &
  started=$!
  trap 'kill "$started" 2>/dev/null || true; wait "$started" 2>/dev/null || true' EXIT
  ready=""
  # Reading an artifact takes as long as it takes: a LOINC release needs about
  # twenty seconds here, and a SNOMED edition more. The liveness check below is
  # what makes waiting this long safe, since a server that dies is noticed at
  # once rather than at the deadline.
  for _ in $(seq 1 "$((READY_TIMEOUT * 5))"); do
    # The child owning the port is what makes the answer ours. Without this a
    # bind failure left the probe talking to the squatter (#425).
    if ! kill -0 "$started" 2>/dev/null; then
      echo "tx-ecosystem: the server exited before it was ready on port $port" >&2
      cat "$work/server.log" >&2
      exit 1
    fi
    if curl -sf "http://127.0.0.1:$port/health" >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [[ -z "$ready" ]]; then
    echo "tx-ecosystem: the server did not answer /health on port $port within ${READY_TIMEOUT}s" >&2
    cat "$work/server.log" >&2
    exit 1
  fi
  echo "tx-ecosystem: serving on 127.0.0.1:$port (pid $started)"
  # A named artifact the server refused would leave its system unserved and
  # every case over it failing, which the pass list would then record as the
  # state of the world (#493).
  scripts/checks/served-artifacts.sh --server "$server" --index "$index"
fi

rm -rf "$out"
mkdir -p "$out"
# The runner exits non-zero while any test fails; the pass list decides here.
modes=(-mode "$mode")
if [[ "$mode" != general ]]; then
  modes=(-mode '!general' -mode "$mode")
fi
java -jar "$jar" txTests -tx "$server" -test-version "$suite/tests" "${modes[@]}" \
  -output "$out" -ssrf-protection-enabled=false > "$out/runner.log" 2>&1 || true
report=$out/report.json
if [[ ! -f "$report" ]]; then
  echo "the runner produced no report; see $out/runner.log" >&2
  tail -n 40 "$out/runner.log" >&2
  exit 1
fi

total=$(jq '(.test // []) | length' "$report")
if [[ "$total" = 0 ]]; then
  echo "the runner ran no test; see $out/runner.log and $work/server.log" >&2
  tail -n 20 "$out/runner.log" >&2
  tail -n 5 "$work/server.log" >&2 2>/dev/null || true
  exit 1
fi
# The committed total feeds the conformance badges (conformance-badges.sh); a
# suite bump that changes it is recorded in the same change. A mode run
# (icd-11, tx.fhir.org, ...) selects a subset, so only the general run is held
# to it.
committed_total=$(tr -d '[:space:]' < conformance/tx-ecosystem/total.txt)
if [[ "$mode" = general ]] && [[ "$committed_total" != "$total" ]]; then
  echo "tx-ecosystem: the suite ran $total tests but conformance/tx-ecosystem/total.txt says $committed_total; update it in this change" >&2
  exit 1
fi
jq -r '.test[] | select(.action[0].operation.result == "pass") | .name' "$report" | sort > "$out/passing.txt"
passed=$(wc -l < "$out/passing.txt" | tr -d ' ')
echo "tx-ecosystem: $passed of $total $mode tests pass on /$fhir ($(grep -a -o 'tests v[0-9.]*' "$out/runner.log" | head -1), runner $VALIDATOR_VERSION)"

regressions=$(comm -23 "$PASSING" "$out/passing.txt")
gains=$(comm -13 "$PASSING" "$out/passing.txt")
if [[ -n "$gains" ]]; then
  echo "newly passing (add them to $PASSING):"
  for name in $gains; do echo "  + $name"; done
fi
if [[ -n "$regressions" ]]; then
  echo "REGRESSION: on the pass list but failing now:" >&2
  for name in $regressions; do
    echo "  - $name" >&2
    jq -r --arg n "$name" '.test[] | select(.name == $n) | "    \(.action[0].operation.message // "-" | .[0:300])"' "$report" >&2
  done
  exit 1
fi
echo "tx-ecosystem: no regression against $PASSING"
