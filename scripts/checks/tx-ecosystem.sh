#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The HL7 terminology ecosystem suite against a FerroTERM, with a committed
# pass list: a test on the list that fails is a regression, a test that
# newly passes is reported so it can be added.
#
#   scripts/checks/tx-ecosystem.sh [--server URL] [--out DIR] [--mode NAME] [--fhir r4|r4b|r5] [--index DIRS] [--port N] [--names-only]
#
# --names-only prints case names and counts and never a line of the runner's
# log, the server's log or a case's message, which can quote licensed content;
# a run over fetched content uses it, since a public workflow log is public.
# Without --server the script starts target/release/ferroterm on the first free
# port at or above 8098, or the one --port names
# (build it first: cargo build --release -p ferroterm-server), serving the
# comma-separated artifact directories --index names (the
# FERROTERM__TERMINOLOGY__INDEX form). --mode picks
# the suite mode (general by default; icd-11 needs the three ICD-11 artifacts)
# and its pass list conformance/tx-ecosystem/passing[-<fhir>][-<mode>].txt; --fhir
# picks the served FHIR version (r4b by default; the runner reads the version
# from the server's CapabilityStatement). The validator
# jar and the suite are fetched into target/tx-ecosystem/ once and pinned by
# digest and commit. Everything a run writes, the server's log included, goes
# under --out, so a run over licensed content leaves nothing of it in target/.
# A JVM runs here only, never in the server: $JAVA_HOME/bin/java when
# JAVA_HOME is set, otherwise the java on PATH, checked with -version before
# anything starts.
set -euo pipefail
# target/ and conformance/ are named relative to the repository root, so run there.
cd "$(dirname "$0")/../.."

VALIDATOR_VERSION=7.0.6
VALIDATOR_SHA256=2686590539f0b3ba7683149b25d78249f6886c04e2c326e884c5049d8947df89
SUITE_REPO=https://github.com/HL7/fhir-tx-ecosystem-ig
SUITE_COMMIT=9c2bbce57366319d6a5265cbfbf6697f094822e3
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
names_only=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --server) server=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --mode) mode=$2; shift 2 ;;
    --index) index=$2; shift 2 ;;
    --fhir) fhir=$2; shift 2 ;;
    --port) port=$2; shift 2 ;;
    --names-only) names_only=1; shift ;;
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

java=java
if [[ -n "${JAVA_HOME:-}" ]]; then
  java=$JAVA_HOME/bin/java
fi
if ! "$java" -version >/dev/null 2>&1; then
  echo "tx-ecosystem: no Java runtime answers '$java -version'; install one, or set JAVA_HOME to a JDK or JRE home" >&2
  exit 2
fi

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
# Bash connects on any redirection of a /dev/tcp path, and the subshell closes
# the socket as it exits.
# Prints the last lines of a log, or under --names-only only where it lies.
show_log() {
  local lines=$1 file=$2
  if [[ -n "$names_only" ]]; then
    echo "tx-ecosystem: $file is not printed under --names-only" >&2
  else
    tail -n "$lines" "$file" >&2 2>/dev/null || true
  fi
}

port_taken() {
  local probe=$1
  (: <"/dev/tcp/127.0.0.1/$probe") 2>/dev/null
}

rm -rf "$out"
mkdir -p "$out"

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
    export FERROTERM__TERMINOLOGY__INDEX="$index"
  fi
  # The snomed mode's expected displays are US English preferred terms, a
  # deployment's choice of language reference set (the book's conformance page
  # names it beside the count), so the run configures that dialect.
  if [[ "$mode" == snomed ]]; then
    export FERROTERM__TERMINOLOGY__DEFAULT_LANGUAGE=en-US
  fi
  # `$closure` keeps its tables in the resource database, so the run gives the
  # server a fresh one of its own and removes it at exit. The write routes it
  # opens need [auth] or its explicit absence; the run is local to this host.
  resources="$(mktemp -d "$out/resources.XXXXXX")"
  FERROTERM__TERMINOLOGY__RESOURCES="$resources/resources.redb" FERROTERM__AUTH__ENABLED=false FERROTERM__USAGE_REPORT__ENABLED=false FERROTERM__SERVER__LISTEN=127.0.0.1:"$port" FERROTERM__TELEMETRY__FORMAT=json target/release/ferroterm > "$out/server.log" 2>&1 &
  started=$!
  trap 'kill "$started" 2>/dev/null || true; wait "$started" 2>/dev/null || true; rm -rf "$resources"' EXIT
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
      show_log 200 "$out/server.log"
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
    show_log 200 "$out/server.log"
    exit 1
  fi
  echo "tx-ecosystem: serving on 127.0.0.1:$port (pid $started)"
  # A named artifact the server refused would leave its system unserved and
  # every case over it failing, which the pass list would then record as the
  # state of the world (#493).
  scripts/checks/served-artifacts.sh --server "$server" --index "$index"
fi

# The runner exits non-zero while any test fails; the pass list decides here.
modes=(-mode "$mode")
if [[ "$mode" != general ]]; then
  modes=(-mode '!general' -mode "$mode")
fi
"$java" -jar "$jar" txTests -tx "$server" -test-version "$suite/tests" "${modes[@]}" \
  -output "$out" -ssrf-protection-enabled=false > "$out/runner.log" 2>&1 || true
report=$out/report.json
if [[ ! -f "$report" ]]; then
  echo "tx-ecosystem: the runner produced no report; see $out/runner.log" >&2
  show_log 40 "$out/runner.log"
  exit 1
fi

total=$(jq '(.test // []) | length' "$report")
if [[ "$total" = 0 ]]; then
  echo "tx-ecosystem: the runner ran no test; see $out/runner.log and $out/server.log" >&2
  show_log 20 "$out/runner.log"
  show_log 5 "$out/server.log"
  exit 1
fi
# The committed total feeds the conformance badges (conformance-badges.sh); a
# suite bump that changes it is recorded in the same change. A mode run
# (snomed, icd-11, ...) selects a subset and is held to its row of
# ci-modes.tsv or hand-run.tsv at the end instead.
committed_total=$(tr -d '[:space:]' < conformance/tx-ecosystem/total.txt)
if [[ "$mode" = general ]] && [[ "$committed_total" != "$total" ]]; then
  echo "tx-ecosystem: the suite ran $total tests but conformance/tx-ecosystem/total.txt says $committed_total; update it in this change" >&2
  exit 1
fi
jq -r '.test[] | select(.action[0].operation.result == "pass") | .name' "$report" | sort > "$out/passing.txt"
passed=$(wc -l < "$out/passing.txt" | tr -d ' ')
# The suite's version is the one its publication-request.json names: the
# runner reports the last entry of tests/history.json, which the suite keeps
# frozen at 1.9.3 ("retained because the TxTester reads the current version").
suite_version=$(jq -er '.version' "$suite/publication-request.json")
echo "tx-ecosystem: $passed of $total $mode tests pass on /$fhir (test cases $suite_version at ${SUITE_COMMIT:0:7}, runner $VALIDATOR_VERSION)"

# The general-mode cases that do not apply on this surface are counted on the
# badge beside the pass count (#1361), so the inventory has to be the run's
# truth: the gated rows are exactly the cases the runner skipped, and a case
# held as an R4B artefact still runs and still fails.
if [[ "$mode" = general ]]; then
  inventory=conformance/tx-ecosystem/not-applicable.tsv
  jq -r '.test[] | select(.action[0].operation.result == "skip") | .name' "$report" | sort > "$out/skipped.txt"
  awk -F'\t' -v surface="$fhir" '!/^#/ && $1 == surface && $3 == "gated" { print $2 }' "$inventory" | sort > "$out/gated.txt"
  if ! diff -u "$out/gated.txt" "$out/skipped.txt" > "$out/gated.diff"; then
    echo "tx-ecosystem: the runner's skips on /$fhir differ from the gated rows of $inventory (- listed, + skipped):" >&2
    tail -n +3 "$out/gated.diff" >&2
    exit 1
  fi
  jq -r '.test[].name' "$report" | sort > "$out/ran.txt"
  awk -F'\t' -v surface="$fhir" '!/^#/ && $1 == surface && $3 == "r4b-surface" { print $2 }' "$inventory" | sort > "$out/surface.txt"
  missing=$(comm -23 "$out/surface.txt" "$out/ran.txt")
  passing_now=$(comm -12 "$out/surface.txt" "$out/passing.txt")
  if [[ -n "$missing" || -n "$passing_now" ]]; then
    echo "tx-ecosystem: $inventory holds /$fhir rows the run contradicts; drop them:" >&2
    for name in $missing; do echo "  not in the run: $name" >&2; done
    for name in $passing_now; do echo "  passes now: $name" >&2; done
    exit 1
  fi
  echo "tx-ecosystem: $(wc -l < "$out/skipped.txt" | tr -d ' ') gated and $(wc -l < "$out/surface.txt" | tr -d ' ') R4B-surface cases match $inventory"
fi

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
    [[ -z "$names_only" ]] || continue
    jq -r --arg n "$name" '.test[] | select(.name == $n) | "    \(.action[0].operation.message // "-" | .[0:300])"' "$report" >&2
  done
  exit 1
fi
echo "tx-ecosystem: no regression against $PASSING"

# A mode run is held to its row, the denominator of its badge
# (conformance-badges.sh): ci-modes.tsv for a mode CI runs, hand-run.tsv, with
# the day of the run, for one measured by hand.
if [[ "$mode" != general ]]; then
  for rows in conformance/tx-ecosystem/ci-modes.tsv conformance/tx-ecosystem/hand-run.tsv; do
    mode_total=$(awk -F'\t' -v s="$fhir" -v m="$mode" '!/^#/ && $1 == s && $2 == m { print $3 }' "$rows")
    [[ -z "$mode_total" ]] || break
  done
  if [[ "$mode_total" != "$total" ]]; then
    echo "tx-ecosystem: the $mode run on /$fhir ran $total tests but its row says ${mode_total:-nothing}; record the count in ci-modes.tsv, or in hand-run.tsv with the day of this run" >&2
    exit 1
  fi
fi
