---
name: repo-path-ferroterm
description: The local clone moved from RustroverProjects/notio to RustroverProjects/FerroTERM on 2026-09-03; stale test binaries embed the old CARGO_MANIFEST_DIR until touched
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The working clone lives at `/Users/rubentalstra/RustroverProjects/FerroTERM`
since 2026-09-03 (renamed from `notio` by the owner mid-session). The Claude
project directory followed (`-Users-rubentalstra-RustroverProjects-FerroTERM`)
and its `memory` symlink points at the repo's `.claude/memory`.

**Why:** a shell whose cwd was the old path dies with "working directory was
deleted", and compiled test binaries keep the old `env!("CARGO_MANIFEST_DIR")`,
so `sct-ecl`'s corpus test fails with "No such file or directory" until
the sources that use it are touched and rebuilt.

**How to apply:** use the new path in every command; after any rename, touch
the files that use `CARGO_MANIFEST_DIR` (`grep -rl CARGO_MANIFEST_DIR crates
app tools | xargs touch`) before trusting a test run. See
[[official-name-ferroterm]].
