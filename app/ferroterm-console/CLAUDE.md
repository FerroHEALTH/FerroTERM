<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md (`app/ferroterm-console`)

The FerroTERM Console: every page drawn on its own server with the shared
FerroHEALTH components of `ferrohealth-ui` 0.6, with a backend-for-frontend
in front of the terminology server. Its own binary and image
(`ferroterm-console`, `docker/console/Dockerfile`). The component rules are
FerroUI's `.claude/rules/console-ui.md`; this console's own are
**`.claude/rules/console-ui.md`** here, which loads whenever a file in this
directory is read. Nothing below repeats either.

Three properties decide most questions in this crate:

- **The crate depends on no workspace crate.** Every request goes through
  `crate::fhir` as one `fhir::exchange::Exchange`, sent server to server by
  `crate::upstream`. `scripts/checks/console-boundary.sh` walks the resolved
  closure and fails on an edge into the engine.
- **A screen is one async function from the request to its markup.**
  `screen::draw` runs it for the address's page, `shell::console_shell`
  frames it, and `app::answer` writes the document, or the redirect or
  status the screen said through `Request::answering`.
- **No code system is a special case.** Every screen renders from
  `TerminologyCapabilities` and the operations. The test
  `pages::tests::no_screen_names_a_code_system` refuses a canonical written
  into a screen.
- **Every page works with no script, no token reaches the browser, and
  nothing is stored in it.** Reads are links and `GET` forms, writes plain
  `POST` forms to the page's own address (`posted`, `pages::editor`); the
  sign-in is the console server's (`session`, `oidc`); preferences are its
  cookies (`prefs`, `post`), read once per request into the visit
  (`visit`). FerroUI's modules only improve a page that already works.

## Layout

| Path | What it is |
|---|---|
| `src/lib.rs`, `src/main.rs` | the library and the binary |
| `src/server.rs`, `src/upstream.rs` | the HTTP service and its security headers; the client of the FHIR server |
| `src/config/`, `src/session/`, `src/oidc/`, `src/cli.rs`, `src/command.rs` | the configuration, the sign-in and the binary |
| `src/app.rs`, `src/shell.rs`, `src/screen.rs`, `src/pages/` | the document and the answer, the chrome around a page, the routes and the page each draws, and the screens |
| `src/visit.rs`, `src/request.rs`, `src/prefs.rs`, `src/post.rs`, `src/posted.rs` | what the server read of the request, what a page reads of it, the preferences, the console's form routes, a posted form |
| `src/markup.rs` | the shorthands a screen writes its own escaped markup with |
| `src/fhir/` | the requests and the minimum the screens read of each answer |
| `src/authoring/` | the editors' models, plain values with unit tests |
| `locales/` | the catalog of the console's words |

The models live under `src/authoring/` rather than at the crate root because
`pages::tests::no_screen_names_a_code_system` walks directories: a screen's
logic added there is inside the neutrality guard the day it lands.

## Gates

`/ui-gates` runs the battery: `cargo fmt`, `cargo clippy`, `cargo nextest
run -p ferroterm-console` and the console's guards. The browser journeys are
`scripts/ui-e2e.sh`, with FerroUI's modules loaded and refused and every
screen at 320 px, and CI's `ui-e2e` job gates the merge whatever ran
locally.
