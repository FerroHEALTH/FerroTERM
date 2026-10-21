---
name: leptos-lookup
description: >
  Finds and reads the authoritative Leptos guidance for a topic (signals,
  effects, <For>, LocalResource, Suspense and Transition, router, forms,
  server rendering and hydration, binary size, cargo-leptos) in the official Leptos book,
  cached locally. Use before implementing or reviewing any viewer
  (app/ferroterm-viewer) behaviour the rule file does not fully settle, or
  when a "how does Leptos do X" question comes up.
allowed-tools: Read, Grep, Glob, Bash
argument-hint: "<signal / resource / router / form / ssr / bundle-size topic>"
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /leptos-lookup

Answer Leptos questions from the official book text, never from memory. Leptos
is pre-1.0 and moves, so training data drifts. The distilled rules live in
`.claude/rules/leptos-ui.md`; this skill goes back to the source when a case is
not covered or needs full context.

**The viewer is client-side rendered.** Read the CSR chapters. The book's SSR,
hydration, server-function, and `cargo-leptos` chapters describe a shape this
project does not use; read them only to confirm that something does not apply,
and say so explicitly when you cite one.

## Procedure

1. **Ensure the book cache exists** (shared, survives sessions):

   ```bash
   BOOK=~/.cache/ferroterm/leptos-book
   [ -d "$BOOK/src" ] || git clone --depth 1 https://github.com/leptos-rs/book "$BOOK"
   ```

   If the cache is older than about 30 days
   (`git -C "$BOOK" log -1 --format=%cr`), `git -C "$BOOK" pull --ff-only`
   first. The book's `main` targets the current Leptos 0.x line, so
   cross-check any version-sensitive answer against the pinned version in the
   viewer crate's `Cargo.toml`.

2. **Route to the owning chapter** (`src/SUMMARY.md` is the index):
   - getting started, CSR, mounting: `src/getting_started/README.md`,
     `src/csr_wrapping_up.md`
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
   - WebAssembly size and deployment: `src/deployment/*` (CSR deployment is
     `src/deployment/csr.md`)
   - styling: `src/interlude_styling.md`; head and metadata:
     `src/metadata.md`; testing: `src/testing.md`; `web_sys` and JS interop:
     `src/web_sys.md`

3. **Grep** the exact API name (`LocalResource`, `Suspend`, `ForEnumerate`,
   `bind:value`, `use_query`) across `src/**/*.md` when the routing is not
   obvious.

4. **Read the surrounding section**, including the warning blocks, because the
   book's prohibitions live there.

5. **Answer with citations** (`<chapter>.md` plus the heading). For API
   signatures beyond the book, follow its docs.rs links rather than guessing;
   Context7 is the preferred fetcher for a pinned crate's documentation.
   cargo-leptos questions go to its README
   (<https://github.com/leptos-rs/cargo-leptos>), not the book.
   If the answer belongs in the standing rules, propose the
   `.claude/rules/leptos-ui.md` addition explicitly.
