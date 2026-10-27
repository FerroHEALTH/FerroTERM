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
# server image. Each one carries the switch (`FERROTERM__USAGE_REPORT__ENABLED=false`, the
# same key in a workflow `env:` map, or `usageReport.enabled=false` for a
# chart), or is only ever sourced by scripts that carry it. A `docker run` of
# the server image carries it on its own command line (`--env
# FERROTERM__USAGE_REPORT__ENABLED=false`): a container inherits nothing from the shell.
# A Rust source that starts the server as a child process (the bench harness)
# sets `.env("FERROTERM__USAGE_REPORT__ENABLED", "false")` on that `Command`.
# The one-shot `ferroterm healthcheck` starts no server and is not a launch.
# No FHIR or SNOMED CT specification governs this: our own design.
#
# A launcher can also start a sibling family server (FerroEHR, FerroPIX,
# FerroTERM, FerroFED, FerroBRIDGE), which reports under its own key,
# `<PRODUCT>__USAGE_REPORT__ENABLED`. A Compose service running a family image
# sets that key to false or to an interpolation of `FERROTERM__USAGE_REPORT__ENABLED`, a
# `docker run` of a sibling image hands the container its key, and a crate
# whose Rust source names a sibling image sets `"<PRODUCT>__USAGE_REPORT__ENABLED",
# "false"` on the container.
#
# Exempt: this file, which reads the commands as data.
#
#   scripts/checks/usage-report-off.sh [--self-test]
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly SELF='scripts/checks/usage-report-off.sh'
readonly OWN_KEY='FERROTERM__USAGE_REPORT__ENABLED'
readonly SWITCH='FERROTERM__USAGE_REPORT__ENABLED(=|:[[:space:]]*["'\'']?)["'\'']?false|usageReport\.enabled=false'
# shellcheck disable=SC2016 # a regular expression, never expanded
readonly BINARY='(target/(release|debug)/|/usr/local/bin/|\$bin/|\$\{bin\}/)ferroterm(["'\''[:space:]]|$)|cargo[[:space:]]+run[[:space:]].*(-p|--package)[[:space:]=]+ferroterm-server|cargo[[:space:]]+run[[:space:]].*--bin[[:space:]=]+ferroterm([[:space:]]|$)'
readonly COMPOSE_UP='docker[ -]compose([[:space:]].*)?[[:space:]]up([[:space:]]|$)'
readonly HELM_INSTALL='(^|[;&|(`[:space:]])helm[[:space:]]+(upgrade|install)[[:space:]]'
# `docker run` in command position: at the start of a line, after a shell
# operator, after a workflow `run:` key, or after a word that runs the next
# one. The same words inside a string (an `echo` writing a fixture) start
# nothing.
readonly DOCKER_RUN='(^|[;&|(`]|(^|[[:space:]])(-[[:space:]]+)?run:|(^|[[:space:]])(then|do|exec|sudo|time|nohup|!))[[:space:]]*docker[[:space:]]+run[[:space:]].*(ghcr\.io/ferrohealth/ferroterm[:@]|SERVER_IMAGE|FERROTERM_IMAGE)'
# The switch handed to a container on its own `docker run` command line.
readonly RUN_SWITCH='(-e|--env)[[:space:]=]+["'\'']?FERROTERM__USAGE_REPORT__ENABLED=false'
# A Rust `Command` that starts the server, and the switch it must carry.
readonly RUST_LAUNCH='\.env\( ?"FERROTERM__SERVER__LISTEN",'
readonly RUST_SWITCH='\.env\( ?"FERROTERM__USAGE_REPORT__ENABLED", ?"false",? ?\)'
# A one-shot subcommand that starts no server.
readonly ONE_SHOT='[[:space:]"'\'',]healthcheck(["'\''[:space:]]|$)'

# Prints the file's content without comment lines, one logical line per
# backslash-continued command.
logical_lines() {
  local file="$1"
  awk '/^[[:space:]]*#/ { next }
       { l = $0; if (sub(/\\$/, "", l)) { buf = buf l " "; next } print buf l; buf = "" }
       END { if (buf != "") print buf }' "$file"
}

# The predicates grep a captured copy, never a pipe: `grep -q` exits at the
# first match, and under pipefail the writer's SIGPIPE would read as no match.
is_launcher() {
  local lines starts
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
  sibling_findings "$@"
}

# Prints every tracked Rust source that starts the server as a child process
# without the switch. A Rust launcher names the listen address on its
# `Command` (`.env("FERROTERM__SERVER__LISTEN", …)`), as the bench harness does, and
# must also set `.env("FERROTERM__USAGE_REPORT__ENABLED", "false")`. The source is read
# with its line breaks folded, so a call that rustfmt wraps still matches.
rust_findings() {
  local f text
  while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    grep -q 'FERROTERM__SERVER__LISTEN' "$f" || continue
    text="$(tr '\n' ' ' <"$f" | tr -s '[:space:]' ' ')"
    grep -qE "$RUST_LAUNCH" <<<"$text" || continue
    grep -qE "$RUST_SWITCH" <<<"$text" || printf '%s\n' "$f"
  done < <("$@" | grep -E '^(app|tools|crates|addons)/.*\.rs$' | sort)
}

# --- Sibling servers: the family products a launcher starts besides this one.
#
# A Compose file, a `docker run` or a test harness can start another family
# server (FerroEHR, FerroPIX, FerroTERM, FerroFED, FerroBRIDGE), and each one
# reports under its own key, `<PRODUCT>__USAGE_REPORT__ENABLED`. This repo's
# switch stops none of them on its own.
readonly FAMILY='ferroehr|ferropix|ferroterm|ferrofed|ferrobridge'
readonly FAMILY_IMAGE="ghcr\\.io/ferrohealth/($FAMILY)[:@]"

# Prints `file: service (KEY)` for each service of a Compose file that runs a
# family server image without that server's switch in its own `environment`:
# KEY set to false, or to an interpolation of OWN_KEY, which every Compose
# launcher exports as false. A service that merges an `x-` anchor
# (`<<: *name`) runs the anchor's image; one whose `entrypoint` names another
# binary of the image starts no server.
compose_findings() {
  awk -v own="$OWN_KEY" -v img="$FAMILY_IMAGE" -v file="$1" -v sq="'" '
    function close_block() {
      if (kind == "anchor" && product != "") anchor_image[name] = product
      if (kind == "service") { svc[++n] = name; svc_image[name] = product; svc_merge[name] = merge; svc_entry[name] = entry }
      kind = ""; name = ""; product = ""; merge = ""; entry = ""
    }
    /^[[:space:]]*#/ { next }
    /^[^[:space:]]/ {
      close_block(); in_services = ($0 ~ /^services:/)
      if (match($0, /&[A-Za-z0-9_-]+/)) { kind = "anchor"; name = substr($0, RSTART + 1, RLENGTH - 1) }
      next
    }
    in_services && /^  [A-Za-z0-9_.-]+:/ {
      close_block(); kind = "service"; name = $0; sub(/^  /, "", name); sub(/:.*/, "", name); next
    }
    kind == "" { next }
    /^[[:space:]]+image:/ && match($0, img) { product = toupper(substr($0, RSTART + 20, RLENGTH - 21)) }
    /^[[:space:]]+entrypoint:/ { entry = $0 }
    match($0, /<<:[[:space:]]*\*[A-Za-z0-9_-]+/) { merge = substr($0, RSTART, RLENGTH); sub(/.*\*/, "", merge) }
    match($0, /[A-Z]+__USAGE_REPORT__ENABLED[[:space:]]*[:=]/) {
      key = substr($0, RSTART, RLENGTH); sub(/[[:space:]]*[:=]$/, "", key)
      value = substr($0, RSTART + RLENGTH); gsub(sq, "", value); gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", value)
      if (kind == "service" && (value == "false" || index(value, "${" own) > 0)) ok[name, key] = 1
    }
    END {
      close_block()
      for (i = 1; i <= n; i++) {
        s = svc[i]; p = svc_image[s]
        if (p == "" && svc_merge[s] != "") p = anchor_image[svc_merge[s]]
        # An entrypoint naming another binary of the image (an index build) serves nothing.
        if (p != "" && svc_entry[s] != "" && svc_entry[s] !~ ("/" tolower(p) "([^-a-z]|$)")) continue
        if (p != "" && !((s, p "__USAGE_REPORT__ENABLED") in ok))
          printf "%s: %s (%s__USAGE_REPORT__ENABLED)\n", file, s, p
      }
    }
  ' "$1"
}

# Prints `file (KEY)` for each `docker run` in $1 that starts a sibling family
# server without handing the container that server's switch on its own
# command line. A one-shot job starts no server.
sibling_run_findings() {
  local line product key
  while IFS= read -r line; do
    product="$(grep -oE "$FAMILY_IMAGE" <<<"$line" | sed -nE '1{s|.*/||; s|[:@]$||; p;}' | tr '[:lower:]' '[:upper:]')"
    key="${product}__USAGE_REPORT__ENABLED"
    [[ -n "$product" && "$key" != "$OWN_KEY" ]] || continue
    grep -qE '[[:space:]](healthcheck|--version|--help|config[[:space:]]+check)([[:space:]]|$)' <<<"$line" && continue
    grep -qE "(-e|--env)[[:space:]=]+[\"']?${key}=false" <<<"$line" || printf '%s (%s)\n' "$1" "$key"
  done < <(logical_lines "$1" | grep -E "docker[[:space:]]+run[[:space:]].*$FAMILY_IMAGE" || true)
}

# Prints `crate (KEY)` for each crate whose Rust source names a sibling
# family server image (`"ghcr.io/ferrohealth/<product>"`) while no Rust
# source of that crate sets `"<PRODUCT>__USAGE_REPORT__ENABLED", "false"` on
# the container. $@ = the file-listing command.
rust_sibling_findings() {
  local files f crate product key
  files="$("$@" | grep -E '\.rs$' || true)"
  [[ -n "$files" ]] || return 0
  # shellcheck disable=SC2086 # one word per tracked path, none with spaces
  while IFS= read -r f; do
    crate="$(cut -d/ -f1-2 <<<"$f")"
    while IFS= read -r product; do
      key="$(tr '[:lower:]' '[:upper:]' <<<"$product")__USAGE_REPORT__ENABLED"
      [[ "$key" != "$OWN_KEY" ]] || continue
      # shellcheck disable=SC2046 # one word per tracked path, none with spaces
      grep -qE "\"${key}\",[[:space:]]*\"false\"" $(grep -E "^${crate}/" <<<"$files") ||
        printf '%s (%s)\n' "$crate" "$key"
    done < <(grep -oE "\"ghcr\\.io/ferrohealth/($FAMILY)\"" "$f" | sed -E 's|.*/||; s|"$||' | sort -u)
  done < <(grep -lE "\"ghcr\\.io/ferrohealth/($FAMILY)\"" $files 2>/dev/null || true)
}

# The Compose files among the listed ones: YAML with a top-level `services:`.
compose_files() {
  local f
  while IFS= read -r f; do
    case "$f" in
      .github/* | scripts/checks/*) continue ;;
      *.yml | *.yaml) grep -qE '^services:' "$f" 2>/dev/null && printf '%s\n' "$f" ;;
      *) ;;
    esac
  done < <("$@")
}

# Prints every sibling finding over the listed files.
sibling_findings() {
  local f
  while IFS= read -r f; do compose_findings "$f"; done < <(compose_files "$@")
  while IFS= read -r f; do [[ -n "$f" ]] && sibling_run_findings "$f"; done <<<"$(candidates "$@")"
  rust_sibling_findings "$@"
}


# Lists the self-test's fixture tree the way `git ls-files` lists the repo.
tracked_fixtures() { find . -type f | sed 's|^\./||'; }

self_test() {
  local tmp found failures=0 name
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $tmp now, while it is still in scope
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "$tmp/scripts/fam" "$tmp/scripts/checks" "$tmp/docker" "$tmp/.github/workflows"
  printf '#!/usr/bin/env bash\nFERROTERM__USAGE_REPORT__ENABLED=false target/release/ferroterm &\n' >"$tmp/scripts/off.sh"
  printf '#!/usr/bin/env bash\nFERROTERM__SERVER__LISTEN=127.0.0.1:1 target/release/ferroterm > log 2>&1 &\n' >"$tmp/scripts/on.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\nFERROTERM__TERMINOLOGY__INDEX=x \\\n  "$bin/ferroterm" >log &\n' >"$tmp/scripts/bin-on.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\nexport FERROTERM__USAGE_REPORT__ENABLED=false\n"$bin/ferroterm" &\n' >"$tmp/scripts/bin-off.sh"
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
  printf '#!/usr/bin/env bash\nexport FERROTERM__USAGE_REPORT__ENABLED=false\n. scripts/fam/covered.sh\n' >"$tmp/scripts/carrier.sh"
  printf 'target/release/ferroterm &\n' >"$tmp/scripts/fam/orphan.sh"
  printf '#!/usr/bin/env bash\n. scripts/fam/orphan.sh\n' >"$tmp/scripts/bare-carrier.sh"
  printf 'jobs:\n  a:\n    steps:\n      - run: |\n          docker run -d ghcr.io/ferrohealth/ferroterm:main\n' >"$tmp/.github/workflows/run-on.yml"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf 'env:\n  FERROTERM__USAGE_REPORT__ENABLED: "false"\njobs:\n  a:\n    steps:\n      - run: docker run -d "$SERVER_IMAGE"\n' >"$tmp/.github/workflows/run-job-env.yml"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf 'jobs:\n  a:\n    steps:\n      - run: docker run -d --env FERROTERM__USAGE_REPORT__ENABLED=false "$SERVER_IMAGE"\n' >"$tmp/.github/workflows/run-off.yml"
  printf 'env:\n  FERROTERM__USAGE_REPORT__ENABLED: "false"\njobs:\n  a:\n    steps:\n      - run: target/release/ferroterm &\n' >"$tmp/.github/workflows/bin-off.yml"
  printf 'jobs:\n  a:\n    steps:\n      - run: target/release/ferroterm &\n' >"$tmp/.github/workflows/bin-on.yml"
  printf '#!/usr/bin/env bash\ngrep "target/release/ferroterm &" x\n' >"$tmp/$SELF"
  printf '#!/usr/bin/env bash\ndocker run --rm ghcr.io/ferrohealth/ferroterm:main healthcheck\n' >"$tmp/docker/one-shot.sh"
  printf '#!/usr/bin/env bash\necho "Run docker run ghcr.io/ferrohealth/ferroterm:1.0.0." >fixture.md\n' >"$tmp/scripts/echo-data.sh"
  printf '#!/usr/bin/env bash\nset -e; docker run -d ghcr.io/ferrohealth/ferroterm:main\n' >"$tmp/scripts/run-after-operator.sh"
  mkdir -p "$tmp/tools/bench/src"
  printf 'Command::new(b)\n    .env("FERROTERM__SERVER__LISTEN", a)\n    .spawn()\n' >"$tmp/tools/bench/src/on.rs"
  printf 'Command::new(b)\n    .env("FERROTERM__SERVER__LISTEN", a)\n    .env(\n        "FERROTERM__USAGE_REPORT__ENABLED",\n        "false",\n    )\n' >"$tmp/tools/bench/src/off.rs"
  printf 'Command::new(b).env("FERROTERM__TERMINOLOGY__INDEX", a).spawn()\n' >"$tmp/tools/bench/src/build.rs"

  # Sibling servers: Compose services, a docker run and a Rust harness.
  mkdir -p "$tmp/scripts" "$tmp/tools/bare/src" "$tmp/tools/held/src"
  # shellcheck disable=SC2016 # the fixture is literal Compose text, never expanded here
  printf 'x-node: &node\n  image: ghcr.io/ferrohealth/ferroehr:1.0.0\nservices:\n  ferroterm:\n    image: ghcr.io/ferrohealth/ferroterm:1.0.0\n    environment:\n      FERROTERM__USAGE_REPORT__ENABLED: ${FERROTERM__USAGE_REPORT__ENABLED:-true}\n  ferroehr:\n    image: ghcr.io/ferrohealth/ferroehr:1.0.0\n    environment:\n      FERROEHR__USAGE_REPORT__ENABLED: ${FERROTERM__USAGE_REPORT__ENABLED:-true}\n  ferropix:\n    image: ghcr.io/ferrohealth/ferropix:1.0.0@sha256:0\n    environment:\n      - X=1\n  ferropix-off:\n    image: ghcr.io/ferrohealth/ferropix:1.0.0\n    environment:\n      FERROPIX__USAGE_REPORT__ENABLED: "false"\n  node-a:\n    <<: *node\n    environment:\n      X: "1"\n  ferroehr-postgres:\n    image: ghcr.io/ferrohealth/ferroehr-postgres:1.0.0\n  ferropix-build:\n    image: ghcr.io/ferrohealth/ferropix:1.0.0\n    entrypoint: ["/usr/local/bin/ferropix-build"]\n  ferroehr-serve:\n    image: ghcr.io/ferrohealth/ferroehr:1.0.0\n    entrypoint: ["/usr/local/bin/ferroehr", "serve"]\n' > "$tmp/compose.sib.yaml"
  printf '#!/usr/bin/env bash\nexport FERROTERM__USAGE_REPORT__ENABLED=false\ndocker run -d ghcr.io/ferrohealth/ferroehr:1.0.0\n' > "$tmp/scripts/sib-run-on.sh"
  printf '#!/usr/bin/env bash\ndocker run -d \\\n  -e FERROEHR__USAGE_REPORT__ENABLED=false ghcr.io/ferrohealth/ferroehr:1.0.0\n' > "$tmp/scripts/sib-run-off.sh"
  printf '#!/usr/bin/env bash\ndocker run --rm ghcr.io/ferrohealth/ferroehr:1.0.0 healthcheck\n' > "$tmp/scripts/sib-run-job.sh"
  printf 'const IMAGE: &str = "ghcr.io/ferrohealth/ferropix";\n' > "$tmp/tools/bare/src/images.rs"
  printf 'const IMAGE: &str = "ghcr.io/ferrohealth/ferropix";\n' > "$tmp/tools/held/src/images.rs"
  printf 'fn f(i: I) -> I {\n    i.with_env_var("FERROPIX__USAGE_REPORT__ENABLED", "false")\n}\n' > "$tmp/tools/held/src/start.rs"

  found="$(cd "$tmp" && findings tracked_fixtures)"
  for name in "compose.sib.yaml: ferropix (FERROPIX__USAGE_REPORT__ENABLED)" "compose.sib.yaml: ferroehr-serve (FERROEHR__USAGE_REPORT__ENABLED)" "compose.sib.yaml: node-a (FERROEHR__USAGE_REPORT__ENABLED)" \
    "scripts/sib-run-on.sh (FERROEHR__USAGE_REPORT__ENABLED)" "tools/bare (FERROPIX__USAGE_REPORT__ENABLED)"; do
    grep -qxF "$name" <<< "$found" || {
      echo "self-test: $name was not caught" >&2
      failures=1
    }
  done
  if grep -E 'compose\.sib\.yaml: (ferroterm|ferroehr|ferropix-off|ferroehr-postgres|ferropix-build) |sib-run-off|sib-run-job|tools/held' <<< "$found"; then
    echo "self-test: a switched sibling, a one-shot job or a non-server image was wrongly caught" >&2
    failures=1
  fi
  for name in scripts/on.sh scripts/bin-on.sh scripts/cargo-on.sh scripts/compose-on.sh scripts/long-on.sh \
    scripts/helm-on.sh scripts/fam/orphan.sh .github/workflows/run-on.yml .github/workflows/run-job-env.yml \
    .github/workflows/bin-on.yml tools/bench/src/on.rs scripts/run-after-operator.sh; do
    grep -qxF "$name" <<<"$found" || {
      echo "self-test: $name was not caught" >&2
      failures=1
    }
  done
  for name in scripts/off.sh scripts/bin-off.sh scripts/helm-off.sh scripts/commented.sh scripts/tools.sh \
    scripts/probe.sh scripts/fam/covered.sh scripts/carrier.sh scripts/bare-carrier.sh .github/workflows/run-off.yml \
    .github/workflows/bin-off.yml "$SELF" docker/one-shot.sh tools/bench/src/off.rs tools/bench/src/build.rs \
    scripts/echo-data.sh; do
    if grep -qxF "$name" <<<"$found"; then
      echo "self-test: $name was wrongly caught" >&2
      failures=1
    fi
  done
  [[ "$failures" -eq 0 ]] || return 1
  echo "usage-report-off: self-test OK (twelve unswitched launchers and five unswitched sibling servers caught, sixteen clean or exempt files and seven sibling cases allowed)."
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
  echo "Set FERROTERM__USAGE_REPORT__ENABLED=false in the launcher (or in every script that sources it," >&2
  echo "or in the workflow env); a docker run of the server passes" >&2
  echo "--env FERROTERM__USAGE_REPORT__ENABLED=false on its own command line." >&2
  echo "A sibling server named (KEY) takes its own key: false, or \${FERROTERM__USAGE_REPORT__ENABLED...}," >&2
  echo "in its Compose environment, --env KEY=false on a docker run, and" >&2
  echo ".with_env_var(\"KEY\", \"false\") in a Rust harness." >&2
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
