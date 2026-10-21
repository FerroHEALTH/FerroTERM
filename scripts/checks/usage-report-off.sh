#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Test launchers switch the usage report off.
#
# The server sends a usage report by default, so every instance a test lane
# starts would report from a throwaway runner and count as an installation.
# A launcher is a tracked shell script or workflow that starts the server: the
# binary itself (`target/release/ferroterm`, `"$bin/ferroterm"`,
# `/usr/local/bin/ferroterm`, `cargo run -p ferroterm-server`), `docker
# compose … up`, `helm install` / `helm upgrade`, or `docker run` of the
# server image. Each one carries the switch (`FERROTERM_USAGE_REPORT=off`, the
# same key in a workflow `env:` map, or `usageReport.enabled=false` for a
# chart), or is only ever sourced by scripts that carry it. A `docker run` of
# the server image carries it on its own command line (`--env
# FERROTERM_USAGE_REPORT=off`): a container inherits nothing from the shell.
# A Rust source that starts the server as a child process (the bench harness)
# sets `.env("FERROTERM_USAGE_REPORT", "off")` on that `Command`.
# The one-shot `ferroterm healthcheck` starts no server and is not a launch.
# No FHIR or SNOMED CT specification governs this: our own design.
#
# Exempt: this file, which reads the commands as data.
#
#   scripts/checks/usage-report-off.sh [--self-test]
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly SELF='scripts/checks/usage-report-off.sh'
readonly SWITCH='FERROTERM_USAGE_REPORT(=|:[[:space:]]*["'\'']?)["'\'']?off|usageReport\.enabled=false'
# shellcheck disable=SC2016 # a regular expression, never expanded
readonly BINARY='(target/(release|debug)/|/usr/local/bin/|\$bin/|\$\{bin\}/)ferroterm(["'\''[:space:]]|$)|cargo[[:space:]]+run[[:space:]].*(-p|--package)[[:space:]=]+ferroterm-server|cargo[[:space:]]+run[[:space:]].*--bin[[:space:]=]+ferroterm([[:space:]]|$)'
readonly COMPOSE_UP='docker[ -]compose([[:space:]].*)?[[:space:]]up([[:space:]]|$)'
readonly HELM_INSTALL='(^|[;&|(`[:space:]])helm[[:space:]]+(upgrade|install)[[:space:]]'
readonly DOCKER_RUN='docker[[:space:]]+run[[:space:]].*(ghcr\.io/ferrohealth/ferroterm[:@]|SERVER_IMAGE|FERROTERM_IMAGE)'
# The switch handed to a container on its own `docker run` command line.
readonly RUN_SWITCH='(-e|--env)[[:space:]=]+["'\'']?FERROTERM_USAGE_REPORT=off'
# A Rust `Command` that starts the server, and the switch it must carry.
readonly RUST_LAUNCH='\.env\( ?"FERROTERM_LISTEN",'
readonly RUST_SWITCH='\.env\( ?"FERROTERM_USAGE_REPORT", ?"off",? ?\)'
# A one-shot subcommand that starts no server.
readonly ONE_SHOT='[[:space:]"'\'',]healthcheck(["'\''[:space:]]|$)'

# Prints the file's content without comment lines, one logical line per
# backslash-continued command.
logical_lines() {
  awk '/^[[:space:]]*#/ { next }
       { l = $0; if (sub(/\\$/, "", l)) { buf = buf l " "; next } print buf l; buf = "" }
       END { if (buf != "") print buf }' "$1"
}

# The predicates grep a captured copy, never a pipe: `grep -q` exits at the
# first match, and under pipefail the writer's SIGPIPE would read as no match.
is_launcher() {
  local lines starts runs
  lines="$(logical_lines "$1")"
  grep -qE "$COMPOSE_UP|$HELM_INSTALL" <<<"$lines" && return 0
  starts="$(grep -E "$BINARY|$DOCKER_RUN" <<<"$lines" || true)"
  [[ -n "$starts" ]] && grep -qvE "$ONE_SHOT" <<<"$starts"
}

# Whether $1 starts the server with a `docker run` that does not hand the
# container the switch itself.
bare_docker_run() {
  local lines runs
  lines="$(logical_lines "$1")"
  runs="$(grep -E "$DOCKER_RUN" <<<"$lines" | grep -vE "$ONE_SHOT" || true)"
  [[ -n "$runs" ]] && grep -qvE "$RUN_SWITCH" <<<"$runs"
}

carries_switch() {
  local lines
  lines="$(logical_lines "$1")"
  grep -qE "$SWITCH" <<<"$lines"
}

# Lists the candidate files: shell scripts and workflow/action YAML.
candidates() {
  local f
  while IFS= read -r f; do
    case "$f" in
      "$SELF") continue ;;
      .github/*.yml | .github/*.yaml | *.sh) printf '%s\n' "$f" ;;
      scripts/* | docker/* | deploy/* | e2e/* | bench/*)
        head -n 1 "$f" 2>/dev/null | grep -qE '^#!.*(ba)?sh' && printf '%s\n' "$f"
        ;;
      *) ;;
    esac
  done < <("$@" | grep -E '^(scripts|docker|deploy|e2e|bench|\.github/(workflows|actions))/' | sort)
}

# Prints every candidate that sources `$1` (`. path` or `source path`).
sourcers() {
  local target="$1" list="$2" f
  while IFS= read -r f; do
    [[ "$f" = "$target" ]] && continue
    grep -qE "^[[:space:]]*(\.|source)[[:space:]]+[\"']?([^[:space:]]*/)?${target//./\\.}([\"'[:space:]]|$)" "$f" &&
      printf '%s\n' "$f"
  done <<<"$list"
}

# Prints one finding per launcher that neither carries the switch nor is
# covered by every script that sources it. $@ = the file-listing command.
findings() {
  local list f carriers c covered
  list="$(candidates "$@")"
  while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    is_launcher "$f" || continue
    if bare_docker_run "$f"; then
      printf '%s\n' "$f"
      continue
    fi
    carries_switch "$f" && continue
    carriers="$(sourcers "$f" "$list")"
    covered=0
    if [[ -n "$carriers" ]]; then
      covered=1
      while IFS= read -r c; do
        carries_switch "$c" || covered=0
      done <<<"$carriers"
    fi
    [[ "$covered" -eq 1 ]] || printf '%s\n' "$f"
  done <<<"$list"
  rust_findings "$@"
}

# Prints every tracked Rust source that starts the server as a child process
# without the switch. A Rust launcher names the listen address on its
# `Command` (`.env("FERROTERM_LISTEN", …)`), as the bench harness does, and
# must also set `.env("FERROTERM_USAGE_REPORT", "off")`. The source is read
# with its line breaks folded, so a call that rustfmt wraps still matches.
rust_findings() {
  local f text
  while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    grep -q 'FERROTERM_LISTEN' "$f" || continue
    text="$(tr '\n' ' ' <"$f" | tr -s '[:space:]' ' ')"
    grep -qE "$RUST_LAUNCH" <<<"$text" || continue
    grep -qE "$RUST_SWITCH" <<<"$text" || printf '%s\n' "$f"
  done < <("$@" | grep -E '^(app|tools|crates|addons)/.*\.rs$' | sort)
}

self_test() {
  local tmp found failures=0 name
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $tmp now, while it is still in scope
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "$tmp/scripts/fam" "$tmp/scripts/checks" "$tmp/docker" "$tmp/.github/workflows"
  printf '#!/usr/bin/env bash\nFERROTERM_USAGE_REPORT=off target/release/ferroterm &\n' >"$tmp/scripts/off.sh"
  printf '#!/usr/bin/env bash\nFERROTERM_LISTEN=127.0.0.1:1 target/release/ferroterm > log 2>&1 &\n' >"$tmp/scripts/on.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\nFERROTERM_INDEX=x \\\n  "$bin/ferroterm" >log &\n' >"$tmp/scripts/bin-on.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\nexport FERROTERM_USAGE_REPORT=off\n"$bin/ferroterm" &\n' >"$tmp/scripts/bin-off.sh"
  printf '#!/usr/bin/env bash\ncargo run --release -p ferroterm-server\n' >"$tmp/scripts/cargo-on.sh"
  printf '#!/usr/bin/env bash\ndocker compose -f compose.yaml up -d\n' >"$tmp/scripts/compose-on.sh"
  # A launch early in a long file: a predicate that piped into grep -q would miss it.
  { printf '#!/usr/bin/env bash\ndocker compose up -d\n'; seq 1 20000 | sed 's/^/echo /'; } >"$tmp/scripts/long-on.sh"
  printf '#!/usr/bin/env bash\nhelm upgrade --install r chart \\\n  --set image.tag=x\n' >"$tmp/scripts/helm-on.sh"
  printf '#!/usr/bin/env bash\nhelm upgrade --install r chart \\\n  --set usageReport.enabled=false\n' >"$tmp/scripts/helm-off.sh"
  printf '#!/usr/bin/env bash\n# target/release/ferroterm &\necho done\n' >"$tmp/scripts/commented.sh"
  printf '#!/usr/bin/env bash\ntarget/release/ferroterm-build --out x\ntarget/release/ferroterm-bench\n' >"$tmp/scripts/tools.sh"
  printf '#!/usr/bin/env bash\n/usr/local/bin/ferroterm healthcheck\n' >"$tmp/scripts/probe.sh"
  printf 'target/release/ferroterm &\n' >"$tmp/scripts/fam/covered.sh"
  printf '#!/usr/bin/env bash\nexport FERROTERM_USAGE_REPORT=off\n. scripts/fam/covered.sh\n' >"$tmp/scripts/carrier.sh"
  printf 'target/release/ferroterm &\n' >"$tmp/scripts/fam/orphan.sh"
  printf '#!/usr/bin/env bash\n. scripts/fam/orphan.sh\n' >"$tmp/scripts/bare-carrier.sh"
  printf 'jobs:\n  a:\n    steps:\n      - run: |\n          docker run -d ghcr.io/ferrohealth/ferroterm:main\n' >"$tmp/.github/workflows/run-on.yml"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf 'env:\n  FERROTERM_USAGE_REPORT: "off"\njobs:\n  a:\n    steps:\n      - run: docker run -d "$SERVER_IMAGE"\n' >"$tmp/.github/workflows/run-job-env.yml"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf 'jobs:\n  a:\n    steps:\n      - run: docker run -d --env FERROTERM_USAGE_REPORT=off "$SERVER_IMAGE"\n' >"$tmp/.github/workflows/run-off.yml"
  printf 'env:\n  FERROTERM_USAGE_REPORT: "off"\njobs:\n  a:\n    steps:\n      - run: target/release/ferroterm &\n' >"$tmp/.github/workflows/bin-off.yml"
  printf 'jobs:\n  a:\n    steps:\n      - run: target/release/ferroterm &\n' >"$tmp/.github/workflows/bin-on.yml"
  printf '#!/usr/bin/env bash\ngrep "target/release/ferroterm &" x\n' >"$tmp/$SELF"
  printf '#!/usr/bin/env bash\ndocker run --rm ghcr.io/ferrohealth/ferroterm:main healthcheck\n' >"$tmp/docker/one-shot.sh"
  mkdir -p "$tmp/tools/bench/src"
  printf 'Command::new(b)\n    .env("FERROTERM_LISTEN", a)\n    .spawn()\n' >"$tmp/tools/bench/src/on.rs"
  printf 'Command::new(b)\n    .env("FERROTERM_LISTEN", a)\n    .env(\n        "FERROTERM_USAGE_REPORT",\n        "off",\n    )\n' >"$tmp/tools/bench/src/off.rs"
  printf 'Command::new(b).env("FERROTERM_INDEX", a).spawn()\n' >"$tmp/tools/bench/src/build.rs"

  found="$(cd "$tmp" && findings find scripts docker .github tools -type f)"
  for name in scripts/on.sh scripts/bin-on.sh scripts/cargo-on.sh scripts/compose-on.sh scripts/long-on.sh \
    scripts/helm-on.sh scripts/fam/orphan.sh .github/workflows/run-on.yml .github/workflows/run-job-env.yml \
    .github/workflows/bin-on.yml tools/bench/src/on.rs; do
    grep -qxF "$name" <<<"$found" || {
      echo "self-test: $name was not caught" >&2
      failures=1
    }
  done
  for name in scripts/off.sh scripts/bin-off.sh scripts/helm-off.sh scripts/commented.sh scripts/tools.sh \
    scripts/probe.sh scripts/fam/covered.sh scripts/carrier.sh scripts/bare-carrier.sh .github/workflows/run-off.yml \
    .github/workflows/bin-off.yml "$SELF" docker/one-shot.sh tools/bench/src/off.rs tools/bench/src/build.rs; do
    if grep -qxF "$name" <<<"$found"; then
      echo "self-test: $name was wrongly caught" >&2
      failures=1
    fi
  done
  [[ "$failures" -eq 0 ]] || return 1
  echo "usage-report-off: self-test OK (eleven unswitched launchers caught, fifteen clean or exempt files allowed)."
}

report() {
  local found="$1"
  if [[ -z "$found" ]]; then
    echo "usage-report-off: OK (every launcher switches the usage report off)."
    return 0
  fi
  echo "error: a launcher starts a FerroTERM server with the usage report on" >&2
  local f
  while IFS= read -r f; do printf '  %s\n' "$f" >&2; done <<<"$found"
  echo "Set FERROTERM_USAGE_REPORT=off in the launcher (or in every script that sources it," >&2
  echo "or in the workflow env); a docker run of the server passes" >&2
  echo "--env FERROTERM_USAGE_REPORT=off on its own command line." >&2
  return 1
}

case "${1:-}" in
  --self-test) self_test ;;
  "") report "$(findings git ls-files)" ;;
  *)
    echo "usage: scripts/checks/usage-report-off.sh [--self-test]" >&2
    exit 2
    ;;
esac
