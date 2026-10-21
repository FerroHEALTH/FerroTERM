<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# sct-scg

The SNOMED CT Compositional Grammar lexer, parser, and printer. Hand-written;
the normative ABNF of the Compositional Grammar specification is the authority
(<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-compositional-grammar-specification/design/5-syntax-specification>),
commit pinned in `docs/VERSIONS.md`.

- The grammar in the parser mirrors the ABNF rule for rule; a rule name in a
  comment cites the rule. The ABNF and the example corpus are vendored under
  `vendor/` by `scripts/vendor/scg-grammar.sh` (the commit in
  `docs/VERSIONS.md`); never hand-edit them. The lexer only folds the
  character-level rules into tokens; the ABNF's adjacency (`sctId`,
  `numericValue`, `"#" numericValue`) is checked from the token spans, and a
  rule whose content the lexer cannot bound (`term`, `stringValue`) is checked
  by the parser against the ABNF's character classes.
- The crate is syntax only. No concept identifier is resolved, no concept
  model is checked, and no subsumption is decided here; that is the SNOMED
  provider's work in `crates/fhir-terminology`.
- A parse failure is a typed error with a byte offset, never a panic
  (`.claude/rules/reliability.md`).
- Every example of the vendored corpus parses and round-trips through the
  printer; coverage only ratchets up (`.claude/rules/testing.md`).
