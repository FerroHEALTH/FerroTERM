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
- the component rules, [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.2.0/.claude/rules/leptos-ui.md):
  the mandates (§0: Rust only, no inline style, no server function in the
  library, no browser storage, no patient identifier in a URL), the two
  tiers (§1), reactivity, views, forms, tables and paging, accessibility,
  tokens and themes, icons, the bundle (§12) and the catalogs (§13); on
  `main` the same file adds the four-layer interaction contract (§1), the
  book's rules (§14) and the island rules (§15);
- the crate discipline, [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/main/crates/ferrohealth-ui/CLAUDE.md):
  the interaction contract, islands mode and the list of ready islands;
- the reviewed design, [`design/README.md`](https://github.com/FerroHEALTH/FerroUI/blob/main/design/README.md),
  [`design/COMPONENTS.md`](https://github.com/FerroHEALTH/FerroUI/blob/main/design/COMPONENTS.md)
  (the component IDs such as R9, F13 and M1 to M6, the family rules, the
  page templates) and the console's mockup under
  [`design/consoles/ferroterm/`](https://github.com/FerroHEALTH/FerroUI/tree/main/design/consoles/ferroterm).

**The pin decides the version you read.** The console pins `ferrohealth-ui
= "=0.2.0"` (root `Cargo.toml`), so its component rules are read at the tag
`v0.2.0`. The guide, the crate discipline and the design entered FerroUI
after that tag and are read on `main` until the pin reaches a tag that
carries them. The console moves to FerroUI 0.3.x, islands mode and the
four-layer interaction contract in the console rollout
(FerroHEALTH/FerroUI#161, after FerroUI 0.3.2); new UI work waits for it.
FerroUI's `design/` is the only design source: a design folder anywhere
else is stale and never synced from.

**Nothing FerroUI has or should have is built here.** A missing component,
a wrong behaviour in one, a stylesheet override or a script the console
would need is filed on FerroHEALTH/FerroUI and linked from the console's
issue (§4). Never a local copy, a shadow component, a local stylesheet
override or a hand-written script.

This file keeps only what is particular to FerroTERM's console. The oracle
for a Leptos question FerroUI does not settle is the official Leptos book
(<https://github.com/leptos-rs/book>, Leptos 0.8) through `/leptos-lookup`.

## 1. What the console is

- **A backend-for-frontend binary, rendered on the server and hydrated.**
  `ferroterm-console` (`serve`, `config check`, `healthcheck`) serves the
  pages, the server functions, the site bundle and the components' and the
  brand's stylesheets. The halves are chosen by compilation target, never by
  a Cargo feature: the host half links `leptos` with `ssr` and `nonce`, the
  wasm32 half with `hydrate`. cargo-leptos 0.3.11 builds it
  (`scripts/release/console-site.sh`). The terminology server serves no UI
  file.
- **A FHIR client and nothing else.** Every request is an ordinary request to
  the server's public FHIR API, built in `crate::fhir` and sent as one
  `fhir::exchange::Exchange`: the server half sends it itself
  (`crate::upstream`), the browser half through the `forward` server
  function, which sends it from the console's server. The console depends on
  **no** workspace crate, not `fhir-terminology`, not `fhir-types`;
  `scripts/checks/console-boundary.sh` walks the resolved closure and fails on
  an edge into the engine.
- **No code system is a special case.** Every screen renders from
  `TerminologyCapabilities` and the operations; the test
  `pages::tests::no_screen_names_a_code_system` refuses a canonical written
  into a screen.

## 2. Server functions and the session

- **Every server function is a `POST` and reads the session itself**, since
  each is a public endpoint (the Leptos book, `server/25_server_functions`):
  `fhir::exchange::forward`, `settings::set_theme`, `set_version`,
  `save_settings`. A function that takes a value checks it (`upstream::target`
  forwards only the four FHIR roots, `/health` and the sync service's `/runs`;
  `settings::return_path` returns only to the console's own pages).
- **The reader's sign-in lives on the console's server** (`crate::session`,
  `crate::oidc`): authorization code with PKCE at the configured OpenID
  Provider, the session in memory behind one opaque `HttpOnly` cookie. The
  browser never holds a token; it is told the reader's name and granted
  scopes (`auth::Operator`), which decide whether an edit control is drawn.
  The terminology server checks the token on every write as before.
- **Reading is open.** A visitor reads every screen, as the server's read
  surface asks for no token; the sign-in is for editing, and a console with
  no `[oidc]` offers none.
- **Preferences are cookies of the console's own** (theme, density, default
  FHIR version, display language, page size), written by a plain form post
  and read while the page renders, so the theme and the density are on
  `<html>` before any script runs. No browser storage, ever; the runs a
  reader made live in the page alone (`runs::RunList`).

## 3. Rendering

- **A route renders whole on the server** (`SsrMode::Async`): content a
  screen exists to show is a `Resource` read inside a `<Transition>` (the
  `Reading` section or an explicit one), so the server waits for it and the
  browser hydrates from what the server sent.
- **A read a memo outside every suspense boundary consumes is a
  `LocalResource`**, the browser's alone: the capability documents a form is
  shaped by, the offers a picker makes. Reading a `Resource` outside a
  boundary is what made hydration fail; the browser's log says so in
  development, and a journey fails on it.
- **A live region's text reads its resource inside a `<Transition>`**, never
  through a memo built outside one.
- An input written from the address carries `value` for the server's render
  and `prop:value` for the hydrated page; an option says itself whether it is
  `selected`, because the options of a picker can arrive after the select.

## 4. Styling

- The components' stylesheet is `ferrohealth_ui::style::STYLESHEET`, served at
  `/ferrohealth-ui.css`; the brand is `ferrohealth_ui::style::Brand::Term`'s
  theme (FerroTERM teal), served at `/theme.css`; the
  console's own utilities are Tailwind 4 over `style/tailwind.css`, which
  imports Tailwind's theme and utilities and **not its preflight** (it would
  reset what the components' stylesheet draws), and maps the FerroUI roles,
  steps and spacing in an `@theme inline reference` block.
- `scripts/checks/console-tokens.sh` refuses a palette entry, a per-element
  `dark:` variant, a raw type size and a spacing value off the four steps.
- A screen draws with FerroUI components. What FerroUI does not have yet is
  filed on FerroHEALTH/FerroUI and drawn with the nearest FerroUI piece, its
  place marked `TODO(#730)`; nothing is built as a single-use component here.
- A detail screen's tab is a query parameter (`tab`, `components::detail`),
  never a fragment: the server renders the pane the address names, so the
  page hydrates without a mismatch.

## 5. Security headers

Every answer carries a Content-Security-Policy with scripts by a nonce minted
per response, `style-src 'self'` (no `style` attribute, no `<style>`),
`frame-ancestors 'none'` and `form-action` the console and the provider's
end-session origin, plus `nosniff`, `DENY` framing and no referrer; documents
and server function answers are `no-store`, and only the site bundle is
compressed (`server.rs`).

## 6. Testing and gates

- FerroUI's own gates (its `leptos-ui.md` §10) hold the library; the
  design checks of the console's mockup (`design/COMPONENTS.md` § Checks
  every console runs, `design/tools/check.sh`) run in FerroUI. The console
  runs the gates below over its own code.
- Business logic lives outside components, in plain types with unit tests
  (URL building, `OperationOutcome` flattening, the paging arithmetic, the
  tree model, the authoring models, the exchange's path check).
- `cargo fmt` and `leptosfmt` clean; `cargo clippy -p ferroterm-console
  --all-targets -- -D warnings` on the host and `cargo clippy -p
  ferroterm-console --lib --target wasm32-unknown-unknown -- -D warnings`;
  `cargo nextest run -p ferroterm-console`.
- **The journeys are a merge gate, Rust only**: `scripts/ui-e2e.sh` builds
  the server, the console and the stub issuer, starts them on loopback with
  chromedriver, and runs `e2e/` (thirtyfour). Every journey fails on a
  browser log at `SEVERE`; waits are explicit, never `sleep`; the painted
  colour walk in `e2e/tests/it/accessibility.rs` measures what the browser
  drew in both themes and both densities.

## 7. Bundle

`scripts/checks/console-bundle.sh` fails when the brotli-compressed
WebAssembly of the release site bundle is over the number below, which it
reads from this table. The console carries the reader and the editor in one
bundle.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 1153434 bytes |

The budget is 1.1 MiB, set with the console's first measured release
bundle: 1071380 bytes on ferrohealth-ui 0.1.4 (2026-10-19, cargo-leptos
0.3.11, wasm-bindgen 0.2.129, Rust 1.98.0), 1122043 on 0.1.7 (its Fluent catalogs: the parser, the CLDR plural rules and the catalogs), 1069909 on 0.1.6, 1071227 on 0.1.5, 1068012 on 0.1.3, 1066218 on
0.1.2, 1025446 on 0.1.1. The client-side reader and editor bundles it replaced were 534059 and
798540 bytes gzipped. FerroTERM issue 733 tracks winning headroom back. A
change that needs more raises the budget here, in its own pull request, with
the measured size and the reason.

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
- The server chooses the locale once per request (`crate::locale::requested`):
  the best language of `Accept-Language` the console's catalog holds, else
  `[server] locale`. The tag travels in `Facts` to the browser half, `App`
  provides it with `ferrohealth_ui::i18n::provide_locale` so the console's
  words and the components' switch together, and the document writes it as
  `<html lang>`.
- `scripts/checks/stray-literals.sh app/ferroterm-console/src` runs in CI
  against `scripts/checks/stray-literals.baseline`: a file may hold no more
  stray literals than its count, and a change that moves strings into the
  catalog lowers the count with `--update`. Most of the remaining strings
  reach a component that takes a `&'static str`, which FerroHEALTH/FerroUI#114
  widens.
