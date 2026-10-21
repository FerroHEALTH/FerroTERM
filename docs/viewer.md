<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FerroTERM Console

No FHIR or SNOMED CT specification governs a terminology server's user
interface: this is our own design. What the console *reads* is spec-bound, and
every citation below points at the FHIR specification, OpenID Connect, the
Leptos book, or a pinned crate's own documentation.

FerroTERM answers the FHIR terminology API and nothing else. Someone who has
just started it cannot see what it loaded, browse a hierarchy, or try an
expansion without writing a request by hand. The console closes that gap: a
dashboard of what the server serves, listings and detail screens for its code
systems, value sets and concept maps, a runner for each terminology operation,
and editors for the resources a deployment writes.

## 1. The shape

**The console is its own binary and its own image, a backend-for-frontend in
front of the server.** `ferroterm-viewer` renders every page on the server and
hydrates it in the browser (Leptos 0.8, the book's `ssr` chapters). It reaches
the terminology server over the server's public FHIR API, server to server,
and the terminology server serves no user interface of its own.

The console is one of four FerroHEALTH consoles (FerroEHR, FerroFED, FerroTERM
and FerroBRIDGE) that share one component library, `ferrohealth-ui`, one
shape, and one operator experience. That is the reason for the shape:

- **One family.** The four consoles draw the same shell, navigation, type
  scale, spacing, tables, pagers, forms and empty, loading and error states
  from `ferrohealth-ui`; FerroTERM keeps its teal brand through the library's
  `Brand::Term` theme. A screen draws with the library's components; what the
  library lacks is added to it, never built in one console.
- **No token in the browser.** The reader signs in at the console's server
  (authorization code with PKCE, OpenID Connect Core 1.0 §3.1), which keeps
  the session in memory behind one `HttpOnly` cookie and presents the
  reader's access token to the terminology server. The browser is told only
  the reader's name and granted scopes, which decide whether an edit control
  is drawn. The terminology server checks the token on every write as before.
- **The API-completeness claim still holds.** The console sends nothing but
  ordinary requests to the server's public FHIR API: the browser half asks the
  console's server to send them through one server function, `forward`, whose
  target is checked against the four FHIR roots, `/health` and the sync
  service's run list. Anything the console can do, a client can do.
- **Reading is open.** The server's read surface asks for no token, so a
  visitor reads every screen. The sign-in is for editing, and a console
  configured with no `[oidc]` offers none and no authoring screen.

## 2. The stack

| Concern | Choice |
|---|---|
| Framework | Leptos 0.8, `leptos_axum`, `leptos_router`, `leptos_meta` |
| Halves | chosen by compilation target, never by a Cargo feature: the host half links `leptos` with `ssr`, the `wasm32-unknown-unknown` half with `hydrate` |
| Rendering | every route `SsrMode::Async`: the server waits for the reads a screen exists to show, so no content waits on a script |
| Components | `ferrohealth-ui` (crates.io), its stylesheet served at `/ferrohealth-ui.css` and the FerroTERM brand at `/theme.css` |
| Build | cargo-leptos (`scripts/release/viewer-site.sh`), `wasm-opt` pinned and checked by SHA-256 (`scripts/release/viewer-tools.sh`); no Node |
| Image | `docker/viewer/Dockerfile`, distroless static, the binary and the site bundle the release lane stages |

## 3. Screens

| Screen | Address | What it reads |
|---|---|---|
| Dashboard | `/` | counts of code systems, value sets and concept maps, the newest writes from the type-level `_history` of each, the first code systems |
| Code systems | `/systems` | `TerminologyCapabilities`, named from the published `CodeSystem` resources; filter, sort and pager |
| Code system | `/systems/{canonical}` | the published resource (Definition), what the server declares per version (Server support), the resource as sent (JSON) |
| Concept browser | `/browse` | `$expand` with a text filter, one level of `child-of` at a time, `$lookup` for the concept read |
| Value sets | `/valuesets`, `/valuesets/{id}` | the search, the read, a first page of `$expand` |
| Concept maps | `/conceptmaps`, `/conceptmaps/{id}` | the search, the read, every mapping it writes |
| Runners | `/lookup`, `/validate`, `/expand`, `/translate` | `$lookup`, `$validate-code` and `$subsumes`, `$expand`, `$translate` |
| Find | `/find` | what the command bar was given, read by its shape, and the screens that take it |
| Editors | `/codesystem`, `/compose`, `/conceptmap` | create and update, `If-Match` on every update; drawn only on a console with a sign-in |
| Versions | `/history` | the instance `_history`, a comparison of two versions, a restore |
| About | `/about` | the four `CapabilityStatement` documents side by side, the evidence the build ships, the console's preferences |

No code system is a special case: every screen renders from
`TerminologyCapabilities` and the operations, and
`pages::tests::no_screen_names_a_code_system` refuses a canonical written into
a screen.

## 4. State

Every parameter a screen runs with lives in its address, so a screen is a link
a reader can send and the back button walks it. Preferences (theme, density,
the default FHIR version, the display language, the page size) are the
console's own cookies, written by plain form posts and read while the page
renders, so the theme and the density are on `<html>` before any script runs.
Nothing is kept in browser storage; the runs a reader made live in the page.

## 5. Security

Every answer carries a Content-Security-Policy with scripts by a nonce minted
per response, `style-src 'self'` (no `style` attribute, no `<style>`),
`frame-ancestors 'none'` and `form-action` the console and the provider's
end-session origin, plus `nosniff`, `DENY` framing and no referrer. Documents
and server function answers are `no-store`. A request that is not a safe
method must come from the console's own origin. The session cookie is
`__Host-`, `Secure`, `HttpOnly` and `SameSite=Lax`; a sign-out ends the
session, revokes its token at the provider when it publishes a revocation
endpoint (RFC 7009 §2.1), and follows the provider's end-session endpoint
(OpenID Connect RP-Initiated Logout 1.0 §2).

## 6. Testing

Business logic lives outside components, in plain types with unit tests. The
journeys (`e2e/`, thirtyfour) are the merge gate for what a reader sees:
`scripts/ui-e2e.sh` starts two servers, the stub identity provider and two
consoles on loopback and drives Chrome for Testing through chromedriver. They
cover every screen, the sign-in and sign-out, the authoring round trips, the
keyboard paths, and the colours the browser painted against WCAG 2.2 AA in
both themes and both densities. The bundle budget is in
`.claude/rules/leptos-ui.md`, checked by `scripts/checks/viewer-bundle.sh`.
