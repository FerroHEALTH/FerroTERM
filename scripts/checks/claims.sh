#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The evidence in docs/claims.md stays verifiable: a citation names a file and
# a symbol, test, job, step, key or function, never a line number, because
# line numbers rot silently the first time the text above them moves. This
# check fails on a `<file>.<ext>:<line>` citation of any file (a `.rs`, a
# script, a workflow, a manifest), on a cited `path.rs::name` whose file does
# not exist or no longer contains the name, and on a cited script under
# scripts/ that does not exist.
#
# Usage:
#   scripts/checks/claims.sh               # docs/claims.md
#   scripts/checks/claims.sh --self-test   # each refusal on a synthetic page
#
# No specification governs this: our own design.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The line-number citations in $1: a path whose extension starts with a
# letter, a colon and a digit, so an address such as 127.0.0.1:8080 is no
# citation.
line_citations() {
  local doc="$1"
  grep -nE '[A-Za-z0-9_./-]+\.[A-Za-z][A-Za-z0-9]*:[0-9]+' "$doc" || true
}

# Every citation of $1 that does not resolve, one error line each.
unresolved() {
  local doc=$1 cite path name
  while IFS= read -r cite; do
    path="${cite%%::*}"
    name="${cite#*::}"
    if [[ ! -f "$path" ]]; then
      echo "error: $doc cites $path, which does not exist"
    elif ! grep -qF "$name" "$path"; then
      echo "error: $doc cites $cite, and '$name' does not occur in $path"
    fi
  done < <(grep -oE '`[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)+\.rs::[A-Za-z0-9_]+' "$doc" | tr -d '`' | sort -u)
  while IFS= read -r path; do
    [[ -f "$path" ]] || echo "error: $doc cites $path, which does not exist"
  done < <(grep -oE '`scripts/[A-Za-z0-9_./-]+\.(sh|jq)' "$doc" | tr -d '`' | sort -u)
}

# Runs the check over $1; 0 when every citation holds, 1 otherwise.
check() {
  local doc=$1 status=0 lines errors
  lines="$(line_citations "$doc")"
  if [[ -n "$lines" ]]; then
    printf '%s\n' "$lines"
    echo "error: $doc cites a line number; cite the file and a symbol, test, job, step, key or function instead" >&2
    status=1
  fi
  errors="$(unresolved "$doc")"
  if [[ -n "$errors" ]]; then
    printf '%s\n' "$errors" >&2
    status=1
  fi
  return "$status"
}

# Each refusal and each pass, judged on a synthetic page.
self_test() {
  local dir failed=0
  dir="$(mktemp -d)"
  # shellcheck disable=SC2064 # the directory is fixed when the trap is set
  trap "rm -rf '$dir'" EXIT
  local page=$dir/claims.md
  # shellcheck disable=SC2016 # the backticks are Markdown code spans
  local cases=(
    '0|a citation by name and an address|`scripts/checks/claims.sh`, its `line_citations` function, bound to 127.0.0.1:8080'
    '1|a .sh line citation|`scripts/checks/tx-ecosystem.sh:97`'
    '1|a workflow line range|`.github/workflows/ci.yml:12-20`'
    '1|a .rs line citation|`crates/fhir-terminology/src/lib.rs:3`'
    '1|a script that does not exist|`scripts/checks/no-such-guard.sh`'
    '1|a .rs symbol that does not occur|`crates/fhir-terminology/src/lib.rs::no_such_symbol_here`'
  )
  local entry want what text got
  for entry in "${cases[@]}"; do
    want="${entry%%|*}"
    what="${entry#*|}"
    text="${what#*|}"
    what="${what%%|*}"
    printf '| a | %s |\n' "$text" >"$page"
    got=0
    check "$page" >/dev/null 2>&1 || got=$?
    if [[ "$got" -ne "$want" ]]; then
      echo "self-test: $what: exit $got, expected $want" >&2
      failed=1
    fi
  done
  [[ "$failed" -eq 0 ]] && echo "ok: claims self-test (${#cases[@]} cases)"
  return "$failed"
}

case "${1-}" in
  "")
    doc=docs/claims.md
    check "$doc"
    echo "ok: claims ($doc cites no line number, and every cited symbol, test and script resolves)"
    ;;
  --self-test)
    self_test
    ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac
