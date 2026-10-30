<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ui-reviewer memory

One line per confirmed hazard of the server-drawn console, each with the
rule-file citation (`.claude/rules/console-ui.md` or FerroUI's at the pinned
tag). The Leptos-era memories went with the move to FerroUI 0.6 (#944): their
hazards (signals, `<For>` keys, hydration) no longer exist in the console.

- A value a screen writes into its own markup goes through `markup::t` or a
  library part; a `format!` of a raw `String` into HTML is an injection
  (`console-ui.md` §2).
- Every action has a link or a form behind it; a control that needs a
  FerroUI module to act at all is a blocker, held by the `no_script`
  journeys (`console-ui.md` §6).
