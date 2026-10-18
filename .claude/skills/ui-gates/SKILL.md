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

# /ui-gates

Run every gate the console must pass (`.claude/rules/leptos-ui.md` §6 and §7).
Stop and report on the first hard failure; run the cheap gates first.

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

Stage 5 locally needs Chrome for Testing and its chromedriver at the release
`docs/VERSIONS.md` pins, named by `FERROTERM_UI_E2E_CHROME` and
`FERROTERM_UI_E2E_CHROMEDRIVER`. Without them, report `SKIPPED(reason)` and
say that CI's `ui-e2e` job runs it and gates the merge; a skip is not a pass.

## Report

One line per gate: PASS, FAIL, or SKIPPED(reason), with the failing output
excerpted verbatim on a failure. Never mark a gate green you did not run. A
FAIL is never fixed by weakening the gate (removing a lint, deleting a test,
dropping the wasm pass, raising the budget): fix the code.
