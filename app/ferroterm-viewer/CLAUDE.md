<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md (`app/ferroterm-viewer`)

The FerroTERM Console: a Leptos app rendered on the server and hydrated in
the browser, with a backend-for-frontend in front of the terminology server,
drawn with the shared FerroHEALTH components of `ferrohealth-ui`. Its own
binary and image (`ferroterm-viewer`, `docker/viewer/Dockerfile`), built by
cargo-leptos. The component rules are FerroUI's `.claude/rules/leptos-ui.md`;
this console's own are **`.claude/rules/leptos-ui.md`** here, which loads
whenever a file in this directory is read. Nothing below repeats either.

Three properties decide most questions in this crate:

- **The crate depends on no workspace crate.** Every request goes through
  `crate::fhir` as one `fhir::exchange::Exchange`, sent server to server by
  `crate::upstream`; the browser half reaches it through the `forward`
  server function. `scripts/checks/viewer-boundary.sh` walks the resolved
  closure and fails on an edge into the engine.
- **No code system is a special case.** Every screen renders from
  `TerminologyCapabilities` and the operations. The test
  `pages::tests::no_screen_names_a_code_system` refuses a canonical written
  into a screen.
- **No token in the browser, no browser storage.** The reader's sign-in is
  the console server's (`session`, `oidc`); preferences are its cookies
  (`settings`).

## Layout

| Path | What it is |
|---|---|
| `src/lib.rs`, `src/main.rs` | the two halves, chosen by target; `hydrate()` is the browser's entry |
| `src/server.rs`, `src/upstream.rs` | the HTTP service and its security headers; the client of the FHIR server |
| `src/config/`, `src/session/`, `src/oidc/`, `src/cli.rs`, `src/command.rs`, `src/healthcheck.rs` | the configuration, the sign-in and the binary |
| `src/app.rs`, `src/components/`, `src/pages/` | the page tree, the shell and the screens |
| `src/fhir/` | the requests and the minimum the screens read of each answer |
| `src/authoring/` | the editors' models, plain values with unit tests |

The models live under `src/authoring/` rather than at the crate root because
`pages::tests::no_screen_names_a_code_system` walks directories: a screen's
logic added there is inside the neutrality guard the day it lands.

## Gates

`/ui-gates` runs the battery: `cargo fmt` and `leptosfmt`, `cargo clippy` on
the host and on `wasm32-unknown-unknown`, `cargo nextest run -p
ferroterm-viewer`, the release site bundle (`scripts/release/viewer-site.sh
--release`) and its budget (`scripts/checks/viewer-bundle.sh`). The browser
journeys are `scripts/ui-e2e.sh`, and CI's `ui-e2e` job gates the merge
whatever ran locally.
