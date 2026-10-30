#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The console's coverage under the browser journeys, as an lcov report (no
# specification governs this: our own design).
#
#   scripts/ui-e2e-coverage.sh OUT.lcov
#
# The console's screens are proven by the journeys under e2e/, which drive the
# release console binary scripts/ui-e2e.sh builds. This runs that battery with
# the host build instrumented (rustc's `-C instrument-coverage`, the rustc
# book, "Instrumentation-based Code Coverage"), each process writing its raw
# profile into a scratch directory, then merges the profiles with the
# toolchain's llvm-profdata and exports what the console binary ran, as lcov,
# to OUT.lcov. The console draws every page on its server, so the binary is
# all the report measures. The console stops on the battery's SIGTERM and
# writes its profile as it exits.
#
# The battery's own failures fail this script, and the report is written only
# from a green run. Requires what scripts/ui-e2e.sh requires, and the
# toolchain's llvm-tools component (`llvm-profdata`, `llvm-cov`).
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out="${1:?usage: scripts/ui-e2e-coverage.sh OUT.lcov}"

host="$(rustc -vV | sed -n 's/^host: //p')"
tools="$(rustc --print sysroot)/lib/rustlib/$host/bin"
for tool in llvm-profdata llvm-cov; do
  [[ -x "$tools/$tool" ]] || {
    echo "ui-e2e-coverage: no $tool in $tools; add the llvm-tools component" >&2
    exit 1
  }
done

profiles="$(mktemp -d)"
trap 'rm -f -- "$profiles"/*.profraw "$profiles"/merged.profdata; rmdir -- "$profiles" 2> /dev/null || true' EXIT

# The host triple's own flags, so a build script stays uninstrumented (the
# Cargo reference, configuration, `target.<triple>.rustflags`).
flags_var="CARGO_TARGET_$(tr '[:lower:]-' '[:upper:]_' <<< "$host")_RUSTFLAGS"
export "$flags_var=-C instrument-coverage"
export LLVM_PROFILE_FILE="$profiles/%p-%m.profraw"

bash scripts/ui-e2e.sh

shopt -s nullglob
raw=("$profiles"/*.profraw)
[[ "${#raw[@]}" -gt 0 ]] || {
  echo "ui-e2e-coverage: the battery passed and no process wrote a profile" >&2
  exit 1
}
"$tools/llvm-profdata" merge -sparse "${raw[@]}" -o "$profiles/merged.profdata"
"$tools/llvm-cov" export -format=lcov \
  -instr-profile "$profiles/merged.profdata" \
  -ignore-filename-regex '/(\.cargo|rustc|lib/rustlib)/' \
  target/release/ferroterm-console > "$out"
grep -q '^SF:.*app/ferroterm-console/src/pages/' "$out" || {
  echo "ui-e2e-coverage: the report names no page of the console" >&2
  exit 1
}
echo "ui-e2e-coverage: wrote $out from ${#raw[@]} profiles"
