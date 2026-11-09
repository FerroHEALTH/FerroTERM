---
name: sibling-projects
description: The other FerroHEALTH repositories sit beside this checkout and are read-only prior art from here; a change one of them needs is made in its own checkout, branch and pull request, under its own rules
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The family's repositories ([[family-architecture]]) are checked out side by side under `~/RustroverProjects/`: FerroEHR, FerroBRIDGE, FerroFED, FerroPIX, FerroTERM, the shared-crate repositories FerroAUTH (local folder `FerroSMART`), FerroAUDIT, FerroSYS and FerroUI, FerroPULSE, FerroLICENCE and the FerroHEALTH umbrella.

**How to apply:**
- Read any sibling freely: code, rules, history, closed issues. It is the fastest source of prior art.
- A change a sibling needs is made in that sibling's checkout, on its own branch and pull request, under its own CLAUDE.md, rules and hooks, never mixed into a branch here. A gap in a shared crate is filed in the owning repository's current milestone and released there ([[shared-crate-blockers]]).
- A sibling's behaviour is evidence, never an oracle; the specification is the authority.
- A licence is decided per repository by the owner; read the sibling's `LICENSE` before quoting it.
