---
paths: ["app/ferroterm-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FerroTERM Console's own rules (`app/ferroterm-console`)

**FerroUI leads the console's UI.** FerroUI (`ferrohealth-ui` on crates.io,
repository FerroHEALTH/FerroUI) is the family's component library and the
one source of its console design and its Leptos rules. Read these first,
before any console change:

- the consumer guide, [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/main/docs/consoles.md):
  how a console takes the library, what it keeps local, how it moves to a
  new release;
- the component rules, [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.6/.claude/rules/leptos-ui.md):
  the mandates (§0: Rust only, no inline style, no server function in the
  library, no browser storage, no patient identifier in a URL), the
  four-layer interaction contract (§1), reactivity, views, forms, tables and
  paging, accessibility, tokens and themes, icons, the bundle, the catalogs,
  the book's rules and whole-page hydration (§15);
- the crate discipline, [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.6/crates/ferrohealth-ui/CLAUDE.md):
  the interaction contract and the list of ready components;
- the reviewed design, [`design/README.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.6/design/README.md),
  [`design/COMPONENTS.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.6/design/COMPONENTS.md)
  (the component IDs such as R9, F13 and M1 to M6, the family rules, the
  page templates) and the console's mockup under
  [`design/consoles/ferroterm/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.5.6/design/consoles/ferroterm).

**The pin decides the version you read.** The console pins `ferrohealth-ui
= "=0.5.6"` (root `Cargo.toml`), so every FerroUI document above is read at
the tag `v0.5.6`. FerroUI's `design/` is the only design source: a design
folder anywhere else is stale and never synced from.

**Nothing FerroUI has or should have is built here.** A missing component,
a wrong behaviour in one, a stylesheet override or a script the console
would need is filed on FerroHEALTH/FerroUI and linked from the console's
issue (§4). Never a local copy, a shadow component, a local stylesheet
override or a hand-written script.

This file keeps only what is particular to FerroTERM's console. The oracle
for a Leptos question FerroUI does not settle is the official Leptos book
(<https://github.com/leptos-rs/book>, Leptos 0.8) through `/leptos-lookup`.

## 1. What the console is

- **A backend-for-frontend binary whose page the browser hydrates whole.**
  `ferroterm-console` (`serve`, `config check`, `healthcheck`) draws the
  first page on the server; the browser hydrates it and draws every page
  after it, so no link or `GET` form reloads the document (FerroUI
  `docs/consoles.md`, "How a console builds"). The halves are chosen by
  compilation target, never by a Cargo feature: the host half links `leptos`
  with `ssr` and `nonce`, the wasm32 half with `hydrate`, and neither turns
  on `islands`. The pages sit in five lazy route groups (`crate::groups`),
  each loaded as chunks of its own, which cargo-leptos 0.3.11 builds with
  `--split` (`scripts/release/console-site.sh`). Every lane builds the site
  through that script. The terminology server serves no UI file.
- **Every page, the FHIR client and the catalog compile for both halves.**
  Only the configuration, the sign-in, the sessions, the form routes and the
  forwarding to the server (`crate::upstream`) are the host's alone.
- **A FHIR client and nothing else.** Every request is an ordinary request to
  the server's public FHIR API, built in `crate::fhir` and sent from the
  console's server (`crate::upstream`). The console depends on **no**
  workspace crate, not `fhir-terminology`, not `fhir-types`;
  `scripts/checks/console-boundary.sh` walks the resolved closure and fails on
  an edge into the engine.
- **No code system is a special case.** Every screen renders from
  `TerminologyCapabilities` and the operations; the test
  `pages::tests::no_screen_names_a_code_system` refuses a canonical written
  into a screen.

## 2. Requests, forms and the session

- **A page is read on the server, once.** Each screen is one async function
  from its `Request` to the page. The `read_page` server function
  (`crate::groups`) reads the visit (`crate::visit`) and runs the screen with
  a recording client (`crate::fhir::tape`); whichever half shows the page
  runs the screen again from that recording, so the browser hydrates what
  the server drew and draws a page after a navigation from one round trip.
  An exchange the recording does not hold goes through the `forward` server
  function (`crate::fhir::exchange`), which forwards only the paths
  `crate::upstream` allows.
- **What the server read of the request reaches the browser** as the visit:
  the preference cookies (never the session cookie), who is signed in and
  the scopes, the console's own settings a page names, the locale, and a
  posted form. The shell asks for it again after each navigation
  (`visit::FollowVisit`).
- **Every page works with no WebAssembly** (FerroUI's four layers). A read is
  a link or a `GET` form whose parameters live in the address (the
  operation runners, the listings, the concept browser, the history
  comparison). A write is a plain `POST` form to the page's own address: the
  server draws the page again with the posted fields in the visit, sends the
  write, and redirects to what it wrote or draws the refusal over the draft
  the reader keeps (`pages::editor`). A preference, the navigation rail and
  the session's FHIR version are form posts or links to the console's routes
  (`crate::post`); FerroUI's preference and rail forms post in the
  background and the router draws the page they return to.
- **A request that is not a safe method is taken only from the console's own
  pages** (`server::same_origin_only`, fetch metadata first), the server
  functions' posts among them.
- **The reader's sign-in lives on the console's server** (`crate::session`,
  `crate::oidc`): authorization code with PKCE at the configured OpenID
  Provider, the session in memory behind one opaque `HttpOnly` cookie. The
  page is told the reader's name and granted scopes (`auth::Operator`), which
  decide whether an edit control is drawn. The terminology server checks the
  token on every write. A sign-in's outcomes are the states of the sign-in
  page (`/auth/sign-in?state=…`).
- **Reading is open.** A visitor reads every screen, as the server's read
  surface asks for no token; the sign-in is for editing, and a console with
  no `[oidc]` offers none.
- **Preferences are cookies of the console's own**, one per preference:
  FerroUI's family preferences (theme, density, motion, hints, language,
  number format) under their own names, and the console's (default FHIR
  version, display language, page size) prefixed `ferroterm_console_`
  (`crate::prefs`). The server reads them while it draws, so the theme and
  the density are on `<html>` before any script runs. No browser storage,
  ever.

## 3. Rendering

- **A route renders whole on the server** (`SsrMode::Async`), so the first
  page arrives drawn and no loading state is ever sent.
- **The shell is drawn once** (`shell::console_shell`) and follows the
  router: the navigation marks the page in view, the version select and
  the user menu follow the address, and the parts that read the visit draw
  again when it changes.
- **A view closure the server would render once per pass is drawn once on
  the server, outside the closure**, as the page group and the layout do,
  so a page reads the server once and its recording keeps the id the
  browser hydrates it from.
- A resource page's tab, a runner's run, a listing's page, sort and filter,
  and the concept browser's open levels are query parameters, never
  fragments or client state.

## 4. Styling

- The components' stylesheet is `ferrohealth_ui::style::STYLESHEET`, served at
  `/ferrohealth-ui.css`; the brand is `ferrohealth_ui::style::Brand::Term`'s
  theme (FerroTERM teal), served at `/theme.css`. The console has no
  stylesheet of its own and no utility framework.
- `scripts/checks/console-tokens.sh` refuses a class that is not a FerroUI
  `fui-` class, a palette entry and an inline style.
- A screen draws with FerroUI components. What FerroUI does not have yet is
  filed on FerroHEALTH/FerroUI and drawn with the nearest FerroUI piece;
  nothing is built as a single-use component here.

## 5. Security headers

Every answer carries a Content-Security-Policy with scripts by a nonce minted
per response, `style-src 'self'` (no `style` attribute, no `<style>`),
`frame-ancestors 'none'` and `form-action` the console and the provider's
end-session origin, plus `nosniff`, `DENY` framing and no referrer; documents
are `no-store`, and only the site bundle is compressed (`server.rs`).

## 6. Testing and gates

- FerroUI's own gates (its `leptos-ui.md` §10) hold the library; the
  design checks of the console's mockup (`design/COMPONENTS.md` § Checks
  every console runs, `design/tools/check.sh`) run in FerroUI. The console
  runs the gates below over its own code.
- Business logic lives outside components, in plain types with unit tests
  (URL building, `OperationOutcome` flattening, the listing state, the
  authoring models, reading a posted form, the shortcuts' keys, the
  exchange's path check).
- `cargo fmt` and `leptosfmt` clean; `cargo clippy -p ferroterm-console
  --all-targets -- -D warnings` on the host and `cargo clippy -p
  ferroterm-console --lib --target wasm32-unknown-unknown -- -D warnings`;
  `cargo nextest run -p ferroterm-console`.
- The journeys wait for FerroUI's `data-ferroui-hydrated` mark before they
  act, and a press waits for either a new document or a page the router
  drew in place of the one it left (`e2e/tests/it/harness.rs`).
- **The journeys are a merge gate, Rust only**: `scripts/ui-e2e.sh` builds
  the server, the console and the stub issuer, starts them on loopback with
  chromedriver, and runs `e2e/` (thirtyfour). Every journey fails on a
  browser log at `SEVERE`; waits are explicit, never `sleep`; the painted
  colour walk in `e2e/tests/it/accessibility.rs` measures what the browser
  drew in both themes and both densities.

## 7. Bundle

`scripts/checks/console-bundle.sh` fails when the brotli-compressed entry
WebAssembly of the release site bundle is over the number below, which it
reads from this table. It reports the route groups' chunks beside it and
does not gate them: a page loads the entry and its own group's chunks, and
another group's only when the reader goes there.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 1153434 bytes |

The budget is 1.1 MiB, set with the console's first measured release
bundle: 1071380 bytes on ferrohealth-ui 0.1.4 (2026-10-19, cargo-leptos
0.3.11, wasm-bindgen 0.2.129, Rust 1.98.0) when the console was a whole
app hydrated in the browser. Hydrated whole again on ferrohealth-ui 0.5.6,
with its pages in five route groups, the entry is 590475 bytes (2026-10-28,
the same tools, Rust 1.99.1), 51% of the budget, and the groups' 25 chunks
967442 more. A change that needs more raises the budget here, in its own
pull request, with the measured size and the reason.

## 8. Strings and locales

The catalog rule is FerroUI's (its `.claude/rules/leptos-ui.md` §13): no
string the console draws lives outside a catalog, and a language joins as a
catalog file with no code change. What is particular here:

- The console's catalog is
  `app/ferroterm-console/locales/<tag>/ferroterm-console.ftl`, embedded by
  `fluent_templates::static_loader!` in `crate::locale`, where each message
  is one `Msg`. A test holds every catalog directory to the full message
  set, and a throwaway catalog under `tests/locales/` shows the words change
  with no code change.
- The server chooses the locale once per request (`crate::locale::chosen`):
  the language the reader chose in Your preferences when the console ships
  it, else the best language of `Accept-Language` the catalog holds, else
  `[server] locale`. `App` provides it with
  `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
  components' switch together, the document writes it as `<html lang>`, and
  the catalog formats numbers with FerroUI's number formatter
  (`number-group` and `number-decimal` in the catalog).
- A test holds every `tr_with` call to the variables its message names, so
  no screen falls back to a message key.
- `scripts/checks/stray-literals.sh app/ferroterm-console/src` runs in CI
  against `scripts/checks/stray-literals.baseline`: a file may hold no more
  stray literals than its count, and a change that moves strings into the
  catalog lowers the count with `--update`.
