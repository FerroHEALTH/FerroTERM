---
name: perl-edit-pitfalls
description: "Editing Rust with perl in this repo: use heredoc-quoted literal replacements, never q{} or s{}{} with braces in the text; rustfmt reflows targets between edits; a stale rmeta after fast edits needs cargo clean -p"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Bash is the editing tool here, and three failures repeated on 2026-09-03:
a `q{...}` or `s{...}{...}` perl string that contains Rust braces breaks the
script ("unmatched right curly bracket"); a replacement written from memory
of the file fails once `cargo fmt` has reflowed the target (long attribute
lines, tuple rows, `match` arms), so read the current text right before
matching or match on a short anchor; and an edit landing in the same second
as the previous build leaves a stale `.rmeta`, so dependents report
"unresolved import" for an item that exists until `cargo clean -p <crate>`.

**Why:** Each cost a round trip or three in the ICD-10, RxNorm, and ICD-11
work; the owner is waiting on those turns.

**How to apply:** Put every literal in a perl `<<'END'` heredoc and match with
`\Q...\E` (die on a miss so nothing half-applies); after a perl edit that
changes a public API, run `cargo clean -p` for that crate before checking
dependents when the error names an item that is plainly there.
