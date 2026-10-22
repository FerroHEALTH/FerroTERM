---
name: ui-gates
description: >
  Runs the full console quality-gate battery for app/ferroterm-viewer:
  formatting, clippy on both halves, nextest, the release site bundle built by
  cargo-leptos, and its size against the budget. Use before committing any
  console change, when the user asks to "check the UI", or as the done-gate a
  ui-implementer task must pass.
allowed-tools: Bash, Read, Grep, Glob
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /ui-gates

Run every gate the console must pass (`.claude/rules/leptos-ui.md` §6 and §7).
Stop and report on the first hard failure; run the cheap gates first.

## What FerroUI already checks (inherited, not re-run here)

FerroUI leads the console's UI, and its gates hold every component the
console draws before a release reaches the pin (`ferrohealth-ui` in the
root `Cargo.toml`, `=0.2.0` today):

- the library gates of FerroUI's `.claude/rules/leptos-ui.md` §10 (fmt,
  clippy on the host and wasm32, nextest, the gallery journeys and
  screenshots) and its no-inline-style and no-WebAssembly tests
  (`crates/ferrohealth-ui/tests/it/stylesheet.rs` and, on `main`,
  `app/ferrohealth-ui-gallery/tests/it/no_wasm.rs`);
- the design checks of `design/COMPONENTS.md` § Checks every console runs
  (`render.py`, `overflow.py`, `hover.py`, `contrast.py`, `selects.py`,
  `popups.py`, `forms.py`, run by `design/tools/check.sh` over the console's
  mockup in `design/consoles/ferroterm/`).

A gate failure that comes from a FerroUI part is reported as a FerroUI
issue (FerroHEALTH/FerroUI), never fixed by a local override. The console's
own gates follow; the CI guards `viewer-boundary.sh`, `viewer-tokens.sh`
and `stray-literals.sh app/ferroterm-viewer/src` hold its boundary, its
tokens and its catalog beside them.

## Preconditions

- Tooling presence, reported rather than silently installed:
  `rustup target list --installed | grep wasm32`, `cargo leptos --version`
  (0.3.11), `wasm-bindgen --version` (the `=` pin of the root manifest),
  `leptosfmt --version`. `scripts/release/viewer-tools.sh` fetches the
  Tailwind CLI and `wasm-opt` by SHA-256. Ask before installing anything else.
- Shared `./target`, no ad-hoc `RUSTFLAGS`, no flag variation between runs.

## The battery, in order

```bash
# 1. Formatting.
cargo fmt -p ferroterm-viewer --check
leptosfmt --check app/ferroterm-viewer/src

# 2. Clippy on both halves: the host half (ssr) and the browser half
#    (hydrate), the gate that catches a dependency that cannot compile for
#    the browser.
cargo clippy -p ferroterm-viewer --all-targets -- -D warnings
cargo clippy -p ferroterm-viewer --lib --target wasm32-unknown-unknown -- -D warnings

# 3. Tests: the component-free logic.
cargo nextest run -p ferroterm-viewer

# 4. The release site bundle and its size. A budget never moves to match a
#    fatter build.
bash scripts/release/viewer-site.sh --release
bash scripts/checks/viewer-bundle.sh
bash scripts/checks/viewer-paths.sh

# 5. The journeys (merge-gating in CI): server, console and stub issuer on
#    loopback, Chrome for Testing over chromedriver.
bash scripts/ui-e2e.sh
```

Stage 5 drives Chrome for Testing and its chromedriver at the release
`docs/VERSIONS.md` pins. `FERROTERM_UI_E2E_CHROME` and
`FERROTERM_UI_E2E_CHROMEDRIVER` name them; without either, and with no
chromedriver on PATH, the script fetches that release into
`target/journey-tools` (`scripts/release/journey-tools.sh`) and the pinned
`tailwindcss` and `wasm-opt` into `target/viewer-tools`. It needs `jq`,
`unzip`, `cargo-nextest` and `cargo-leptos`. If it still cannot run, report
`SKIPPED(reason)` and say that CI's `ui-e2e` job runs it and gates the merge;
a skip is not a pass.

## Report

One line per gate: PASS, FAIL, or SKIPPED(reason), with the failing output
excerpted verbatim on a failure. Never mark a gate green you did not run. A
FAIL is never fixed by weakening the gate (removing a lint, deleting a test,
dropping the wasm pass, raising the budget): fix the code.
