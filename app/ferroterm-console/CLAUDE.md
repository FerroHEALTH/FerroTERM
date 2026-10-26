<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md (`app/ferroterm-console`)

The FerroTERM Console: a Leptos app drawn on the server in islands mode, with
a backend-for-frontend in front of the terminology server, drawn with the
shared FerroHEALTH components of `ferrohealth-ui`. Its own binary and image
(`ferroterm-console`, `docker/console/Dockerfile`), built by cargo-leptos.
The component rules are FerroUI's `.claude/rules/leptos-ui.md`; this
console's own are **`.claude/rules/leptos-ui.md`** here, which loads
whenever a file in this directory is read. Nothing below repeats either.

Three properties decide most questions in this crate:

- **The crate depends on no workspace crate.** Every request goes through
  `crate::fhir` as one `fhir::exchange::Exchange`, sent server to server by
  `crate::upstream`. `scripts/checks/console-boundary.sh` walks the resolved
  closure and fails on an edge into the engine.
- **No code system is a special case.** Every screen renders from
  `TerminologyCapabilities` and the operations. The test
  `pages::tests::no_screen_names_a_code_system` refuses a canonical written
  into a screen.
- **Every page works with no WebAssembly, no token reaches the browser, and
  nothing is stored in it.** Reads are links and `GET` forms, writes plain
  `POST` forms to the page's own address (`posted`, `pages::editor`); the
  sign-in is the console server's (`session`, `oidc`); preferences are its
  cookies (`prefs`, `post`).

## Layout

| Path | What it is |
|---|---|
| `src/lib.rs`, `src/main.rs` | the two halves, chosen by target; `hydrate()` hydrates the islands |
| `src/server.rs`, `src/upstream.rs` | the HTTP service and its security headers; the client of the FHIR server |
| `src/config/`, `src/session/`, `src/oidc/`, `src/cli.rs`, `src/command.rs` | the configuration, the sign-in and the binary |
| `src/app.rs`, `src/shell.rs`, `src/pages/` | the page tree, the chrome and the screens |
| `src/request.rs`, `src/prefs.rs`, `src/post.rs`, `src/posted.rs` | what a page reads of its request, the preferences, the console's form routes, a posted form |
| `src/islands.rs` | the console's one island: its keyboard shortcuts |
| `src/fhir/` | the requests and the minimum the screens read of each answer |
| `src/authoring/` | the editors' models, plain values with unit tests |
| `locales/` | the catalog of the console's words |

The models live under `src/authoring/` rather than at the crate root because
`pages::tests::no_screen_names_a_code_system` walks directories: a screen's
logic added there is inside the neutrality guard the day it lands.

## Gates

`/ui-gates` runs the battery: `cargo fmt` and `leptosfmt`, `cargo clippy` on
the host and on `wasm32-unknown-unknown`, `cargo nextest run -p
ferroterm-console`, the release site bundle (`scripts/release/console-site.sh
--release`) and its budget (`scripts/checks/console-bundle.sh`). The browser
journeys are `scripts/ui-e2e.sh`, and CI's `ui-e2e` job gates the merge
whatever ran locally.
