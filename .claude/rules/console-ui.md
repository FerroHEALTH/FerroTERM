---
paths: ["app/ferroterm-console/**", "e2e/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FerroTERM Console's own rules (`app/ferroterm-console`)

**FerroUI leads the console's UI.** FerroUI (`ferrohealth-ui` on crates.io,
repository FerroHEALTH/FerroUI) is the family's component library and the
one source of its console design and its component rules. Read these first,
before any console change:

- the consumer guide, [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/docs/consoles.md):
  how a console takes the library, what it keeps local, how it moves to a
  new release; and the move from 0.5,
  [`docs/migration-0.6.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/docs/migration-0.6.md);
- the component rules, [`.claude/rules/console-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/.claude/rules/console-ui.md):
  the mandates (§0: server-rendered HTML, a little plain JavaScript, no
  inline style, no route or state in the library, no patient identifier in
  a URL), the interaction contract (§1), templates and markup, modules,
  accessibility, tokens and themes, icons, the catalogs;
- the crate discipline, [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/crates/ferrohealth-ui/CLAUDE.md):
  the interaction contract and the rendering mode;
- the reviewed design, [`design/README.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/design/README.md),
  [`design/COMPONENTS.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/design/COMPONENTS.md)
  (the component IDs such as R9, F13 and M1 to M6, the family rules, the
  page templates) and the console's mockup under
  [`design/consoles/ferroterm/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.6.2/design/consoles/ferroterm).

**The pin decides the version you read.** The console pins `ferrohealth-ui
= "=0.6.2"` (root `Cargo.toml`), so every FerroUI document above is read at
the tag `v0.6.2`. FerroUI's `design/` is the only design source: a design
folder anywhere else is stale and never synced from.

**Nothing FerroUI has or should have is built here.** A missing component,
a wrong behaviour in one, a stylesheet override or a script the console
would need is filed on FerroHEALTH/FerroUI and linked from the console's
issue, and the console stops that part until the release that ships it. The
console writes no script and no component of its own: never a local copy, a
shadow component, a local stylesheet override or a hand-written script.

This file keeps only what is particular to FerroTERM's console. The oracles
for a markup or behaviour question FerroUI does not settle are the HTML
Living Standard, WCAG 2.2 and WAI-ARIA 1.2, and for the HTTP surface the
axum documentation of the pinned release.

## 1. What the console is

- **A backend-for-frontend binary whose server draws every page.**
  `ferroterm-console` (`serve`, `config check`, `healthcheck`) is one axum
  service: every page is a handler that reads the terminology server and
  answers the whole document (`crate::server::page`, `crate::app`), and a
  link or a form loads the next page as one document (FerroUI
  `docs/consoles.md`, "How a console builds"). FerroUI's script modules run
  over what the server drew: the tree folds a level in place, the menus and
  dialogs open, the search suggests as you type, the preference forms post
  on change. Every page works with them refused. One `cargo build` builds
  it: no WebAssembly, no site bundle, no Node. The terminology server
  serves no UI file.
- **A FHIR client and nothing else.** Every request is an ordinary request
  to the server's public FHIR API, built in `crate::fhir` and sent from the
  console's server (`crate::upstream::Upstream`). The console depends on
  **no** workspace crate, not `fhir-terminology`, not `fhir-types`;
  `scripts/checks/console-boundary.sh` walks the resolved closure and fails
  on an edge into the engine.
- **No code system is a special case.** Every screen renders from
  `TerminologyCapabilities` and the operations; the test
  `pages::tests::no_screen_names_a_code_system` refuses a canonical written
  into a screen.

## 2. Requests, forms and the session

- **A page is read on the server, once.** Each screen is one async
  function from its `Request` to its content (`crate::screen::draw`), which
  reads what it shows and draws it with FerroUI's components and the
  console's own markup (`crate::markup`, every value escaped). What a
  screen says of its answer (its title, its status, a redirect in its
  place) it says through `Request::answering`, and the handler writes the
  document or the `303` from it.
- **What the server read of the request** is the visit (`crate::visit`):
  the preference cookies (never the session cookie), who is signed in and
  the scopes, what the session has left, the console's own settings a page
  names, the locale, a posted form and the way to the server as the
  reader. A screen reads no cookie, session or request of its own.
- **Every page works with no script** (FerroUI's interaction contract). A
  read is a link or a `GET` form whose parameters live in the address (the
  operation runners, the listings, the concept browser and its tree's
  toggles, the history comparison). A write is a plain `POST` form to the
  page's own address: the server draws the page again with the posted
  fields, sends the write, and redirects to what it wrote or draws the
  refusal over the draft the reader keeps (`pages::editor`). A preference,
  the navigation rail and the session's FHIR version are form posts or
  links to the console's routes (`crate::post`).
- **A request that is not a safe method is taken only from the console's
  own pages** (`server::same_origin_only`, fetch metadata first).
- **The reader's sign-in lives on the console's server** (`crate::session`,
  `crate::oidc`): authorization code with PKCE at the configured OpenID
  Provider, the session in memory behind one opaque `HttpOnly` cookie. The
  page is told the reader's name and granted scopes (`auth::Operator`),
  which decide whether an edit control is drawn. The terminology server
  checks the token on every write. A sign-in's outcomes are the states of
  the sign-in page (`/auth/sign-in?state=…`).
- **A signed-in page watches its session** (FerroUI `docs/consoles.md`,
  "The session's end"): the shell draws `view::session::session_watch`
  from what the session had left as the server drew the page, the sign-in
  page of an ended session draws `view::session::session_ended`, and
  `session::watch` answers the session route `/auth/session`, where `GET`
  extends nothing and `POST`, the reader's own activity, starts the idle
  timeout again.
- **Reading is open.** A visitor reads every screen, as the server's read
  surface asks for no token; the sign-in is for editing, and a console with
  no `[oidc]` offers none.
- **Preferences are cookies of the console's own**, one per preference:
  FerroUI's family preferences (theme, density, motion, hints, language,
  number format) under their own names, and the console's (default FHIR
  version, display language, page size) prefixed `ferroterm_console_`
  (`crate::prefs`). The server reads them while it draws, so the theme and
  the density are on `<html>` before any script runs. No browser storage of
  the console's own; FerroUI's tree keeps its folds for the session.

## 3. Rendering

- **Every page is one document** written by `view::shell::document`
  (`crate::app`): the stylesheet, the FerroTERM theme and the one loader
  script, all FerroUI's, served by its router under
  `/ferrohealth-ui/<version>/`, and the brand's favicon and marks served
  from the binary (`server::BRAND_FILES`).
- **The shell is drawn by every page** (`shell::console_shell`) from its own
  request: the navigation marks the page in view, the version select names
  its address, the status pill and the development banner say how the
  server answered the probe the handler sent beside the page's own read.
- A resource page's tab, a runner's run, a listing's page, sort and filter,
  and the concept browser's open levels are query parameters, never
  fragments or client state; the tree's in-place folds are FerroUI's, kept
  for the session.

## 4. Styling

- The components' stylesheet and the brand's theme are FerroUI's
  (`ferrohealth_ui::serve::Assets::of(Brand::Term)`). The console has no
  stylesheet of its own and no utility framework.
- `scripts/checks/console-tokens.sh` refuses a class that is not a FerroUI
  `fui-` class, a palette entry and an inline style.
- A screen draws with FerroUI components. What FerroUI does not have yet is
  filed on FerroHEALTH/FerroUI and drawn with the nearest FerroUI piece;
  nothing is built as a single-use component here.

## 5. Security headers

Every answer carries the Content-Security-Policy of a server-rendered
console (`ConsoleHeaders::server_rendered` of `ferrohealth-server`
0.4.2): scripts and styles from the console alone with no nonce
(`script-src 'self'; style-src 'self'`, no `style` attribute, no `<style>`,
no inline script), `frame-ancestors 'none'` and `form-action` the console and
the provider's end-session origin, beside `ferrohealth-server`'s `nosniff`,
`DENY` framing and no referrer; documents are `no-store`, FerroUI's
versioned assets are cached for a year, and no answer is compressed.

## 6. Testing and gates

- FerroUI's own gates hold the library; the design checks of the console's
  mockup (`design/COMPONENTS.md` § Checks every console runs) run in
  FerroUI. The console runs the gates below over its own code.
- Business logic lives outside the markup, in plain types with unit tests
  (URL building, `OperationOutcome` flattening, the listing state, the
  authoring models, reading a posted form, the exchange's path check), and
  a part of a page is tested by rendering it to a string and asserting on
  the markup.
- `cargo fmt` clean; `cargo clippy -p ferroterm-console --all-targets --
  -D warnings`; `cargo nextest run -p ferroterm-console`.
- The journeys wait for FerroUI's loader mark, `html[data-fui-script]`,
  and for every element that asks for a module to carry `data-fui-ready`
  before they act, and a press waits for the new document it loads
  (`e2e/tests/it/harness.rs`). `Journey::open_without_scripts` refuses the
  browser every module: `no_script` holds every page to its content, its
  links and its forms, `navigation` the tree's in-place folds over a
  reload, and `reflow` every screen at 320 px with the modules loaded and
  refused.
- **The journeys are a merge gate, Rust only**: `scripts/ui-e2e.sh` builds
  the server, the console and the stub issuer, starts them on loopback
  with chromedriver, and runs `e2e/` (thirtyfour). Every journey fails on a
  browser log at `SEVERE` (a refused module aside, where a journey refuses
  them); waits are explicit, never `sleep`; the painted colour walk in
  `e2e/tests/it/accessibility.rs` measures what the browser drew in both
  themes and both densities.

## 7. Strings and locales

The catalog rule is FerroUI's (its `.claude/rules/console-ui.md`): no
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
  `[server] locale`. The request carries it, every lookup names it
  (`locale::tr(locale, msg)`) and every FerroUI component takes it, so the
  console's words and the components' switch together, the document writes
  it as `<html lang>`, and the catalog formats numbers with FerroUI's
  number formatter (`number-group` and `number-decimal` in the catalog).
- A test holds every `tr_with` call to the variables its message names, so
  no screen falls back to a message key.
- `scripts/checks/stray-literals.sh app/ferroterm-console/src` runs in CI
  against `scripts/checks/stray-literals.baseline`: a file may hold no more
  stray literals than its count, and a change that moves strings into the
  catalog lowers the count with `--update`.
