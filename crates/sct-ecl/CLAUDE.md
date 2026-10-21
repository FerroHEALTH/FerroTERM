<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# sct-ecl

The ECL lexer, parser, and evaluator. Hand-written; the ECL specification and
its ANTLR grammar are the authority
(<https://github.com/IHTSDO/snomed-expression-constraint-language>), version
pinned in `docs/VERSIONS.md`.

- The grammar in the parser mirrors the `.g4` rule for rule; a rule name in a
  comment cites the grammar section. The grammar and the example corpus are
  vendored under `vendor/` by `scripts/vendor/ecl-grammar.sh` (the tag in
  `docs/VERSIONS.md`); never hand-edit them. The lexer only folds the
  character-level rules into tokens; mandatory whitespace and adjacency are
  checked from the token spans.
- The grammar's alternative order is the tie-breaker wherever two rules admit
  one text; a test asserts each such case.
- A parse failure is the answer for a grammar probe and is the one sanctioned
  `Result` to `Option` conversion; a failure anywhere else is a typed error
  (`.claude/rules/reliability.md`).
- Every ECL operator gets its own spec-cited test case over a synthetic
  hierarchy; coverage only ratchets up (`.claude/rules/testing.md`).
