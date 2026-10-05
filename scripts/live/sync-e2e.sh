#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# One end-to-end run of the synchronisation on this machine, with YOUR NTS
# account: the server starts on an empty index root with its admin listener,
# ferroterm-sync runs once against the Nictiz Nationale Terminologieserver,
# the server reloads, and the report shows what it serves afterwards and what
# the run record says about every entry the feed offered. This is the first
# real run #602 asks for.
#
# Local only, by design: it reads a licensed feed with a personal account, so
# it refuses to run under CI and no workflow calls it. The binaries are built
# from this checkout and run natively; no image is needed. Everything the run
# writes lands under one work directory (default: a fresh temporary one) that
# holds licensed content and stays out of the repository.
#
# Usage: scripts/live/sync-e2e.sh [--work DIR] [--systems URI,URI,...] [--keep]
#   --work DIR     the work directory (index root, managed resources, staging,
#                  records, state, logs); default mktemp
#   --systems ...  the canonicals to subscribe to; default the NL edition of
#                  SNOMED CT, LOINC, UCUM, and ICD-10
#   --keep         keep the work directory when it was a temporary one
# Credentials: FERROTERM_NTS_USERNAME and FERROTERM_NTS_PASSWORD, or
# FERROTERM_NTS_CLIENT_ID and FERROTERM_NTS_CLIENT_SECRET, as the add-on reads
# them. Requires: cargo, curl, jq, python3, and a free port 8080 and 8081.
#
# The FerroEHR hop comes after this: point its quickstart overlay
# (docker-compose.terminology.yml, [terminology.external]) at
# http://<this host>:8080/r4b and commit a composition whose binding names a
# code this run made available.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ -n "${CI:-}" ] || [ -n "${GITHUB_ACTIONS:-}" ]; then
  echo "sync-e2e: this run reads a licensed feed with a personal account and never runs under CI" >&2
  exit 2
fi

work=""
systems="http://snomed.info/sct/11000146104,http://loinc.org,http://unitsofmeasure.org,http://hl7.org/fhir/sid/icd-10"
keep=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --work) work="${2:?--work needs a directory}"; shift 2 ;;
    --systems) systems="${2:?--systems needs a list}"; shift 2 ;;
    --keep) keep=1; shift ;;
    -h|--help) sed -n '4,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "usage: $0 [--work DIR] [--systems URI,URI,...] [--keep]" >&2; exit 2 ;;
  esac
done

for tool in cargo curl jq python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "sync-e2e: missing required tool: $tool" >&2; exit 2; }
done
if [ -z "${FERROTERM_NTS_CLIENT_SECRET:-}" ] && { [ -z "${FERROTERM_NTS_USERNAME:-}" ] || [ -z "${FERROTERM_NTS_PASSWORD:-}" ]; }; then
  echo "sync-e2e: set FERROTERM_NTS_USERNAME and FERROTERM_NTS_PASSWORD (or FERROTERM_NTS_CLIENT_ID and FERROTERM_NTS_CLIENT_SECRET)" >&2
  exit 2
fi

temporary=0
if [ -z "$work" ]; then
  work="$(mktemp -d)"
  temporary=1
fi
mkdir -p "$work/index" "$work/codesystems" "$work/staging" "$work/records" "$work/state"
echo "== work directory: $work"

echo "== building ferroterm, ferroterm-build, ferroterm-sync (release)"
cargo build --release --locked -p ferroterm-server -p ferroterm-build -p ferroterm-sync >"$work/build.log" 2>&1 ||
  { tail -20 "$work/build.log" >&2; echo "sync-e2e: the build failed, see $work/build.log" >&2; exit 1; }
bin="$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/release"

server_pid=""
cleanup() {
  if [ -n "$server_pid" ] && kill -0 "$server_pid" 2>/dev/null; then
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [ "$temporary" -eq 1 ] && [ "$keep" -eq 0 ]; then
    rm -rf "$work"
  else
    echo "== kept: $work (licensed content, never commit it)"
  fi
}
trap cleanup EXIT

echo "== starting the server on 127.0.0.1:8080 (admin 127.0.0.1:8081) over the empty root"
FERROTERM_INDEX="$work/index" \
FERROTERM_CODESYSTEMS="$work/codesystems" \
FERROTERM_LISTEN=127.0.0.1:8080 \
FERROTERM_ADMIN_LISTEN=127.0.0.1:8081 \
FERROTERM_LOG_FORMAT=json \
  "$bin/ferroterm" >"$work/server.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 60); do
  if curl -sf http://127.0.0.1:8080/health >/dev/null 2>&1; then break; fi
  sleep 1
done
curl -sf http://127.0.0.1:8080/health >/dev/null || { tail -20 "$work/server.log" >&2; echo "sync-e2e: the server did not come up" >&2; exit 1; }

# The configuration carries no credential; the add-on reads them from the
# environment this script inherited.
list="$(printf '%s' "$systems" | python3 -c 'import sys; print(", ".join(f"\"{s.strip()}\"" for s in sys.stdin.read().split(",") if s.strip()))')"
cat >"$work/sync.toml" <<TOML
listen = "127.0.0.1:8181"
server_admin_url = "http://127.0.0.1:8081"
fhir_base_url = "http://127.0.0.1:8080/r4b"
index_root = "$work/index"
resources = "$work/codesystems"
staging = "$work/staging"
records = "$work/records"
state = "$work/state"
retention = 2
activation = "auto"
build_command = "$bin/ferroterm-build"

[[source]]
kind = "nts"

[source.config]
systems = [$list]
TOML

echo "== one run against the NTS (this downloads and builds; a SNOMED CT edition takes minutes)"
"$bin/ferroterm-sync" --config "$work/sync.toml" run-once 2>&1 | tee "$work/sync.log" | grep -v '"password"\|"client_secret"' | tail -40

echo "== the run record"
record="$(find "$work/records" -name '*.json' -type f -print0 2>/dev/null | xargs -0 ls -t 2>/dev/null | head -1 || true)"
if [ -n "$record" ]; then
  jq '{outcome, duration_ms, entries_taken, errors, sources, activation, retention, revalidation}' "$record"
else
  echo "   no record written" >&2
fi

echo "== what the server serves now (metadata?mode=terminology)"
curl -sf 'http://127.0.0.1:8080/r4b/metadata?mode=terminology' |
  jq -r '.codeSystem[]? | "\(.uri) \(.version[]?.code // "")"' | sort | uniq
echo "ok: sync-e2e (one run completed; the FerroEHR hop is described at the top of this script)"
