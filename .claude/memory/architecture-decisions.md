---
name: architecture-decisions
description: FerroTERM's locked, evidence-backed architecture (the OWL-EL two-problem split, the index-materialized graph store, the code-system-neutral provider seam with SNOMED CT first, the generated FHIR model consumed as fhir-types from FerroBRIDGE, and the pure-Rust stack)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroTERM is a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other
clinical code systems, SNOMED CT first (owner decision 2026-09-02: build the
engine around a code system provider seam from the first line of engine code;
the substrates never know one system's identifiers; `docs/architecture.md` §5,
`docs/terminologies.md`). The full design +
citations live in `docs/architecture.md`; these are the load-bearing decisions,
each owner-confirmed and adjudicated from cited research (two deep-research
streams, 2026-09-02).

- **SNOMED is an OWL 2 EL ontology, not merely a graph.** The system splits into
  OFFLINE classification (the inferred is-a hierarchy is a reasoner result: the
  International edition ships no transitive-closure file, so the default computes
  the closure from the shipped inferred Relationship file, and uses a distributed
  transitive-closure file as an opportunistic fast path where an edition includes
  one; an ELK-style reasoner is a later capability) and ONLINE serving from
  precomputed structures.
- **Graph MODEL, index-materialized STORE: never a graph database, never live
  traversal on the hot path.** Integer-keyed CSR adjacency (is-a + per-attribute)
  plus `roaring` transitive-closure bitmaps. Subsumption = O(1) bitmap test; ECL
  = bitmap set algebra; text search = a separate `fst` index. Evidence: every
  production server (Snowstorm, Ontoserver, Hermes) uses a materialized index;
  graph DBs lose on ECL's deep-reachability queries. This honors the owner's
  (correct) graph intuition while implementing it the fast way.
- **Persistence = `redb`** (pure-Rust, disk-backed, ACID; NOT memory-mapped,
  it dropped its mmap backend in 0.14.0 and the workspace forbids `unsafe`):
  the persistence FORMAT, not the query-time path. The hot closure is read
  RESIDENT at startup (ordinal-indexed `Vec<RoaringBitmap>`), never
  redb-get-and-deserialize per `$subsumes` (that would forfeit the µs goal). The
  dense concept/display columns are read in at open; the rest of the store is
  point-read through redb's page cache. Closure ~100 to 300 MB resident.
- **Offline classification default = COMPUTE the transitive closure from the
  inferred Relationship file** (SNOMED ships NO transitive-closure file for the
  International edition, only a script); consume a shipped TC file only where an
  edition includes one. A reasoner (ELK-style) is a later capability.
- **FHIR is machine-generated, and not here.** The model is the
  `fhir-types` crate, emitted from the official FHIR packages with
  per-version modules for **R4 + R4B + R5 + R6 (ballot)**. Its generator was
  first built here (R4B first, a terminology root-set closure); on
  2026-09-12 (#300) the crate and its generator moved to FerroBRIDGE, which
  owns and publishes it ([[family-architecture]]). FerroTERM consumes it
  from crates.io: never hand-write FHIR types, never edit `// @generated`,
  and request a missing shape on the FerroBRIDGE tracker. ECL parser =
  `winnow` (not the pre-1.0 chumsky).
- **Pure Rust, single binary, no JVM / no Elasticsearch.** SNOMED CT content is
  licence-gated and NEVER committed (bring-your-own RF2); fixtures are
  shaped/synthetic.

Crate plan (see `docs/architecture.md`): `rf2`, `concept-graph`,
`concept-store`, `designation-index`, `sct-ecl`, `fhir-terminology`,
`app/ferroterm-server`, `tools/ferroterm-build`, with the generated
`fhir-types` consumed from crates.io. On 2026-09-12 (#300) that crate and its
generator moved to the FerroBRIDGE repository, which vendors the HL7 packages
and publishes the crate; nothing is generated here any more.

Build sequence as first planned: (1) FHIR codegen → `fhir-types`, R4B
first then R5/R4/R6 (FerroBRIDGE's since #300); (2) RF2 + offline build →
redb artifacts; (3) read-only serving core ($lookup/$subsumes/$validate-code);
(4) ECL + $expand + $translate; (5) hardening + packaging. Reference oracles:
Snowstorm + Hermes; the ECL ANTLR grammar. See [[owner-work-style]].
