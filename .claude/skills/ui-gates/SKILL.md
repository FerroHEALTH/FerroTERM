---
name: ui-gates
description: >
  Runs the full console quality-gate battery for app/ferroterm-console:
  formatting, clippy, nextest, the console's guards and the browser
  journeys with FerroUI's modules loaded and refused. Use before committing
  any console change, when the user asks to "check the UI", or as the
  done-gate a ui-implementer task must pass.
allowed-tools: Bash, Read, Grep, Glob
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /ui-gates

Run every gate the console must pass (`.claude/rules/console-ui.md` §6).
Stop and report on the first hard failure; run the cheap gates first.

## What FerroUI already checks (inherited, not re-run here)

FerroUI leads the console's UI, and its gates hold every component the
console draws before a release reaches the pin (`ferrohealth-ui` in the
root `Cargo.toml`, `=0.6.2` today): the library gates of FerroUI's
`.claude/rules/console-ui.md` (fmt, clippy, nextest, the markup rules, the
script budget, the gallery's journeys with the modules refused and loaded,
and its screenshots), and the design checks of `design/COMPONENTS.md`
§ Checks every console runs over the console's mockup in
`design/consoles/ferroterm/`.

A gate failure that comes from a FerroUI part is reported as a FerroUI
issue (FerroHEALTH/FerroUI), never fixed by a local override.

## Preconditions

- `cargo-nextest`, `jq` and `unzip` on PATH, reported rather than silently
  installed. Ask before installing anything else.
- Shared `./target`, no ad-hoc `RUSTFLAGS`, no flag variation between runs.

## The battery, in order

```bash
# 1. Formatting.
cargo fmt -p ferroterm-console --check

# 2. Clippy.
cargo clippy -p ferroterm-console --all-targets -- -D warnings

# 3. Tests: the screens' plain logic, the markup of their parts, the
#    service in-process.
cargo nextest run -p ferroterm-console

# 4. The console's guards: the FHIR-client boundary, the tokens, the catalog.
bash scripts/checks/console-boundary.sh
bash scripts/checks/console-tokens.sh
bash scripts/checks/stray-literals.sh app/ferroterm-console/src

# 5. The journeys (merge-gating in CI): server, console and stub issuer on
#    loopback, Chrome for Testing over chromedriver, the modules loaded and
#    refused, every screen at 320 px.
bash scripts/ui-e2e.sh
```

Stage 5 drives Chrome for Testing and its chromedriver at the release
`docs/VERSIONS.md` pins. `FERROTERM_UI_E2E_CHROME` and
`FERROTERM_UI_E2E_CHROMEDRIVER` name them; without either, and with no
chromedriver on PATH, the script fetches that release into
`target/journey-tools` (`scripts/release/journey-tools.sh`). If it still
cannot run, report `SKIPPED(reason)` and say that CI's `ui-e2e` job runs it
and gates the merge; a skip is not a pass.

## Report

One line per gate: PASS, FAIL, or SKIPPED(reason), with the failing output
excerpted verbatim on a failure. Never mark a gate green you did not run. A
FAIL is never fixed by weakening the gate (removing a lint, deleting a test,
skipping a screen): fix the code, or file the FerroUI issue the failure
belongs to.
