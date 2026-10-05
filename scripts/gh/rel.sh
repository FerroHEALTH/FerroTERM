#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# scripts/gh/rel.sh: the deterministic GitHub issue-relationship helper.
#
# WHY THIS EXISTS: `gh` has NO native subcommand for sub-issues or issue
# dependencies (verified against gh 2.88.1), so relationships can only be set
# through `gh api`. Worse, every WRITE endpoint takes the target issue's
# DATABASE id, not its #number, a foot-gun that makes hand-typed `gh api`
# calls easy to get wrong. This wrapper resolves #number -> database id for
# you and calls the one correct endpoint, so every relationship command is
# consistent, typed correctly, and fails loud on a bad number.
#
# Reads are #number-keyed and need no id resolution; writes go through here.
#
# Official docs (durable references, the ONLY citations allowed for this):
#   Sub-issues API ...... https://docs.github.com/en/rest/issues/sub-issues
#   Dependencies API .... https://docs.github.com/en/rest/issues/issue-dependencies
#   Sub-issues (concept)  https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-sub-issues
#   Dependencies (concept) https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-issue-dependencies
#
# Policy, WHEN to use each relationship: no specification governs it: our own
# design.
#
# Limits (from the docs above): <=100 sub-issues per parent, <=8 nesting
# levels, one parent per issue (use --replace to move it); <=50 issues per
# dependency direction.
#
# Usage:
#   scripts/gh/rel.sh parent     <child> <parent> [--replace]  # child -> sub-issue of parent
#   scripts/gh/rel.sh unparent   <child>                       # detach child from its parent
#   scripts/gh/rel.sh blocked-by <n> <blocker>                # n is blocked by blocker
#   scripts/gh/rel.sh unblock    <n> <blocker>                # remove "n blocked-by blocker"
#   scripts/gh/rel.sh blocking   <n> <blocked>                # n blocks blocked
#   scripts/gh/rel.sh unblocking <n> <blocked>                # remove "n blocking blocked"
#   scripts/gh/rel.sh tree       <n>                          # print every relationship of n
#   scripts/gh/rel.sh id         <n>                          # print the database id of n
#   scripts/gh/rel.sh --self-test
#       Drives this program against a stub gh on PATH: each write resolves
#       the database id and calls its one endpoint, and every usage error
#       touches nothing.
#   No argument, an unknown command, a wrong operand count or a flag other
#   than --replace after `parent`, --help included, prints this usage and
#   exits 2 before any gh call.
#
# All commands act on the current repository (`gh repo view`); default
# FerroHEALTH/FerroTERM when run inside its clone.

set -euo pipefail

die() {
  echo "gh-rel: $*" >&2
  exit 1
}

usage() {
  sed -n '/^# Usage:/,/^$/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

need_int() {
  case "${1:-}" in
    '' | *[!0-9]*) die "expected an issue number, got '${1:-}'" ;;
    *) ;;
  esac
}

# The repository every call acts on, resolved by preflight once the command
# line has been read, so a usage error never reaches gh.
REPO=""

preflight() {
  command -v gh >/dev/null 2>&1 || die "the GitHub CLI (gh) is not installed"
  REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null)" ||
    die "could not resolve the current repository (run inside a gh-authenticated clone)"
}

# Resolve an issue #number to its database id (the value every write endpoint
# wants). Fails loud if the issue does not exist.
dbid() {
  local number="$1" id
  need_int "$number"
  id="$(gh api "repos/$REPO/issues/$number" --jq '.id' 2>/dev/null)" ||
    die "issue #$number not found in $REPO"
  case "$id" in
    '' | *[!0-9]*) die "could not resolve the database id for #$number" ;;
    *) ;;
  esac
  printf '%s' "$id"
}

# Print a jq-formatted list from an endpoint, or "    —" when empty/absent.
list_or_dash() {
  local endpoint="$1" filter="$2" out
  out="$(gh api "$endpoint" --jq "$filter" 2>/dev/null || true)"
  if [[ -n "$out" ]]; then echo "$out"; else echo "    —"; fi
}

cmd_parent() {
  local child="${1:?child issue number}" parent="${2:?parent issue number}" flag="${3:-}"
  need_int "$child"
  need_int "$parent"
  [[ "$child" != "$parent" ]] || die "an issue cannot be its own parent"
  local cid body
  cid="$(dbid "$child")"
  if [[ "$flag" = "--replace" ]]; then
    body="$(printf '{"sub_issue_id":%d,"replace_parent":true}' "$cid")"
  elif [[ -n "$flag" ]]; then
    die "unknown flag '$flag' (only --replace is supported)"
  else
    body="$(printf '{"sub_issue_id":%d}' "$cid")"
  fi
  printf '%s' "$body" | gh api --method POST "repos/$REPO/issues/$parent/sub_issues" --input - >/dev/null
  echo "ok: #$child is now a sub-issue of #$parent"
}

cmd_unparent() {
  local child="${1:?child issue number}"
  need_int "$child"
  local parent cid
  parent="$(gh api "repos/$REPO/issues/$child/parent" --jq '.number' 2>/dev/null || true)"
  [[ -n "$parent" ]] || die "#$child has no parent"
  cid="$(dbid "$child")"
  printf '{"sub_issue_id":%d}' "$cid" | gh api --method DELETE "repos/$REPO/issues/$parent/sub_issue" --input - >/dev/null
  echo "ok: detached #$child from parent #$parent"
}

cmd_blocked_by() {
  local n="${1:?issue number}" blocker="${2:?blocker issue number}"
  need_int "$n"
  need_int "$blocker"
  [[ "$n" != "$blocker" ]] || die "an issue cannot block itself"
  local bid
  bid="$(dbid "$blocker")"
  printf '{"issue_id":%d}' "$bid" | gh api --method POST "repos/$REPO/issues/$n/dependencies/blocked_by" --input - >/dev/null
  echo "ok: #$n is now blocked by #$blocker"
}

cmd_unblock() {
  local n="${1:?issue number}" blocker="${2:?blocker issue number}"
  need_int "$n"
  need_int "$blocker"
  local bid
  bid="$(dbid "$blocker")"
  gh api --method DELETE "repos/$REPO/issues/$n/dependencies/blocked_by/$bid" >/dev/null
  echo "ok: #$n is no longer blocked by #$blocker"
}

# "n blocks blocked" is stored as "blocked is blocked-by n" (the only writable
# direction the API exposes), so we POST to the OTHER issue's blocked_by list.
cmd_blocking() {
  local n="${1:?issue number}" blocked="${2:?blocked issue number}"
  need_int "$n"
  need_int "$blocked"
  [[ "$n" != "$blocked" ]] || die "an issue cannot block itself"
  local nid
  nid="$(dbid "$n")"
  printf '{"issue_id":%d}' "$nid" | gh api --method POST "repos/$REPO/issues/$blocked/dependencies/blocked_by" --input - >/dev/null
  echo "ok: #$n now blocks #$blocked"
}

cmd_unblocking() {
  local n="${1:?issue number}" blocked="${2:?blocked issue number}"
  need_int "$n"
  need_int "$blocked"
  local nid
  nid="$(dbid "$n")"
  gh api --method DELETE "repos/$REPO/issues/$blocked/dependencies/blocked_by/$nid" >/dev/null
  echo "ok: #$n no longer blocks #$blocked"
}

cmd_tree() {
  local n="${1:?issue number}"
  need_int "$n"
  local title parent
  title="$(gh api "repos/$REPO/issues/$n" --jq '"[\(.state)] \(.title)"' 2>/dev/null || true)"
  echo "#$n ${title:-} — $REPO"
  # The parent endpoint 404s when there is no parent, and gh emits the error
  # body on stdout; capture only on a clean (2xx) call so it does not leak.
  parent="$(gh api "repos/$REPO/issues/$n/parent" --jq '"#\(.number) [\(.state)] \(.title)"' 2>/dev/null)" || parent=""
  echo "  parent:"
  echo "    ${parent:-—}"
  echo "  sub-issues:"
  list_or_dash "repos/$REPO/issues/$n/sub_issues" '.[] | "    #\(.number) [\(.state)] \(.title)"'
  echo "  blocked by:"
  list_or_dash "repos/$REPO/issues/$n/dependencies/blocked_by" '.[] | "    #\(.number) [\(.state)] \(.title)"'
  echo "  blocking:"
  list_or_dash "repos/$REPO/issues/$n/dependencies/blocking" '.[] | "    #\(.number) [\(.state)] \(.title)"'
}

cmd_id() {
  dbid "${1:?issue number}"
  echo
}

# The self-test stands before the preflight, because it answers every gh call
# from a stub on PATH and the real `gh repo view` refuses a runner that holds
# no token for this repository.
self_test() {
  local work stub calls
  work="$(mktemp -d)"
  stub="$work/bin"
  calls="$work/calls"
  mkdir -p "$stub"
  cat > "$stub/gh" <<'STUB'
#!/usr/bin/env bash
# The gh a self-test run of scripts/gh/rel.sh speaks to. It writes every call,
# and the body a write sends on stdin, where the test reads them, and answers
# an issue read with a database id of 1000 plus the issue number.
set -euo pipefail
printf '%s\n' "$*" >> "$GH_STUB_CALLS"
case "${1:-} ${2:-}" in
  "repo view")
    printf '%s\n' "Example-Org/Example"
    ;;
  "api repos/Example-Org/Example/issues/"*)
    printf '%s\n' "$((1000 + ${2##*/}))"
    ;;
  "api --method")
    if [[ "${4:-}" = "--input" ]] || [[ "${5:-}" = "--input" ]]; then
      printf 'body: %s\n' "$(cat)" >> "$GH_STUB_CALLS"
    fi
    ;;
  *)
    echo "stub: an unexpected call: $*" >&2
    exit 90
    ;;
esac
STUB
  chmod 0755 "$stub/gh"

  local rc out err
  # run NAME CODE [ARG...]: this program through the stub with ARGs, landing
  # on CODE.
  run() {
    local name=$1 code=$2
    shift 2
    rc=0
    : > "$calls"
    PATH="$stub:$PATH" GH_STUB_CALLS="$calls" \
      bash "$0" "$@" > "$work/out" 2> "$work/err" || rc=$?
    out="$work/out"
    err="$work/err"
    if [[ "$rc" -ne "$code" ]]; then
      echo "gh-rel: self-test failed: $name exited $rc, wanted $code." >&2
      cat "$work/out" "$work/err" "$calls" >&2
      exit 1
    fi
  }
  # said NAME FILE NEEDLE: the case left that sentence where it belongs.
  said() {
    local name=$1 file=$2 needle=$3
    if ! grep -qF -- "$needle" "$file"; then
      echo "gh-rel: self-test failed: $name did not say '$needle'." >&2
      cat "$work/out" "$work/err" "$calls" >&2
      exit 1
    fi
  }
  # untouched NAME: the case made no gh call at all.
  untouched() {
    if [[ -s "$calls" ]]; then
      echo "gh-rel: self-test failed: $1 called gh." >&2
      cat "$work/out" "$work/err" "$calls" >&2
      exit 1
    fi
  }

  # A sub-issue write sends the child's database id to the parent's list.
  local sub_issue_case="parent 3 7"
  run "$sub_issue_case" 0 parent 3 7
  said "$sub_issue_case" "$out" "ok: #3 is now a sub-issue of #7"
  said "$sub_issue_case" "$calls" "api --method POST repos/Example-Org/Example/issues/7/sub_issues --input -"
  said "$sub_issue_case" "$calls" 'body: {"sub_issue_id":1003}'
  run "parent 3 7 --replace" 0 parent 3 7 --replace
  said "parent 3 7 --replace" "$calls" 'body: {"sub_issue_id":1003,"replace_parent":true}'

  # "3 blocks 7" is written as "7 is blocked by 3", the one writable direction.
  local blocking_case="blocking 3 7"
  run "$blocking_case" 0 blocking 3 7
  said "$blocking_case" "$out" "ok: #3 now blocks #7"
  said "$blocking_case" "$calls" "api --method POST repos/Example-Org/Example/issues/7/dependencies/blocked_by --input -"
  said "$blocking_case" "$calls" 'body: {"issue_id":1003}'

  run "id 5" 0 id 5
  said "id 5" "$out" "1005"

  # A usage error prints the usage and exits 2 before a single gh call.
  local argument
  for argument in --help -h help --bogus bogus; do
    run "the argument $argument" 2 "$argument"
    said "the argument $argument" "$err" "Usage:"
    untouched "the argument $argument"
  done
  run "no argument" 2
  said "no argument" "$err" "Usage:"
  untouched "no argument"
  run "parent with one operand" 2 parent 3
  untouched "parent with one operand"
  run "parent with an unknown flag" 2 parent 3 7 --force
  untouched "parent with an unknown flag"
  run "tree with no operand" 2 tree
  untouched "tree with no operand"
  run "id with two operands" 2 id 5 6
  untouched "id with two operands"
  run "a second argument after --self-test" 2 --self-test --bogus
  untouched "a second argument after --self-test"

  # An operand that is not an issue number fails before gh too, with exit 1.
  run "id abc" 1 id abc
  said "id abc" "$err" "expected an issue number, got 'abc'"
  untouched "id abc"

  rm -r "$work"
  echo "gh-rel: self-test OK."
}

main() {
  local sub="${1:-}" want operand
  case "$#:$sub" in
    1:--self-test)
      self_test
      exit 0
      ;;
    *) ;;
  esac
  case "$sub" in
    parent) [[ $# -eq 3 || ( $# -eq 4 && "${4:-}" == "--replace" ) ]] || usage ;;
    blocked-by | unblock | blocking | unblocking) [[ $# -eq 3 ]] || usage ;;
    unparent | tree | id) [[ $# -eq 2 ]] || usage ;;
    *) usage ;;
  esac
  shift
  if [[ "$sub" == parent ]]; then want=2; else want=$#; fi
  for operand in "${@:1:$want}"; do need_int "$operand"; done
  preflight
  case "$sub" in
    parent) cmd_parent "$@" ;;
    unparent) cmd_unparent "$@" ;;
    blocked-by) cmd_blocked_by "$@" ;;
    unblock) cmd_unblock "$@" ;;
    blocking) cmd_blocking "$@" ;;
    unblocking) cmd_unblocking "$@" ;;
    tree) cmd_tree "$@" ;;
    id) cmd_id "$@" ;;
    *) usage ;;
  esac
}

main "$@"
