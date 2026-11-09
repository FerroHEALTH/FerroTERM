#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The performance gate of ferroterm-server and ferroterm-sync (#1151):
# ferrohealth-perf (FerroHEALTH/FerroSYS#249) in its A/B mode, the head of a
# change judged against its base measured beside it on the same runner, each
# service the release build of its own tree over its own data.
#
#   scripts/perf/gate.sh build [BASE]
#       Builds the services and the fixture writer of the checkout, the
#       head, and of its base: the merge base of HEAD with BASE, a commit or
#       ref, or with no BASE the newest v* tag before HEAD, else HEAD's first
#       parent. The base is built from `git archive` of that commit under
#       target/, with its own toolchain pin, into the same target directory,
#       so the dependencies build once. The base must define both services;
#       its fixture writer is built when its tree defines one (#1180).
#   scripts/perf/gate.sh run
#       Writes the scaled synthetic SNOMED CT edition with each side's
#       ferroterm-fixture, the head's for a base older than the writer,
#       starts each side's ferroterm over it and its own
#       e2e CodeSystem fixtures with the admin listener on (the head on 8080
#       and 8081, the base on 8090 and 8091), and each side's ferroterm-sync
#       with its next scheduled run twelve hours away (the head on 8181, the
#       base on 8191), and runs `ferrohealth-perf ab` over
#       perf/server/plan.toml and perf/sync/plan.toml: the head's idle phase,
#       then pairs of load phases side by side. Then it starts a head
#       ferroterm-sync whose schedule never waits and runs
#       perf/sync/busy-loop.toml against it, which must fail on idle CPU.
#       Exit 0 when all three hold, 1 when a gate fails or the busy service
#       passes, 2 when a run cannot be judged or set up.
#   scripts/perf/gate.sh --self-test
#       Proves the schedule time, the agreement of the sync plans' idle
#       limits, the base resolution and the base's binaries offline.
#
# Needs curl, git, jq and ferrohealth-perf on PATH, on Linux, where the tool
# reads CPU and memory from /proc. Neither service holds a database, so no
# transactions are counted. No specification governs this: our own design.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly WORK=target/perf-run
readonly BASE_TREE=target/perf-base
readonly BINARIES=(ferroterm ferroterm-sync ferroterm-fixture)
# The binaries the gate measures, which the base must define too.
readonly SERVICES=(ferroterm ferroterm-sync)
readonly FIXTURE=ferroterm-fixture
readonly HEAD_SERVER=http://127.0.0.1:8080
readonly BASE_SERVER=http://127.0.0.1:8090
readonly HEAD_SYNC=http://127.0.0.1:8181
readonly BASE_SYNC=http://127.0.0.1:8191
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

# base_rev [BASE] [AT]: the commit the base is built from, as `build` names
# it, resolved from AT (HEAD by default).
base_rev() {
  local base="${1:-}" at="${2:-HEAD}" tag
  # The commit before the push that created a branch is all zeros.
  [[ "$base" =~ ^0+$ ]] && base=""
  if [[ -n "$base" ]]; then
    git -C "$root" merge-base "$at" "$base"
  elif tag="$(git -C "$root" describe --tags --abbrev=0 --match 'v*' "$at^" 2> /dev/null)"; then
    git -C "$root" rev-parse "$tag^{commit}"
  else
    git -C "$root" rev-parse "$at^1"
  fi
}

# bin_targets: the names of the binary targets in the `cargo metadata` JSON
# on stdin, one per line, sorted.
bin_targets() {
  jq -r '.packages[].targets[] | select(.kind | index("bin")) | .name' | sort -u
}

# base_binaries: the binaries to build from a base whose binary targets are
# the lines on stdin, in BINARIES order. Both services or nothing: a base
# without one cannot be measured beside the head.
base_binaries() {
  local defined binary
  defined="$(cat)"
  for binary in "${SERVICES[@]}"; do
    grep -qxF "$binary" <<< "$defined" || die "the base defines no $binary binary, so it cannot be measured"
  done
  for binary in "${BINARIES[@]}"; do
    if grep -qxF "$binary" <<< "$defined"; then
      echo "$binary"
    fi
  done
}

# fixture_writer BASE_DIR HEAD_DIR: the fixture writer that writes the
# base's edition, the base's own when it has one, else the head's.
fixture_writer() {
  if [[ -x "$1/$FIXTURE" ]]; then
    echo "$1/$FIXTURE"
  else
    echo "$2/$FIXTURE"
  fi
}

# cargo_build BINARY... [-- CARGO_ARG...]: the release builds of BINARY of
# the tree in the working directory.
cargo_build() {
  local args=(--release --locked -p ferroterm-server -p ferroterm-sync)
  while [[ $# -gt 0 && "$1" != "--" ]]; do
    [[ "$1" != "$FIXTURE" ]] || args+=(-p ferroterm-testkit)
    args+=(--bin "$1")
    shift
  done
  [[ $# -eq 0 ]] || shift
  cargo build "${args[@]}" "$@"
}

# build [BASE]: the base's binaries into target/perf-base/, then the head's
# into target/release/.
build() {
  local rev binary binaries
  cd "$root"
  command -v jq > /dev/null || die "jq is not on PATH"
  rev="$(base_rev "${1:-}")" || die "the base of ${1:-the previous release} cannot be resolved"
  echo "perf-gate: the base is $rev"
  rm -rf "$BASE_TREE"
  mkdir -p "$BASE_TREE/src"
  git archive "$rev" | tar -x -C "$BASE_TREE/src"
  printf '%s\n' "$rev" > "$BASE_TREE/rev"
  # From inside the base's tree, so its own toolchain pin builds it.
  binaries="$(cd "$BASE_TREE/src" && cargo metadata --no-deps --locked --format-version 1 | bin_targets)" ||
    die "the base's binary targets cannot be read"
  binaries="$(base_binaries <<< "$binaries")"
  read -r -d '' -a binaries <<< "$binaries" || true
  if [[ " ${binaries[*]} " != *" $FIXTURE "* ]]; then
    echo "perf-gate: the base defines no $FIXTURE, so the head's writes its edition"
  fi
  (cd "$BASE_TREE/src" && cargo_build "${binaries[@]}" -- --target-dir "$root/target")
  for binary in "${binaries[@]}"; do
    cp "target/release/$binary" "$BASE_TREE/$binary"
  done
  cargo_build "${BINARIES[@]}"
}

pids=()
base_note=""
cleanup() {
  local pid
  for pid in "${pids[@]}"; do
    kill "$pid" 2> /dev/null || true
    wait "$pid" 2> /dev/null || true
  done
}

# start_sync NAME BIN LISTEN ADMIN [VAR=VALUE...]: starts BIN/ferroterm-sync
# over its own tree under $WORK/NAME against the server admin listener at
# ADMIN, and prints its process id.
start_sync() {
  local name="$1" bin="$2" listen="$3" admin="$4" dir
  shift 4
  dir="$WORK/$name"
  mkdir -p "$dir/index" "$dir/codesystems" "$dir/staging" "$dir/records" "$dir/state"
  env FERROTERM_SYNC__LISTEN="$listen" \
    FERROTERM_SYNC__SERVER_ADMIN_URL="$admin" \
    FERROTERM_SYNC__INDEX_ROOT="$dir/index" \
    FERROTERM_SYNC__RESOURCES="$dir/codesystems" \
    FERROTERM_SYNC__STAGING="$dir/staging" \
    FERROTERM_SYNC__RECORDS="$dir/records" \
    FERROTERM_SYNC__STATE="$dir/state" \
    FERROTERM_SYNC__AUTH__ENABLED=false \
    "$@" "$bin/ferroterm-sync" serve > "$dir.log" 2>&1 &
  echo $!
}

# start_server NAME BIN TREE PORT ADMIN WRITER: writes the scaled edition
# with WRITER and starts BIN/ferroterm over it and TREE's e2e CodeSystem
# fixtures under $WORK/NAME, and prints its process id.
start_server() {
  local name="$1" bin="$2" tree="$3" port="$4" admin="$5" writer="$6" dir
  dir="$WORK/$name"
  mkdir -p "$dir"
  "$writer" snomed-scaled "$dir/index/snomed" "$CONCEPTS" > "$dir/fixture.log"
  FERROTERM__USAGE_REPORT__ENABLED=false \
    FERROTERM__SERVER__LISTEN="127.0.0.1:$port" \
    FERROTERM__ADMIN__LISTEN="127.0.0.1:$admin" \
    FERROTERM__AUTH__ENABLED=false \
    FERROTERM__TERMINOLOGY__INDEX="$dir/index/snomed" \
    FERROTERM__TERMINOLOGY__CODE_SYSTEMS="$tree/e2e/fixtures/codesystems" \
    FERROTERM__TERMINOLOGY__RESOURCES="$dir/resources.redb" \
    "$bin/ferroterm" > "$dir.log" 2>&1 &
  echo $!
}

start() {
  local binary at writer
  for binary in "${BINARIES[@]}"; do
    [[ -x "target/release/$binary" ]] || die "target/release/$binary is not built"
  done
  for binary in "${SERVICES[@]}"; do
    [[ -x "$BASE_TREE/$binary" ]] || die "$BASE_TREE/$binary is not built: run gate.sh build first"
  done
  rm -rf "$WORK"
  mkdir -p "$WORK"

  writer="$(fixture_writer "$BASE_TREE" target/release)"
  if [[ "$writer" != "$BASE_TREE/$FIXTURE" ]]; then
    base_note="The base defines no \`$FIXTURE\`, so the head's writes its edition."
    echo "perf-gate: ${base_note//\`/}"
  fi
  head_server="$(start_server head-server target/release . 8080 8081 "target/release/$FIXTURE")"
  pids+=("$head_server")
  base_server="$(start_server base-server "$BASE_TREE" "$BASE_TREE/src" 8090 8091 "$writer")"
  pids+=("$base_server")
  wait_for "the head server" 120 curl --fail --silent "$HEAD_SERVER/health"
  wait_for "the base server" 120 curl --fail --silent "$BASE_SERVER/health"

  at="FERROTERM_SYNC__SCHEDULE__AT=$(twelve_hours_on "$(date -u +%H)")"
  head_sync="$(start_sync head-sync target/release 127.0.0.1:8181 http://127.0.0.1:8081 "$at")"
  pids+=("$head_sync")
  base_sync="$(start_sync base-sync "$BASE_TREE" 127.0.0.1:8191 http://127.0.0.1:8091 "$at")"
  pids+=("$base_sync")
  wait_for "the head sync service" 60 curl --fail --silent "$HEAD_SYNC/health/readiness"
  wait_for "the base sync service" 60 curl --fail --silent "$BASE_SYNC/health/readiness"
}

# gate NAME BASE_PID BASE_URL HEAD_PID HEAD_URL: runs the A/B mode of
# perf/NAME/plan.toml against the two sides.
gate() {
  ferrohealth-perf ab --plan "perf/$1/plan.toml" \
    --base-url "$3" --base-pid "$2" --head-url "$5" --head-pid "$4" \
    --out-dir "target/perf/$1"
}

# busy_loop: a head sync whose schedule never waits must fail the idle phase
# on its CPU.
busy_loop() {
  local code=0 summary="$WORK/busy-loop.md" breaches pid
  pid="$(start_sync busy target/release 127.0.0.1:8183 http://127.0.0.1:8081 FERROTERM_SYNC__SCHEDULE__EVERY=1ms)"
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
  [[ $# -eq 0 ]] || die "usage: gate.sh run"
  cd "$root"
  command -v ferrohealth-perf > /dev/null || die "ferrohealth-perf is not on PATH"
  trap cleanup EXIT
  start
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf "The base is \`%s\`. %s\n\n" "$(< "$BASE_TREE/rev")" "${base_note:-}" >> "$GITHUB_STEP_SUMMARY"
  fi
  local code=0 step
  for step in "server $base_server $BASE_SERVER $head_server $HEAD_SERVER" \
    "sync $base_sync $BASE_SYNC $head_sync $HEAD_SYNC"; do
    # shellcheck disable=SC2086 # the five words of the step
    gate $step || {
      step=$?
      [[ "$code" -eq 1 ]] || code="$step"
    }
  done
  busy_loop || { [[ "$code" -ne 0 ]] || code=1; }
  if [[ "$code" -ne 0 ]]; then
    echo "perf-gate: the server and sync logs follow" >&2
    tail -n 30 "$WORK/head-server.log" "$WORK/base-server.log" "$WORK/head-sync.log" "$WORK/base-sync.log" >&2
  fi
  exit "$code"
}

self_test() {
  local work n=0 failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now
  trap "rm -r -- '$work'" EXIT
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
  local repo="$work/repo" saved_root="$root"
  git init --quiet "$repo"
  commit() { git -C "$repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false -c tag.gpgsign=false commit --quiet --allow-empty -m "$1"; }
  commit one
  commit two
  git -C "$repo" -c tag.gpgsign=false tag v0.0.1
  commit three
  git -C "$repo" branch side
  commit four
  git -C "$repo" checkout --quiet side
  commit side-only
  git -C "$repo" checkout --quiet -
  root="$repo"
  check "with no BASE the base is the newest tag before HEAD" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev "")"
  check "a BASE names its merge base with HEAD" \
    "$(git -C "$repo" rev-parse HEAD~1)" "$(base_rev side)"
  check "an all-zero BASE takes the previous release" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev 0000000000000000000000000000000000000000)"
  check "with no tag before it the base is the first parent" \
    "$(git -C "$repo" rev-parse v0.0.1~1)" "$(base_rev "" v0.0.1)"
  root="$saved_root"
  local metadata='{"packages":[
    {"targets":[{"kind":["lib"],"name":"ferroterm_server"},{"kind":["bin"],"name":"ferroterm"}]},
    {"targets":[{"kind":["bin"],"name":"ferroterm-sync"},{"kind":["test"],"name":"it"}]},
    {"targets":[{"kind":["lib"],"name":"ferroterm_testkit"},{"kind":["bin"],"name":"ferroterm-fixture"}]}]}'
  check "the binary targets are read from cargo metadata" \
    "$(printf '%s\n' ferroterm ferroterm-fixture ferroterm-sync)" "$(bin_targets <<< "$metadata")"
  check "a base with the fixture writer builds all three" \
    "$(printf '%s\n' "${BINARIES[@]}")" "$(bin_targets <<< "$metadata" | base_binaries)"
  check "a base without the fixture writer builds the two services" \
    "$(printf '%s\n' ferroterm ferroterm-sync)" "$(printf '%s\n' ferroterm ferroterm-sync bench | base_binaries)"
  check "a base without the sync service cannot be measured" \
    "2" "$( (printf '%s\n' ferroterm ferroterm-fixture | base_binaries) > /dev/null 2>&1 && echo 0 || echo $?)"
  mkdir -p "$work/base" "$work/head"
  : > "$work/head/$FIXTURE"
  chmod +x "$work/head/$FIXTURE"
  check "a base without the fixture writer has its edition written by the head's" \
    "$work/head/$FIXTURE" "$(fixture_writer "$work/base" "$work/head")"
  : > "$work/base/$FIXTURE"
  chmod +x "$work/base/$FIXTURE"
  check "a base with the fixture writer writes its own edition" \
    "$work/base/$FIXTURE" "$(fixture_writer "$work/base" "$work/head")"
  [[ "$failed" -eq 0 ]] || exit 1
  echo "perf-gate: self-test OK ($n checks)."
}

case "${1:-}" in
  build)
    shift
    build "$@"
    ;;
  run)
    shift
    run "$@"
    ;;
  --self-test) self_test ;;
  *) die "usage: gate.sh build [BASE] | run | --self-test" ;;
esac
