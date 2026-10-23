---
name: leptos-lookup
description: >
  Finds and reads the authoritative guidance for a console UI topic: first
  in FerroUI, which leads the UI (its consumer guide, component rules,
  crate discipline and design with the component IDs, at the tag matching
  the pin), then in the official Leptos book, cached locally (signals,
  effects, <For>, resources, Suspense and Transition, router, forms, server
  rendering and hydration, server functions, islands, binary size,
  cargo-leptos). Use before implementing or reviewing any console
  (app/ferroterm-console) behaviour the rule file does not fully settle, or
  when a "how does Leptos do X" question comes up.
allowed-tools: Read, Grep, Glob, Bash
argument-hint: "<signal / resource / router / form / ssr / bundle-size topic>"
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /leptos-lookup

Answer UI questions from the source text, never from memory. Leptos is
pre-1.0 and moves, so training data drifts. FerroUI leads the console's UI,
so its documents come first; the Leptos book answers what FerroUI does not
settle. The console's own rules live in `.claude/rules/leptos-ui.md`.

**The console is drawn on the server in islands mode** (`leptos-ui.md` §1
to §3), so the book's SSR and "Guide: Islands" chapters apply; its CSR-only
chapters do not, and the console has no server function.

## Procedure

1. **Read FerroUI first** (FerroHEALTH/FerroUI; a local checkout or
   GitHub). Take the tag from the console's pin (`ferrohealth-ui` in the
   root `Cargo.toml`, `=0.3.4` today, tag `v0.3.4`):

   ```bash
   UI=~/.cache/ferroterm/ferroui
   [ -d "$UI/.git" ] || git clone https://github.com/FerroHEALTH/FerroUI "$UI"
   git -C "$UI" fetch --tags -q origin
   TAG="v$(sed -n 's/^ferrohealth-ui = "=\(.*\)"/\1/p' Cargo.toml)"
   git -C "$UI" show "$TAG:.claude/rules/leptos-ui.md"
   ```

   Route by topic: how a console takes the library, what stays local and
   how it moves to a release: `docs/consoles.md`; a component rule
   (reactivity, views, forms, tables, accessibility, tokens, bundle,
   catalogs): `.claude/rules/leptos-ui.md`; the interaction contract,
   islands mode and the ready islands: `crates/ferrohealth-ui/CLAUDE.md`;
   what a page looks like, a component ID (R9, F13, M1) or a family rule:
   `design/COMPONENTS.md`, `design/README.md` and
   `design/consoles/ferroterm/`. A document missing at `$TAG` is read on
   `origin/main` (`git -C "$UI" show origin/main:<path>`), and the answer
   says which version it came from. When FerroUI settles the question,
   stop here and cite the file and section.

2. **Ensure the book cache exists** (shared, survives sessions):

   ```bash
   BOOK=~/.cache/ferroterm/leptos-book
   [ -d "$BOOK/src" ] || git clone --depth 1 https://github.com/leptos-rs/book "$BOOK"
   ```

   If the cache is older than about 30 days
   (`git -C "$BOOK" log -1 --format=%cr`), `git -C "$BOOK" pull --ff-only`
   first. The book's `main` targets the current Leptos 0.x line, so
   cross-check any version-sensitive answer against the pinned version in the
   console crate's `Cargo.toml`.

3. **Route to the owning chapter** (`src/SUMMARY.md` is the index):
   - server rendering, hydration and its bugs: `src/ssr/*`; server
     functions: `src/server/*`; islands: `src/islands.md`
   - signals, effects, memos, derived values: `src/reactivity/*`, plus the
     appendices `appendix_reactive_graph.md`, `appendix_life_cycle.md`
   - components, props, children, context: `src/view/03…09_*`,
     `src/interlude_projecting_children.md`
   - lists and keys: `src/view/04_iteration.md`, `04b_iteration.md`
   - control flow and errors: `src/view/06…07_*`
   - forms: `src/view/05_forms.md`
   - async, `LocalResource`, `Suspense`, `Transition`, `Action`:
     `src/async/*` (note `10_resources.md` on the `Resource` versus
     `LocalResource` split)
   - router, params, queries: `src/router/*`; global state:
     `src/15_global_state.md`
   - WebAssembly size and deployment: `src/deployment/*`
   - styling: `src/interlude_styling.md`; head and metadata:
     `src/metadata.md`; testing: `src/testing.md`; `web_sys` and JS interop:
     `src/web_sys.md`

4. **Grep** the exact API name (`LocalResource`, `Suspend`, `ForEnumerate`,
   `bind:value`, `use_query`) across `src/**/*.md` when the routing is not
   obvious.

5. **Read the surrounding section**, including the warning blocks, because the
   book's prohibitions live there.

6. **Answer with citations** (`<chapter>.md` plus the heading). For API
   signatures beyond the book, follow its docs.rs links rather than guessing;
   Context7 is the preferred fetcher for a pinned crate's documentation.
   cargo-leptos questions go to its README
   (<https://github.com/leptos-rs/cargo-leptos>), not the book.
   If the answer belongs in the standing rules, propose the addition
   explicitly: a library rule to FerroUI's `.claude/rules/leptos-ui.md` as
   a FerroUI issue, a console-only rule to this repository's
   `.claude/rules/leptos-ui.md`.
