---
name: ferroui-lookup
description: >
  Finds and reads the authoritative guidance for a console UI topic in
  FerroUI, which leads the UI: its consumer guide, its move from 0.5, its
  component rules, the crate discipline and the design with the component
  IDs, at the tag matching the pin, and the source of the part in question.
  Use before implementing or reviewing any console (app/ferroterm-console)
  behaviour the rule file does not fully settle, or when a "which FerroUI
  part draws X" question comes up.
allowed-tools: Read, Grep, Glob, Bash
argument-hint: "<component / module / page template / migration topic>"
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /ferroui-lookup

Answer UI questions from the source text, never from memory. FerroUI is
pre-1.0 and moves, so training data drifts. The console's own rules live in
`.claude/rules/console-ui.md`.

**The console draws every page on its server** (`console-ui.md` §1 to §3):
FerroUI's components return HTML, its plain script modules run over it, and
every page works with them refused.

## Procedure

1. **Read FerroUI at the pinned tag** (FerroHEALTH/FerroUI; a local
   checkout or GitHub). Take the tag from the console's pin
   (`ferrohealth-ui` in the root `Cargo.toml`, `=0.6.0` today, tag
   `v0.6.0`):

   ```bash
   UI=~/.cache/ferroterm/ferroui
   [ -d "$UI/.git" ] || git clone https://github.com/FerroHEALTH/FerroUI "$UI"
   git -C "$UI" fetch --tags -q origin
   TAG="v$(sed -n 's/^ferrohealth-ui = "=\(.*\)"/\1/p' Cargo.toml)"
   git -C "$UI" show "$TAG:docs/consoles.md"
   ```

2. **Route by topic**: how a console takes the library, what stays local
   and how it moves to a release: `docs/consoles.md`; the move from a 0.5
   part to its successor: `docs/migration-0.6.md`; a component rule
   (templates, modules, accessibility, tokens, catalogs):
   `.claude/rules/console-ui.md`; the interaction contract and the
   rendering mode: `crates/ferrohealth-ui/CLAUDE.md`; what a page looks
   like, a component ID (R9, F13, M1) or a family rule:
   `design/COMPONENTS.md`, `design/README.md` and
   `design/consoles/ferroterm/`.

3. **Read the part itself** when the documents leave a signature or a
   behaviour open: `crates/ferrohealth-ui/src/view/<part>.rs`, its template
   under `crates/ferrohealth-ui/templates/`, its module under
   `crates/ferrohealth-ui/scripts/`, and the gallery's page and journeys
   under `app/ferrohealth-ui-gallery/` as the worked example
   (`git -C "$UI" show "$TAG:<path>"`, `git -C "$UI" grep <name> "$TAG"`).

4. **Answer with citations** (the file at the tag, and its section or item).
   A document missing at `$TAG` is read on `origin/main`, and the answer says
   which version it came from. If FerroUI lacks the part or draws it wrongly,
   the answer is a FerroUI issue to file (what is missing, the component ID
   it belongs to, the console page that needs it), never a local copy; a
   console-only rule goes to this repository's `.claude/rules/console-ui.md`.
