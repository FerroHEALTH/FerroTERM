<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Architecture

FerroTERM is a pure-Rust FHIR terminology server for SNOMED CT, LOINC,
and other clinical code systems, SNOMED CT first. It serves the HL7 FHIR
terminology API across R4, R4B, R5, and R6 from one running server, backed by a
precomputed concept index per loaded code system, built offline and read at
startup, with no JVM, no Elasticsearch, and no graph database.

This document is the design authority. It is grounded in the terminology-server
literature and the graph-reachability literature, cited inline, rather than in
convention.

## First principles: what SNOMED CT actually is

SNOMED CT is the first code system FerroTERM serves and the one that shapes the
engine, because it is the hardest case: a polyhierarchy, a formal ontology, and
the only system with its own query language (ECL). A design that serves SNOMED
well serves the simpler systems (LOINC, ICD-10, UCUM) with capabilities turned
off, and decision 5 below records how they plug in.

SNOMED CT is two things at once, and conflating them is the mistake most
architectures make.

1. **It is a formal ontology in the OWL 2 EL profile.** Concept definitions are
   description-logic axioms; the is-a hierarchy a client queries is not the
   *stated* hierarchy but the *inferred* one, produced by a description-logic
   reasoner (ELK, Snorocket, FaCT++) through consequence-based saturation
   (Kazakov, Krötzsch, Simančík, "The Incredible ELK", J. Automated Reasoning
   53(1), 2014, <https://link.springer.com/article/10.1007/s10817-013-9296-3>;
   SNOMED OWL Reference Set,
   <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-owl-reference-set-specification>).
   ELK classifies all of SNOMED in about five seconds. Its output is the
   inferred DIRECT is-a relation (a few million edges); the TRANSITIVE closure
   over it (every ancestor/descendant pair, which is what serving needs) is
   several-fold larger, on the order of tens of millions of pairs (see the
   footprint note in decision 1). Still an extremely sparse fraction of all
   possible concept pairs.
2. **It is a polyhierarchical graph.** A concept has many parents; edges are
   typed (is-a, finding site, associated morphology, and the rest) (SNOMED
   Concept Model,
   <https://docs.snomed.org/snomed-ct-practical-guides/snomed-ct-starter-guide/6-snomed-ct-concept-model>).

The graph model is correct and universal. The consequence that most designs miss
is that the system therefore has **two separate problems with different
engineering answers**: an offline inference problem, and an online serving
problem. Everything below follows from keeping them apart.

## The two problems

### Offline: classification, once per release

The hierarchy FerroTERM serves is a reasoner output, computed once per SNOMED
release, never at query time. The default:

- **Compute the transitive closure from the shipped inferred Relationship file**
  (`typeId = 116680003` is-a), a topological-order bitset-propagation sweep
  over the ~1.5M is-a edges, a matter of seconds offline, then persisted.
  SNOMED does NOT distribute a transitive-closure file in the International
  Release. It ships only a script to generate one
  (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/component-release-file-specification/4.2-file-format-specifications/4.2.5-transitive-closure-files>),
  so computing it is the baseline, not consuming a file. Where a distributed
  edition does include a transitive-closure file, use it as an opportunistic
  fast path. SNOMED's own guidance calls a precomputed transitive closure "one
  of the most efficient ways to test for subsumption"
  (<https://docs.snomed.org/snomed-ct-practical-guides/snomed-ct-data-analytics-guide/6-snomed-ct-analytic-techniques/6.2-subsumption>).
- **Run a reasoner:** an ELK-style consequence-based classifier over the OWL
  axiom refset. Needed only when the input is stated axioms or post-coordinated
  expressions that must be classified locally. This is a later capability, not a
  requirement to serve a released edition (which already ships inferred
  relationships).

The build pipeline turns the release into the materialized serving structures
below. It runs offline (in a tool, `tools/ferroterm-build`), so the running server
never classifies and never re-derives inference.

### Online: serving, from precomputed structures

Every FHIR terminology operation is answered from a precomputed structure, and
each operation has its own optimal shape. This table is the load-bearing design
decision:

| Operation | True query shape | Structure that answers it |
|---|---|---|
| `$subsumes` | deep ancestor test | roaring-bitmap membership, O(1) |
| `$expand` ECL `<<`/`<`/`>>`/`>` | reachability set | the precomputed descendant/ancestor bitmap, returned directly |
| `$expand` ECL refinement (`: attr = <<X`) | set intersection/union across typed edges | bitmap set-algebra over the per-attribute adjacency |
| `$validate-code` | point read + MRCM check | columnar concept/description store |
| `$lookup` (properties, designations) | point read + 1-hop neighborhood | columnar store + adjacency |
| `$lookup`/search by term, autocomplete | text match | a separate `fst` inverted/prefix index |
| `$translate` | map-refset lookup | key-value lookup |

The point the pure-graph view misses: "descend the hierarchy level by level" is
paid **once, offline**, and at runtime the descent collapses into a
set-membership test or a bitmap intersection. Graph traversal is the slow way to
answer what a terminology server is actually asked.

## The defining decisions

### 1. Graph model, index-materialized storage: not a graph database, not live traversal

SNOMED is stored as a graph, natively: integer-keyed (SCTID) **compressed
sparse-row (CSR) adjacency arrays** (is-a in both directions, plus one CSR per
attribute type for ECL refinement) and **roaring bitmaps** holding the
transitive closure (each concept's ancestor and descendant sets). CSR arrays are
graph-native and cache-friendly; they are not a pointer graph and not a database.

A general-purpose graph database is rejected on the evidence, not on taste:

- No production terminology server uses one. Ontoserver is Postgres + a Lucene
  index (Metke-Jimenez et al., "Ontoserver: a syndicated terminology server", J.
  Biomedical Semantics 9:24, 2018,
  <https://jbiomedsem.biomedcentral.com/articles/10.1186/s13326-018-0191-z>);
  Snowstorm is Elasticsearch (<https://github.com/IHTSDO/snowstorm>); Hermes is a
  memory-mapped store plus Lucene (<https://github.com/wardle/hermes>). All three
  chose materialized index structures.
- The graph-database speed wins in the literature benchmark graph traversal
  against naive relational JOINs, not against a precomputed index (Jeon 2025,
  <https://www.medrxiv.org/content/10.1101/2025.07.20.25322556v1>; Campbell et
  al. 2015, <https://www.sciencedirect.com/science/article/pii/S1532046415001847>),
  and a triple store lost to a relational store on query time even where it won
  on load time (Can et al., Entropy 19(1):30, 2017,
  <https://www.mdpi.com/1099-4300/19/1/30>).
- Graph databases degrade exactly where a terminology server is stressed:
  out-of-memory on large multi-hop neighborhoods, slow on high-degree subgraphs
  (<https://pmc.ncbi.nlm.nih.gov/articles/PMC7233100/>), which is ECL
  descendant expansion of a high-level concept.

At SNOMED's size (~360k concepts, ~1.5M edges, static between releases) the full
transitive closure is the degenerate-optimal case of the whole reachability-index
family (2-hop, interval, GRAIL): when the closure fits in memory, it *is* the
best reachability index, O(1) test, no traversal fallback (Cohen et al., 2-hop,
SODA 2002, <https://dl.acm.org/doi/10.5555/545381.545503>; Agrawal et al.,
interval labeling, SIGMOD 1989, <https://dl.acm.org/doi/10.1145/67544.66950>).
Hermes demonstrates the payoff: ~13 to 69 µs subsumption and ~0.82 µs concept lookup
on a laptop, from a materialized structure rather than a graph walk.

Live adjacency traversal is kept for the queries it is best at, the
immediate parents/children and neighbourhood browsing of `$lookup`, and nothing
else routes through it.

### 2. FHIR support is machine-generated across every version

HL7 publishes the whole type system and every operation as machine-readable
`StructureDefinition` and `OperationDefinition` resources, in versioned packages
(`hl7.fhir.r4.core` 4.0.1, `hl7.fhir.r4b.core` 4.3.0, `hl7.fhir.r5.core` 5.0.0,
`hl7.fhir.r6.core` 6.0.0 ballot), plus `hl7.terminology`
(<https://www.hl7.org/fhir/packages.html>). Per-version Rust modules are
generated from those pinned packages, so R5's extra `$expand` parameters
(`useSupplement`, `property`, `displayLanguage`) appear where the spec has them
and are absent where it does not
(<http://hl7.org/fhir/R5/valueset-operation-expand.html>). This mirrors how the
sibling project FerroEHR generates its openEHR model. The generator and the
vendored packages live in the FerroBRIDGE repository, which publishes the model
as the `fhir-types` crate; FerroTERM consumes it from crates.io at the version
`docs/VERSIONS.md` pins (#300). Versions are selected by a runtime wrapper,
so one server answers R4/R4B/R5/R6 callers at once. R6 is a ballot-tracking
generation (publication expected around late 2026).

**The emission scope is a declared root-set closure, not the whole FHIR model.**
A terminology server touches a handful of resources; generating all ~150
resources of each core package across four versions would be dead weight
(compile time, binary size) with no consumer. The generator's root set is the
terminology surface (`CodeSystem`, `ValueSet`, `ConceptMap`, `Parameters`,
`OperationOutcome`, `CapabilityStatement`, `TerminologyCapabilities`, `Bundle`,
plus the terminology `OperationDefinition`s), and it emits the COMPLETE
transitive closure of datatypes and primitives those roots reference. That is
"complete within a declared closure", the sanctioned form of the
emit-the-whole-model rule (`codegen.md`): never trimming inside the closure to
quiet a diff, and never a hand-written shape outside it.

**R4B is the first generation implemented.** It is the current stable release of
the R4 line and a near-superset of R4, so an R4B-first build already serves the
R4-family terminology surface; R5, R4, and R6 follow. (R4 4.0.1 remains the most
widely deployed base and is a generation in its own right: R4B first is a
sequencing choice, not a drop of R4.)

### 3. Persistence is a pure-Rust embedded engine; the hot closure is resident

The concept and description store is persisted in **`redb`**, a pure-Rust,
disk-backed, ACID embedded key-value engine. This is deliberate: it is a real
storage engine on disk, so the store is neither a hand-rolled file format nor a
service, while staying pure Rust inside the same binary. The CSR adjacency and
the roaring closure bitmaps sit beside it in their own artifact files, whose
layout this project owns (no FHIR/SNOMED spec governs it: our own design). The
build tool writes all of them once per edition; the server opens them read-only.

`redb` reads its file with ordinary file reads and caches the pages it reads.
It has no memory-mapped backend, and has had none since 0.14.0 removed
`create_mmapped()`/`open_mmapped()` "because it was infeasible to prove that it
was sound", which "makes the redb API entirely safe"
(<https://github.com/cberner/redb/blob/master/CHANGELOG.md>). That suits this
workspace: mapping a file takes `unsafe`, and `unsafe_code = "forbid"` is set
workspace-wide, so nothing here can map one.

**`redb` is the persistence FORMAT, not the query-time path for the hot
structures.** `redb.get()` yields the value *bytes*, and a `roaring` bitmap has
no query-over-serialized-bytes API: deserializing an ancestor bitmap on every
`$subsumes` would be O(bitmap size) per call and would forfeit the
microsecond-subsumption goal. So at startup the server reads the closure into a
**resident, ordinal-indexed structure** (an SCTID→ordinal map plus
`Vec<RoaringBitmap>` for the ancestor and descendant closures), and subsumption
is then a membership test against the resident bitmap. The dense columns,
addressed by ordinal, are read in at open for the same reason: the concepts,
the displays, the properties, and the acceptabilities. Each is its own file
beside the database (`store.concepts.col` and its three siblings), read once
into the structure it is served from; held in `redb` a column arrived twice,
as the value the database materialized and as the copy the store served from,
and the first cost a Dutch edition 168 MB of resident memory for the life of
the process. The designation text stays in the `redb` file, one row per
concept, and is point-read through the engine's page cache on `$lookup`; a
column of it measured 239 MB resident per SNOMED edition against a read that
already costs three microseconds. This is how Hermes reaches
tens-of-microseconds subsumption, realized in pure Rust.

Footprint: the transitive closure is tens of millions of ancestor/descendant
pairs (both directions are stored: subsumption needs one, ECL returns each set
directly, a deliberate ~2× space cost). Roaring compresses SNOMED-shaped sets
heavily (dense high-level sets to bitmap containers, sparse leaves to array
containers), so the resident closure lands at roughly 100 to 300 MB, plus tens of
MB for CSR adjacency, the `fst` word dictionary, and the dense concept columns,
near the ~500 MB the reference Snowstorm Lite needs, and far under a 2 to 4 GB box.

Serving concurrency: point reads against the store's page cache and
resident-bitmap subsumption run inline in the async handler. A heavy `$expand`
that materializes a 100k-member set, and any cold read that reaches the file,
run on a blocking pool (`tokio::task::spawn_blocking`) so they never stall the
runtime. The blocking seam sits at the `fhir-terminology` engine boundary and is
designed in from the start, not retrofitted.

### 4. The SNOMED semantics are hand-written and owned

RF2 loading, the materialized ontology, ECL evaluation, and `$expand` paging are
the product. Two Rust projects cover parts of the ground (`snomed-rust` parses
RF2 and evaluates simple ECL; `fhir-sdk` generates FHIR models), and neither
is a terminology server; FerroTERM designs its own engine and reads them as prior
art. ECL is the hard part and
the main risk: every expression constraint returns a *set of codes* (SNOMED ECL
specification,
<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>),
so the evaluator compiles ECL to **set algebra** over the descendant bitmaps and
the attribute adjacency, bitmap AND/OR, not pointer-chasing. It is built and
tested as its own layer against the published ECL ANTLR grammar
(<https://github.com/IHTSDO/snomed-expression-constraint-language>) before the
value-set surface depends on it. Correctness is measured against Snowstorm as the
reference server.

A declarative alternative was considered: expressing subsumption and ECL as
Datalog over an embedded engine (`cozo`, pure-Rust, recursive queries native).
Datalog is a sound way to compute and express the closure. For a latency-critical
path the materialized bitmap closure is the hot primitive, so a Datalog layer
stays a possible later addition for complex ad-hoc ECL rather than a foundation
piece.

### 5. The engine is code-system-neutral: one provider seam, SNOMED CT first

The FHIR terminology operations never ask "is this SNOMED". They talk to a
**code system provider** seam, and each code system reaches the operations
through its own provider: a loader that turns the system's release into the
shared substrates, plus the system's semantics (its filters, properties,
implicit value sets, and version rules). No FHIR or SNOMED specification
governs the seam itself (the Terminology Ecosystem IG defines behaviour, not an
interface), so this is our own design; the FHIR `CodeSystem` metadata
(`content`, `hierarchyMeaning`, `caseSensitive`, `compositional`,
`versionNeeded`, `property`, `filter`) is the capability declaration each
provider returns (<https://hl7.org/fhir/R4B/codesystem.html>,
<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).

The substrates are neutral by construction. `concept-store` holds one code system
version's concepts, displays, designations by language, and typed property
values, keyed by dense ordinal, with the system's native code as the string
key. `concept-graph` holds typed edges and closure bitmaps over ordinals; a system
without a hierarchy has no closure. `designation-index` indexes designation words. None
of them knows an SCTID, a LOINC part, or an ICD chapter. A loader (`rf2`
for SNOMED CT, then one crate per system) maps its release into ordinals, edge
types, and a property vocabulary. `tools/ferroterm-build` runs whichever loader the
release needs.

What every provider supplies, and what is optional, follows the seam that the
multi-system servers converge on. `tx.fhir.org`'s `TCodeSystemProvider`
(identity and version rules, `locate`, `Display`, `Designations`,
`getProperties`, `filter`, `searchFilter`, `getIterator`, `subsumesTest`,
`cloneWithSupplements`;
<https://github.com/HealthIntersections/fhirserver/blob/master/library/ftx/ftx_service.pas>)
and hades' `CodeSystem` protocol (`cs-metadata`, `cs-lookup`,
`cs-validate-code`, `cs-subsumes`, `cs-expand*`; <https://github.com/wardle/hades>)
both hide storage behind an opaque concept handle and keep compose, exclude,
dedup, and paging in one shared layer above the providers. HAPI FHIR and
Snowstorm instead push every system into one concept table with a precomputed
ancestor list and grow per-system special cases in the filter code (HAPI's
`handleFilterLoinc*` family, Snowstorm's `isSnomed()` fork). FerroTERM takes the
first shape.

| Capability | Every provider | Declared per system |
|---|---|---|
| Identity: system URI, versions, the default-version rule, `versionNeeded` | yes | SNOMED edition and version URIs; LOINC `2.78`; UCUM fixed |
| Metadata for `TerminologyCapabilities`: content mode, case sensitivity, hierarchy meaning, compositional, property and filter definitions | yes | |
| Locate a code; display, definition, active or inactive with reason, abstract, designations by language | yes | |
| Typed properties, including `parent`, `child`, `status`, `inactive`, `notSelectable`, `itemWeight` | yes | SNOMED attributes and module; LOINC's axes |
| Supplements applied to designations | yes | |
| Generic filters on `concept` and `code` (`=`, `in`, `not-in`, `regex`), membership tests, a closed-or-open answer | yes | |
| Text search over designations | yes | |
| Enumeration with a total count, for expansion | yes | grammar-defined systems validate by parsing; UCUM refuses enumeration and BCP 47 enumerates only a selection its registry bounds |
| Hierarchy and subsumption (`is-a`, `descendent-of`, `generalizes`, `child exists`) | | SNOMED, ICD-10, FHIR CodeSystems with `hierarchyMeaning = is-a`; LOINC and UCUM have none |
| System-specific filters and a filter language | | SNOMED `constraint` (ECL) and refset membership; LOINC's named axes, `parent`, `ancestor`, `copyright`; UCUM `canonical` |
| Implicit value sets and concept maps parsed from the system URI | | SNOMED `?fhir_vs=…` and `?fhir_cm=…`; LOINC `/vs/…`; UCUM `/vs/…` |
| Alternate and normalized codes | | UCUM canonicalization; FHIR CodeSystems with alternate codes |
| Concept maps carried by the release | | SNOMED map and association refsets |

The compose layer (include, exclude, dedup, `offset`, `count`, `expansion.total`)
lives once, in `fhir-terminology`, above every provider. ECL is the one
SNOMED-only filter language and stays in `sct-ecl`, reached through the
SNOMED provider's `constraint` filter. The build order and the per-system facts
(URI, release format, licence, hierarchy, FHIR-defined filters) are in
`docs/terminologies.md`.

## Text search

Description search is a separate concern from the graph, kept in its own index.
SNOMED search is per-word prefix matching in any order, filtered by language
reference set and active status, ranked by matched-term length, not relevance
scoring (Snowstorm search docs,
<https://github.com/IHTSDO/snowstorm/blob/master/docs/search.md>). The structure
is a **word inverted index**: each word token maps to a roaring-bitmap postings
list of the descriptions containing it. A finite-state transducer (`fst`) holds
the word dictionary and answers per-word prefix (and Levenshtein-fuzzy) lookup,
expanding a typed prefix to its matching word ids; the query intersects those
words' postings, ANDs the language-refset and active-status filter bitmaps, and
sorts the surviving descriptions by matched-term length. The `fst` alone is not
the index. It is the prefix front end over the inverted postings. Pure Rust,
built once per edition and read into memory when the server opens the artifact.

## Workspace layout

A single Cargo workspace. Every crate here is hand-written, over the generated
`fhir-types` crate from crates.io; `ferroterm-build` is tooling. The substrate
crates
(`concept-store`, `concept-graph`, `designation-index`) are code-system-neutral; each
code system adds a loader crate (`rf2` is the first) that feeds them.

| Crate | Role | Kind |
|---|---|---|
| `crates/rf2` | SNOMED CT RF2 loader (inferred relationships, descriptions, refsets, transitive-closure file) + typed component model; the first code system loader | hand-written |
| `crates/concept-graph` | The materialized hierarchy of a loaded code system: CSR adjacency (is-a + per-relationship-type) and roaring transitive-closure bitmaps; subsumption + ECL set algebra | hand-written |
| `crates/concept-store` | The `redb`-backed columnar concept and designation store, one per code system version: point reads for `$lookup`/`$validate-code` | hand-written |
| `crates/designation-index` | The `fst` + roaring designation search index (prefix, language and use filter, term-length sort) | hand-written |
| `crates/sct-ecl` | Expression Constraint Language lexer, parser, and evaluator (compiles ECL to set algebra over `concept-graph`) | hand-written |
| `crates/sct-scg` | Compositional Grammar lexer, parser, and printer, the syntax of a post-coordinated expression; syntax only | hand-written |
| `crates/fhir-terminology` | The engine: the FHIR terminology operations over the code system provider seam, dispatched per version | hand-written |
| `crates/loinc` | LOINC release loader and typed row model | hand-written |
| `crates/classification` | ClaML and ICD-10-CM tabular readers into one classification model (ICD-10 family, ICPC-2) | hand-written |
| `crates/icd11` | ICD-11 entities from the WHO ICD-API local deployment, cached and read; the API walker sits behind the `api` feature the build tool enables | hand-written |
| `crates/rxnorm-rrf` | RxNorm Rich Release Format reader and typed row model | hand-written |
| `crates/dhd-thesaurus` | DHD Diagnosethesaurus and Verrichtingenthesaurus delivery reader | hand-written |
| `crates/gstandaard` | G-Standaard (Z-Index) product ladder reader: GPK, PRK, HPK, and article files | hand-written |
| `crates/labcodeset` | Nederlandse Labcodeset publication (labconcepts XML) reader | hand-written |
| `crates/terminology-syndication` | The Atom terminology syndication client: the feed model and its NCTS extensions, entry selection, checksum-verified download, and the `Source` seam each service add-on implements | hand-written |
| `app/ferroterm-server` | The `axum` HTTP server: FHIR endpoints, content negotiation, runtime version routing | hand-written |
| `app/ferroterm-sync` | The synchronisation service: the schedule, the two lanes, activation, retention, and the run records; the one member that links an add-on | hand-written |
| `addons/nts` | The Nationale Terminologieserver (NTS) source add-on for the sync service: the Nictiz feed, its Keycloak grants, its subscription, and the NL fix-ups; depends on `terminology-syndication` only | hand-written |
| `app/ferroterm-viewer` | The FerroTERM Console: a Leptos app rendered on the server and hydrated, its own binary and image, a backend-for-frontend that reaches the server's public FHIR API and links no workspace crate | hand-written |
| `tools/ferroterm-build` | The offline build: a code system release (RF2 first) → the graph/store/text artifacts the server reads, once per release | tooling |
| `tools/ferroterm-testkit` | Synthetic fixtures for the test suites, a shaped edition written the way the build writes it; a dev-dependency only, never shipped | tooling |
| `tools/ferroterm-bench` | The benchmark harness: ingest, footprint, and latency of every served code system into reproducible records | tooling |

Dependencies point one way (app/tools/addons → crates); nothing depends upward
into the server.

### The service and add-on layer

No spec governs this, our own design. Taking content from a national
terminology service runs beside the server, never inside it: `app/ferroterm-sync`
fetches on a schedule, builds and stages what it took, and asks the server to
reload, while the server keeps the read path. Every service it can read from is
an add-on under `addons/*` implementing the `Source` seam
`crates/terminology-syndication` declares, so a new service is a new crate and
one registration line.

The boundary that keeps that layer optional has two directions. An add-on
depends on `crates/terminology-syndication` and the leaf crates the guard names
explicitly, never on the engine, the server, the viewer, or another add-on; and
only `app/ferroterm-sync` links an add-on, so the workspace still builds and
serves with every add-on removed. `scripts/checks/addon-boundary.sh` reads the
declared workspace edges from `cargo metadata` and fails on either direction,
the way `scripts/checks/viewer-boundary.sh` holds the viewer to its FHIR-client
boundary.

## Verification

- **Snowstorm and Hermes as reference** for terminology answers: `$lookup`,
  `$subsumes`, `$validate-code`, `$expand` results are checked against the
  reference servers over the same edition.
- **ECL against its published grammar**: the evaluator is tested as its own layer
  before the value-set surface leans on it.
- **Classification parity**: when a reasoner is added, its inferred hierarchy is
  checked against SNOMED's shipped inferred/transitive-closure files.
- **Generated-layer drift check**: the FHIR generator is re-run in CI and fails
  on any diff.

## Licensing

The software is under the Business Source License 1.1 (free for non-production
and non-commercial production use; commercial production use needs a licence;
Apache 2.0 four years after each version). Code system content is licensed by
its owner and is never
distributed here: SNOMED CT by SNOMED International (free within member
countries, affiliate licence elsewhere), LOINC by the Regenstrief Institute,
and so on per `docs/terminologies.md`. A deployment brings its own releases
under valid licences. The vendored FHIR packages are HL7 material under their own terms,
vendored verbatim with provenance as codegen input.

## Prior art

- **Snowstorm** (SNOMED International, Java + Elasticsearch): the reference
  server and correctness oracle for SNOMED CT.
- **tx.fhir.org** (Health Intersections, the `fhirserver` repository): the
  reference for the HL7 tx-ecosystem test cases, and the clearest existing
  code system provider seam (`TCodeSystemProvider`).
- **hades** (Mark Wardle, Clojure): a composite FHIR terminology server over
  SNOMED CT, LOINC, and FHIR packages, dispatching by canonical URL; the
  closest analogue to the provider design.
- **Ontoserver** (CSIRO, Postgres + Lucene): a production FHIR terminology
  server; index-materialized, not a graph database.
- **Hermes** (Mark Wardle, Clojure, memory-mapped store + Lucene): the
  memory-mapped, materialized-structure design, at microsecond latencies.
- **ELK / Snorocket:** the OWL 2 EL reasoners that classify SNOMED offline.
- **Helios `hfs`** (MIT, Rust): machine-generates per-version FHIR modules; a
  client, not a server; prior art for the generator.
- **`fhir-sdk`** (Rust, <https://github.com/FlixCoder/fhir-sdk>): generated
  per-version FHIR models and a client (`fhir-model` on crates.io); prior art
  for the generated type shapes and the JSON codec.
- **`snomed-rust`** (Rust, <https://github.com/snomed-rust/snomed-rust>): the
  `snomed-rf2` and `snomed-ecl` crates, an RF2 parser and an evaluator for
  simple expression constraints over in-memory hash maps; prior art for the
  RF2 loader, read, not ported.
- **Helios `hfs` and `hts`** (Rust, <https://github.com/HeliosSoftware/hfs>): a
  FHIR server and terminology service over SQL with generated R4, R4B, R5, and
  R6 models, JSON and XML; the closest Rust competitor and a behavioural oracle
  beside Snowstorm for spec-silent edge cases. Not a dependency: `unsafe`
  without `forbid`, build-time downloads, and SQL recursive-query subsumption.
- **`rh-codegen`** (Rust, <https://github.com/reason-healthcare/rh>): FHIR
  models generated from the same `hl7.fhir.*.core` packages `fhir-types` is
  emitted from; a second emitter to compare decisions against.
- **`octofhir-ucum`** (Rust, Apache-2.0): a UCUM parser tested against the
  official suite; the one crate shortlisted as a dependency, for the UCUM
  provider.
- **`sct-rs`** (Rust, AGPL-3.0): RF2 to SQLite and Parquet with an ECL and FHIR
  R4 server mode; copyleft terms the project does not take on, not read for
  code.

No existing Rust project is a complete, standards-generated, multi-version FHIR
terminology server. FerroTERM fills that gap, built the way the evidence supports: a
graph model, an offline classification pass, and an index-materialized store
read at startup, not a graph database and not live traversal.
