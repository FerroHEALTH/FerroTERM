---
paths: ["app/ferroterm-viewer/**"]
---

# The FerroTERM Console's own rules (`app/ferroterm-viewer`)

**The component rules are FerroUI's**: `.claude/rules/leptos-ui.md` in
FerroHEALTH/FerroUI, the one rule set of every FerroHEALTH console, covers the
mandates (Rust only, no inline style, no server function in the library, no
browser storage), the two tiers of component, rendering in both halves,
reactivity, views, forms, tables and paging, accessibility, tokens and icons.
Read it first. This file keeps only what is particular to FerroTERM's console.
The oracle for a Leptos question is the official Leptos book
(<https://github.com/leptos-rs/book>, Leptos 0.8) through `/leptos-lookup`.

## 1. What the console is

- **A backend-for-frontend binary, rendered on the server and hydrated.**
  `ferroterm-viewer` (`serve`, `config check`, `healthcheck`) serves the
  pages, the server functions, the site bundle and the components' and the
  brand's stylesheets. The halves are chosen by compilation target, never by
  a Cargo feature: the host half links `leptos` with `ssr` and `nonce`, the
  wasm32 half with `hydrate`. cargo-leptos 0.3.11 builds it
  (`scripts/release/viewer-site.sh`). The terminology server serves no UI
  file.
- **A FHIR client and nothing else.** Every request is an ordinary request to
  the server's public FHIR API, built in `crate::fhir` and sent as one
  `fhir::exchange::Exchange`: the server half sends it itself
  (`crate::upstream`), the browser half through the `forward` server
  function, which sends it from the console's server. The console depends on
  **no** workspace crate, not `fhir-terminology`, not `fhir-types`;
  `scripts/checks/viewer-boundary.sh` walks the resolved closure and fails on
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
- `scripts/checks/viewer-tokens.sh` refuses a palette entry, a per-element
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

- Business logic lives outside components, in plain types with unit tests
  (URL building, `OperationOutcome` flattening, the paging arithmetic, the
  tree model, the authoring models, the exchange's path check).
- `cargo fmt` and `leptosfmt` clean; `cargo clippy -p ferroterm-viewer
  --all-targets -- -D warnings` on the host and `cargo clippy -p
  ferroterm-viewer --lib --target wasm32-unknown-unknown -- -D warnings`;
  `cargo nextest run -p ferroterm-viewer`.
- **The journeys are a merge gate, Rust only**: `scripts/ui-e2e.sh` builds
  the server, the console and the stub issuer, starts them on loopback with
  chromedriver, and runs `e2e/` (thirtyfour). Every journey fails on a
  browser log at `SEVERE`; waits are explicit, never `sleep`; the painted
  colour walk in `e2e/tests/it/accessibility.rs` measures what the browser
  drew in both themes and both densities.

## 7. Bundle

`scripts/checks/viewer-bundle.sh` fails when the brotli-compressed
WebAssembly of the release site bundle is over the number below, which it
reads from this table. The console carries the reader and the editor in one
bundle.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 1153434 bytes |

The budget is 1.1 MiB, set with the console's first measured release
bundle: 1068012 bytes on ferrohealth-ui 0.1.3 (2026-10-19, cargo-leptos
0.3.11, wasm-bindgen 0.2.129, Rust 1.98.0), 1066218 on 0.1.2, 1025446 on
0.1.1. The client-side reader and editor bundles it replaced were 534059 and
798540 bytes gzipped. FerroTERM issue 733 tracks winning headroom back. A
change that needs more raises the budget here, in its own pull request, with
the measured size and the reason.
