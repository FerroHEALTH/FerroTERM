#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The performance gate of ferroterm-server and ferroterm-sync (#1151):
# ferrohealth-perf (FerroHEALTH/FerroSYS#249) against the release builds.
#
#   scripts/perf/gate.sh run [--update]
#       Writes the scaled synthetic SNOMED CT edition with ferroterm-fixture,
#       starts target/release/ferroterm over it and the e2e CodeSystem
#       fixtures with its admin listener on, and target/release/ferroterm-sync
#       with its next scheduled run twelve hours away, and runs
#       perf/server/plan.toml and perf/sync/plan.toml, each against its
#       baseline.toml. Then it starts a second ferroterm-sync whose schedule
#       never waits and runs perf/sync/busy-loop.toml against it, which must
#       fail on idle CPU. --update writes both baselines from runs that
#       pass. Exit 0 when all three hold, 1 when a gate fails or the busy
#       service passes, 2 when a run cannot be judged or set up.
#   scripts/perf/gate.sh --self-test
#       Proves the schedule time and the agreement of the sync plans' idle
#       limits offline.
#
# Needs curl and ferrohealth-perf on PATH, and the release builds of
# ferroterm-server, ferroterm-sync and ferroterm-testkit's ferroterm-fixture,
# on Linux, where the tool reads CPU and memory from /proc. Neither service
# holds a database, so no transactions are counted. No specification governs
# this: our own design.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly WORK=target/perf-run
readonly SERVER=http://127.0.0.1:8080
readonly SYNC=http://127.0.0.1:8181
readonly BUSY=http://127.0.0.1:8183
# The concepts of the scaled edition the server plan's codes come from.
readonly CONCEPTS=50000

die() {
  echo "perf-gate: $*" >&2
  exit 2
}

# idle_table PLAN: the [idle] table of PLAN, comments and blank lines off.
idle_table() {
  awk '/^\[/ { inside = ($0 == "[idle]") ; next }
       inside && !/^[[:space:]]*(#|$)/ { print }' "$1"
}

# twelve_hours_on HH: the time of day, as HH:00, twelve hours after the
# hour HH.
twelve_hours_on() {
  printf '%02d:00\n' "$(((10#$1 + 12) % 24))"
}

wait_for() {
  local what="$1" tries="$2"
  shift 2
  for _ in $(seq 1 "$tries"); do
    "$@" > /dev/null 2>&1 && return 0
    sleep 1
  done
  die "$what did not come up"
}

pids=()
cleanup() {
  local pid
  for pid in "${pids[@]}"; do
    kill "$pid" 2> /dev/null || true
    wait "$pid" 2> /dev/null || true
  done
}

# start_sync NAME LISTEN [VAR=VALUE...]: starts ferroterm-sync over its own tree
# under $WORK/NAME, and prints its process id.
start_sync() {
  local name="$1" listen="$2" dir
  shift 2
  dir="$WORK/$name"
  mkdir -p "$dir/index" "$dir/codesystems" "$dir/staging" "$dir/records" "$dir/state"
  env FERROTERM_SYNC__LISTEN="$listen" \
    FERROTERM_SYNC__SERVER_ADMIN_URL=http://127.0.0.1:8081 \
    FERROTERM_SYNC__INDEX_ROOT="$dir/index" \
    FERROTERM_SYNC__RESOURCES="$dir/codesystems" \
    FERROTERM_SYNC__STAGING="$dir/staging" \
    FERROTERM_SYNC__RECORDS="$dir/records" \
    FERROTERM_SYNC__STATE="$dir/state" \
    FERROTERM_SYNC__AUTH__ENABLED=false \
    "$@" target/release/ferroterm-sync serve > "$dir.log" 2>&1 &
  echo $!
}

start() {
  local binary pid
  for binary in ferroterm ferroterm-sync ferroterm-fixture; do
    [[ -x "target/release/$binary" ]] || die "target/release/$binary is not built"
  done
  rm -rf "$WORK"
  mkdir -p "$WORK"
  target/release/ferroterm-fixture snomed-scaled "$WORK/index/snomed" "$CONCEPTS"

  FERROTERM__USAGE_REPORT__ENABLED=false \
    FERROTERM__SERVER__LISTEN=127.0.0.1:8080 \
    FERROTERM__ADMIN__LISTEN=127.0.0.1:8081 \
    FERROTERM__AUTH__ENABLED=false \
    FERROTERM__TERMINOLOGY__INDEX="$WORK/index/snomed" \
    FERROTERM__TERMINOLOGY__CODE_SYSTEMS=e2e/fixtures/codesystems \
    FERROTERM__TERMINOLOGY__RESOURCES="$WORK/resources.redb" \
    target/release/ferroterm > "$WORK/server.log" 2>&1 &
  server_pid=$!
  pids+=("$server_pid")
  wait_for "the server" 120 curl --fail --silent "$SERVER/health"

  pid="$(start_sync sync 127.0.0.1:8181 FERROTERM_SYNC__SCHEDULE__AT="$(twelve_hours_on "$(date -u +%H)")")"
  sync_pid="$pid"
  pids+=("$sync_pid")
  wait_for "the sync service" 60 curl --fail --silent "$SYNC/health/readiness"
}

# gate NAME PID BASE [--update]: runs perf/NAME/plan.toml against PID.
gate() {
  local name="$1" pid="$2" base="$3"
  shift 3
  ferrohealth-perf run --plan "perf/$name/plan.toml" --pid "$pid" --base-url "$base" \
    --baseline "perf/$name/baseline.toml" --out-dir "target/perf/$name" "$@"
}

# busy_loop: a sync whose schedule never waits must fail the idle phase on
# its CPU.
busy_loop() {
  local code=0 summary="$WORK/busy-loop.md" breaches pid
  pid="$(start_sync busy 127.0.0.1:8183 FERROTERM_SYNC__SCHEDULE__EVERY=0s)"
  pids+=("$pid")
  wait_for "the busy sync service" 60 curl --fail --silent "$BUSY/health"
  GITHUB_STEP_SUMMARY='' ferrohealth-perf run --plan perf/sync/busy-loop.toml --pid "$pid" \
    --base-url "$BUSY" --baseline "$WORK/no-baseline.toml" \
    --out-dir target/perf/busy-loop --summary "$summary" > /dev/null || code=$?
  kill "$pid" 2> /dev/null || true
  breaches="$(awk '/^### Breaches/ { on = 1; next } /^### / { on = 0 } on' "$summary")"
  if [[ "$code" -eq 1 ]] && grep -qF 'idle.cpu_percent' <<< "$breaches"; then
    echo "perf-gate: the sync service that never waits fails the idle phase on its CPU"
    report_busy_loop "failed the gate, as it must" "$summary"
    return 0
  fi
  echo "perf-gate: the sync service that never waits was not failed on idle CPU (exit $code)" >&2
  cat "$summary" >&2
  report_busy_loop "was NOT failed on idle CPU (exit $code)" "$summary"
  return 1
}

report_busy_loop() {
  [[ -n "${GITHUB_STEP_SUMMARY:-}" ]] || return 0
  {
    printf '## The planted idle busy-loop\n\nA sync service whose schedule never waits %s.\n\n' "$1"
    awk '/^### Breaches/ { on = 1 } /^### Checks/ { on = 0 } on' "$2"
  } >> "$GITHUB_STEP_SUMMARY"
}

run() {
  local update=()
  case "${1:-}" in
    --update) update=(--update) ;;
    "") ;;
    *) die "usage: gate.sh run [--update]" ;;
  esac
  cd "$root"
  command -v ferrohealth-perf > /dev/null || die "ferrohealth-perf is not on PATH"
  trap cleanup EXIT
  start
  local code=0 step
  for step in "server $server_pid $SERVER" "sync $sync_pid $SYNC"; do
    # shellcheck disable=SC2086 # the three words of the step
    gate $step ${update[@]+"${update[@]}"} || {
      step=$?
      [[ "$code" -eq 1 ]] || code="$step"
    }
  done
  busy_loop || { [[ "$code" -ne 0 ]] || code=1; }
  if [[ "$code" -ne 0 ]]; then
    echo "perf-gate: the server and sync logs follow" >&2
    tail -n 30 "$WORK/server.log" "$WORK/sync.log" >&2
  fi
  exit "$code"
}

self_test() {
  local n=0 failed=0
  check() {
    n=$((n + 1))
    if [[ "$2" != "$3" ]]; then
      echo "  FAIL: $1: expected [$2], got [$3]" >&2
      failed=1
    fi
  }
  check "the morning turns to the evening" "21:00" "$(twelve_hours_on 09)"
  check "the evening wraps past midnight" "06:00" "$(twelve_hours_on 18)"
  check "midnight turns to noon" "12:00" "$(twelve_hours_on 00)"
  check "the sync plan and the busy-loop plan hold the same idle limits" \
    "$(idle_table "$root/perf/sync/plan.toml")" "$(idle_table "$root/perf/sync/busy-loop.toml")"
  [[ -n "$(idle_table "$root/perf/sync/plan.toml")" ]] || {
    echo "  FAIL: perf/sync/plan.toml holds no [idle] table" >&2
    failed=1
  }
  [[ "$failed" -eq 0 ]] || exit 1
  echo "perf-gate: self-test OK ($n checks)."
}

case "${1:-}" in
  run)
    shift
    run "$@"
    ;;
  --self-test) self_test ;;
  *) die "usage: gate.sh run [--update] | --self-test" ;;
esac
