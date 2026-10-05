#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# SonarQube Cloud's open issues on the main branch as a SARIF 2.1.0 log, for
# GitHub code scanning (no specification governs this: our own design).
# SonarQube Cloud reports to code scanning itself only on Sonar's Enterprise
# plan and exports no SARIF, so .github/workflows/sonar.yml runs this after the
# scan on main and uploads the log the way scorecard.yml uploads Scorecard's.
#
#   SONAR_TOKEN=… scripts/sonar/sarif.sh <output.sarif>
#
# Reads the scanner's .scannerwork/report-task.txt, waits for the server-side
# analysis it names to finish, then pages through the main branch's open
# issues and writes one result per issue that sits in a file. The token goes
# to curl on stdin, never on its command line.
#
# Exit 0 with the log written. Exit 1 naming what failed: no report task, a
# failed or timed-out analysis, an API error, or more issues than the API
# serves.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly OUT="${1:?usage: scripts/sonar/sarif.sh <output.sarif>}"
readonly TASK_FILE=.scannerwork/report-task.txt
readonly PAGE_SIZE=500
# The Web API serves at most 10,000 issues per search: 20 pages of 500.
readonly MAX_PAGES=20
: "${SONAR_TOKEN:?sonar-sarif: SONAR_TOKEN is required}"

if [[ ! -f "$TASK_FILE" ]]; then
  echo "sonar-sarif: $TASK_FILE is missing; run this after the scanner." >&2
  exit 1
fi

prop() {
  local name="$1" file="$2"
  grep -m1 "^$name=" "$file" | cut -d= -f2- || true
}

server="$(prop serverUrl "$TASK_FILE")"
key="$(prop projectKey "$TASK_FILE")"
task="$(prop ceTaskId "$TASK_FILE")"
org="$(prop sonar.organization sonar-project.properties)"
for name in server key task org; do
  if [[ -z "${!name}" ]]; then
    echo "sonar-sarif: no $name in $TASK_FILE or sonar-project.properties." >&2
    exit 1
  fi
done

api() {
  local path="$1"
  printf 'header = "Authorization: Bearer %s"\n' "$SONAR_TOKEN" \
    | curl --fail --silent --show-error --retry 3 --user-agent ferroterm-ci \
      --config - "$server/api/$path"
}

# The scanner only submits the report; the issues exist once the server has
# processed it. Ten minutes covers the slowest analysis seen so far.
status=PENDING
for _ in $(seq 1 120); do
  status="$(api "ce/task?id=$task" | jq -r '.task.status')"
  case "$status" in
    SUCCESS) break ;;
    FAILED | CANCELED)
      echo "sonar-sarif: analysis $task ended $status." >&2
      exit 1
      ;;
    *) ;; # PENDING or IN_PROGRESS: the analysis is still queued or running.
  esac
  sleep 5
done
if [[ "$status" != "SUCCESS" ]]; then
  echo "sonar-sarif: analysis $task still $status after ten minutes." >&2
  exit 1
fi

issues="$(mktemp)"
trap 'rm -f "$issues"' EXIT
page=1
while :; do
  body="$(api "issues/search?componentKeys=$key&resolved=false&ps=$PAGE_SIZE&p=$page")"
  jq -c '.issues[]' <<<"$body" >>"$issues"
  total="$(jq -r '.paging.total' <<<"$body")"
  # [[ -ge ]] evaluates its operands as arithmetic, so the API's total is held
  # to digits first.
  if [[ ! "$total" =~ ^[0-9]+$ ]]; then
    echo "sonar-sarif: the issue search answered no numeric paging total." >&2
    exit 1
  fi
  if [[ $((page * PAGE_SIZE)) -ge "$total" ]]; then
    break
  fi
  if [[ "$page" -ge "$MAX_PAGES" ]]; then
    echo "sonar-sarif: $total open issues exceed the API's $((MAX_PAGES * PAGE_SIZE)); no partial log is written." >&2
    exit 1
  fi
  page=$((page + 1))
done

# Sonar's text offsets are 0-based with an exclusive end; SARIF columns are
# 1-based with an exclusive end, so both shift by one. An issue on the project
# itself names no file, and code scanning places a result only in a file.
jq -s --arg key "$key" --arg org "$org" '
  def level:
    if . == "BLOCKER" or . == "CRITICAL" then "error"
    elif . == "MAJOR" then "warning"
    else "note" end;
  def rule_uri:
    "https://sonarcloud.io/organizations/\($org)/rules?open=\(@uri)&rule_key=\(@uri)";
  [.[] | select(.component | startswith($key + ":"))] as $found
  | {
      "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
      version: "2.1.0",
      runs: [{
        tool: {driver: {
          name: "SonarQube Cloud",
          informationUri: "https://www.sonarsource.com/products/sonarqube/cloud/",
          rules: ($found | map(.rule) | unique | map({id: ., helpUri: rule_uri}))
        }},
        results: ($found | map({
          ruleId: .rule,
          level: ((.severity // "MAJOR") | level),
          message: {text: .message},
          locations: [{physicalLocation: {
            artifactLocation: {uri: (.component | ltrimstr($key + ":"))},
            region: (if .textRange then {
                startLine: .textRange.startLine,
                endLine: .textRange.endLine,
                startColumn: (.textRange.startOffset + 1),
                endColumn: (.textRange.endOffset + 1)
              } else {startLine: (.line // 1)} end)
          }}],
          properties: {tags: ["sonarqube", ((.type // "issue") | ascii_downcase)]}
        }))
      }]
    }' "$issues" >"$OUT"

echo "sonar-sarif: $(jq '.runs[0].results | length' "$OUT") results written to $OUT."
