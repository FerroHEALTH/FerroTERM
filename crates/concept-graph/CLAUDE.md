<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# concept-graph

The materialized hierarchy of a loaded code system. Hand-written; no FHIR or
SNOMED spec governs the storage layout (our own design, `docs/architecture.md`
decisions 1 and 3), while each code system's own specification governs what
the algebra must answer.

- Modules: `ordinal` (dense `Ordinal` and `EdgeKind` keys), `csr` (compressed
  sparse row adjacency, sorted rows, transpose), `closure` (ancestor and
  descendant roaring bitmaps by a topological sweep; a cycle is refused),
  `subsumption` (the four FHIR `$subsumes` outcomes), `persist` (a versioned
  little-endian layout the store places in its artifact).
- Code-system-neutral: the graph knows ordinals and edge kinds, never SCTIDs,
  LOINC parts, or ICD chapters. A loader maps its release into ordinals; the
  ignored `local_edition` test shows the mapping for SNOMED CT.
- Never live traversal on a request path: `$subsumes` is a bitmap membership
  test, ECL is set algebra over the closure bitmaps. Roaring iterates in
  sorted order, which keeps every consumer deterministic.
- Load-bearing arithmetic is checked (`to_usize`, `try_from`, `saturating_*`);
  never a bare cast or index (`.claude/rules/reliability.md`).
- Property tests (`proptest`) hold the closure to brute-force reachability on
  random DAGs; coverage only ratchets up.
