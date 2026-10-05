# CLAUDE.md

**FerroTERM** is a pure-Rust FHIR terminology server for SNOMED CT,
LOINC, and other clinical code systems, SNOMED CT first. It serves the HL7 FHIR
terminology API across R4, R4B, R5, and R6 from one running server, backed by a
precomputed concept index read at startup, with no JVM and no Elasticsearch. The engine is
code-system-neutral: the operations talk to a code system provider seam, and
each system arrives through its own loader (`docs/architecture.md` §5,
`docs/terminologies.md`). The name is official: Ferro for the Rust family shared
with FerroEHR, TERM for terminology; the site is <https://ferroterm.eu>.

The full design, with citations, is in **`docs/architecture.md`**. Read it
first. This file is the working discipline. Write all prose (docs, comments,
commits, PRs, issues) to `.claude/rules/writing-style.md`.

## The two layers (the FerroEHR split, applied to FHIR + SNOMED)

- **`fhir-types` is GENERATED, and generated elsewhere.** The crate carries the
  per-version FHIR model (R4/R4B/R5/R6), emitted from the machine-readable HL7
  packages, and the FerroBRIDGE repository owns both the generator and those
  packages and publishes the crate to crates.io. FerroTERM depends on it from
  the registry like any other crate, so the operation surface stays correct per
  version by construction and nothing here is generated.
- **The engine is HAND-WRITTEN** and is the product: the code-system-neutral
  disk-backed concept store, hierarchy graph, and `fst` and `roaring`
  designation index, the per-system loaders (SNOMED RF2 first), ECL
  evaluation, and the FHIR terminology operations over the provider seam.
  Idiomatic Rust of our own design, with the FHIR and SNOMED specifications
  as the authority.

## Repo map (a single Cargo workspace)

`crates/*` = libraries, `addons/*` = source add-ons for the sync service,
`app/*` = the server binary, `tools/*` = dev/codegen tooling not shipped in the
server. Every crate carries its own `CLAUDE.md` with crate-local discipline.

- `crates/rf2`: SNOMED CT RF2 loader (inferred relationships, descriptions,
  refsets, transitive-closure file) and typed component model.
- `crates/concept-graph`: the materialized hierarchy of a loaded code system.
  Integer-keyed CSR adjacency (is-a and per-relationship-type) and roaring
  transitive-closure bitmaps; subsumption and ECL set algebra.
- `crates/concept-store`: the `redb`-backed columnar concept and designation
  store, one per code system version. Point reads for
  `$lookup`/`$validate-code`.
- `crates/designation-index`: the `fst` and `roaring` designation search index
  (per-word prefix, language and use filters, matched-term-length sort).
- `crates/sct-ecl`: the Expression Constraint Language lexer, parser, and
  evaluator (compiles ECL to set algebra over `concept-graph`).
- `crates/sct-scg`: the Compositional Grammar lexer, parser, and printer, the
  syntax a post-coordinated expression is written in. Syntax only; resolving a
  concept and deciding subsumption belong to the SNOMED provider.
- `crates/fhir-terminology`: the engine. `$lookup`/`$validate-code`/`$expand`/
  `$subsumes`/`$translate` over the code system provider seam, dispatched per
  FHIR version.
- `crates/loinc`, `crates/classification`, `crates/icd11`, `crates/rxnorm-rrf`,
  `crates/dhd-thesaurus`, `crates/gstandaard`, `crates/labcodeset`: the other
  code system loaders, one crate per source format (the LOINC release, ClaML
  and the ICD-10-CM tabular files, the WHO ICD-API cache, RxNorm RRF, the DHD
  thesauri delivery, the G-Standaard product ladder, the Labcodeset
  publication). Each feeds the same neutral substrates as `rf2`.
- `crates/terminology-syndication`: the Atom terminology syndication client. The feed model
  with its NCTS extensions, entry selection, checksum-verified download, and
  the `Source` seam each service add-on implements. Country-neutral and
  code-system-neutral.
- `addons/*`: the source add-ons the sync service compiles in, one per national
  terminology service (`addons/nts`, the Nictiz Nationale Terminologieserver,
  is the first). Each depends on `crates/terminology-syndication` and the
  leaf crates the guard names, never on the server, the viewer, or another
  add-on; only `app/ferroterm-sync` links an add-on, and nothing else in the
  workspace may. `scripts/checks/addon-boundary.sh` enforces both directions.
  None is published.
- `app/ferroterm-server`: the `axum` HTTP server (FHIR endpoints, content
  negotiation, runtime version routing).
- `app/ferroterm-sync`: the synchronisation service, one way from a terminology
  syndication feed into a running server. The schedule, the RF2 and FHIR lanes,
  activation, retention, the run records, and its own admin listener. The one
  member that may link an `addons/*` crate. Designed in
  `website/book/src/operate/sync.md`, tracked under issue #579.
- `app/ferroterm-viewer`: the Leptos web UI, client-side rendered and served by
  the one server binary as static assets. A pure FHIR client over HTTP that
  depends on no other crate in the workspace, which
  `scripts/checks/viewer-boundary.sh` enforces. Built by Trunk
  (`cd app/ferroterm-viewer && trunk build --release --locked`), never by
  `cargo` alone, and its `dist/` is never committed. Designed in
  `docs/viewer.md`, tracked under issue #366.
- `tools/ferroterm-build`: the offline build, from an RF2 release to the
  graph/store/text artifacts the server reads, once per edition.
- `tools/ferroterm-bench`: the benchmark harness that measures ingest,
  footprint, and latency of every served code system into the records under
  `bench/records/`; never shipped.
- `tools/ferroterm-testkit`: synthetic fixtures for the test suites (a
  shaped SNOMED edition written the way the build writes it). A
  dev-dependency of any crate's tests, never a runtime dependency, never
  published: the one sanctioned edge from a crate's tests into `tools/`.

Dependencies point one way (app/tools depend on crates); nothing depends upward
into the server. A code system is a graph MODEL served from an
index-materialized store (CSR adjacency and roaring closure), never a graph
database and never live traversal on the hot path. Nothing in the substrates or
the operations is SNOMED-specific; SNOMED semantics live in its loader, ECL,
and its provider. See `docs/architecture.md` for the evidence.

## The FHIR model comes from crates.io

The FHIR layer is generated, and the generator is not here. HL7 publishes the
whole type system and every operation as machine-readable resources in
versioned packages; the FerroBRIDGE repository
(<https://github.com/rubentalstra/FerroBRIDGE>) vendors and pins those
packages, generates the `fhir-types` crate from them, and publishes it. This
repository takes the crate from crates.io, pinned in the root `Cargo.toml`
`[workspace.dependencies]` and recorded in `docs/VERSIONS.md`.

- **Consume the generated types directly.** Never re-model or re-serialize
  FHIR by hand, and never shadow a generated shape with a local type.
- **A wrong or missing shape is requested on the FerroBRIDGE tracker**, fixed
  in the generator there, and taken here as a new release.
- **Taking a new release moves three things in one change:** the requirement in
  the root `Cargo.toml`, the pin row in `docs/VERSIONS.md`, and `Cargo.lock`.
  `scripts/checks/versions.sh` fails when they disagree, and Dependabot opens
  that pull request by itself.

## Tech stack (pinned in the manifests)

Rust stable, edition 2024, resolver 3, pinned in `rust-toolchain.toml`. No
`unsafe` (`unsafe_code = "forbid"`). The authoritative dependency set is the root
`Cargo.toml [workspace.dependencies]`; add to a crate with `dep.workspace =
true`. The menu:

- **Ontology store/index:** `redb` (pure-Rust, disk-backed, ACID embedded
  engine, the persistence substrate), `roaring` (transitive-closure bitmaps),
  `fst` (description term dictionary), `petgraph` (in-memory CSR/graph algorithms
  for the offline build). Pure Rust and disk-backed; the artifacts are built
  ahead of time and read at startup, and nothing maps a file (`redb` dropped
  its mmap backend in 0.14.0, and the workspace forbids `unsafe`). Not
  Elasticsearch, not a graph database, not a C storage or search library.
- **HTTP/async:** `axum`, `tower`, `tower-http`, `hyper`, `tokio`.
- **Parsing (ECL):** `logos` (lexer) and `winnow` (parser). Prefer `winnow` over
  `chumsky`, which is still pre-1.0 with churn; the ECL grammar stays faithful to
  the official ANTLR `.g4`. Diagnostics via `miette`/`ariadne`.
- **RF2:** `csv` (RF2 is tab-delimited), streamed.
- **Serde/formats:** `serde`, `serde_json`; FHIR JSON (and XML where a version
  needs it) through the generated codec.
- **Errors:** `thiserror` in libraries, `anyhow` only in the binary. No
  `unwrap`/`expect` outside tests.
- **IDs/time:** `jiff` for FHIR dates and times.
- **Observability:** `tracing`, `tracing-subscriber`.
- **Testing:** `cargo-nextest`, `insta` (snapshots), `proptest`, `reqwest` with
  `wiremock` (the Snowstorm oracle comparison and HTTP mocking).

Verify every crate version against crates.io/docs.rs at the moment it is added;
never pin from memory.

## Build and test

```bash
cargo build --workspace
cargo nextest run --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
```

Conformance is measured against Snowstorm as the reference server (see
`docs/architecture.md` §Verification).

## Conventions

- Crate boundaries mirror the layers; dependencies point downward (app/tools
  depend on crates). Consume the generated `fhir-types` types directly; never
  re-model or re-serialize FHIR by hand.
- Two disciplines by layer. `fhir-types` is generated and published by
  FerroBRIDGE (a shape it lacks is a generator fix there); everything else is
  idiomatic Rust of our own design, with the FHIR/SNOMED specs as the
  authority.
- `thiserror` in libraries, `anyhow` only in the binary. No `unwrap`/`expect`
  outside tests. No `use X as Y` import renaming; use direct names only.
- Async-first: the server is I/O-bound; use idiomatic tokio/axum.

## IMPORTANT hard rules

- **The FHIR and SNOMED specifications are the oracle.** Implement and review
  every spec-facing behaviour (the terminology operations, ECL, RF2 semantics,
  subsumption, value-set expansion) against the vendored FHIR definitions and the
  SNOMED CT / ECL specifications, never from memory. Cite the spec (FHIR operation
  page, ECL grammar section) for conformance-relevant decisions.
- **Cite only durable references:** the FHIR specification, the SNOMED CT / ECL
  specifications, or official external documentation (the Rust book/reference,
  docs.rs of a pinned crate). Never cite an internal markdown file as a design
  authority. Where no spec governs a decision (storage mechanics, the index
  format, infra), flag it: "no spec governs this, our own design".
- **Never shadow a generated FHIR shape with a local type.** A missing or
  wrong shape in `fhir-types` is a generator fix in FerroBRIDGE, released and
  then consumed here.
- **Never distribute SNOMED CT content.** SNOMED CT is licensed by SNOMED
  International; the repository ships no RF2 content and no derived edition data.
  A deployment brings its own licensed RF2 release. Test fixtures use shaped,
  synthetic content only, never real SNOMED concepts extracted from a release.
- **A vendored corpus is verbatim, pinned, and provenance-stamped** (a
  `PROVENANCE.md` beside it, a `scripts/vendor/*.sh` fetcher). Never hand-edit
  one; change the fetcher and re-run.
- **Comments follow RFC 505 and RFC 1574 with hard budgets:** line comments only,
  pending work is `// TODO(#N):`, a settled decision is `// NOTE:` (a citation and
  one sentence). No essays in code; the record lives on the issue/PR.
- **Prose follows `writing-style.md`:** no em dashes, no "not X but Y", no
  decorative triads, no filler buzzwords.
- **Branches use conventional types** (`feat/`, `fix/`, `chore/`, `docs/`,
  `refactor/`, `perf/`, `test/`, `ci/`).
- **NEVER add AI/Claude attribution** to commits, PRs, issues, or code: no
  `Co-Authored-By`, no "Generated with", ever.
- **Never weaken, skip, or delete a test** to make a build pass.
- **Build compiling, tested increments.** Do not defer compilation; keep every
  crate you touch green.

## Issue workflow (the loop)

The tracker is GitHub Issues; the open issue list is the worklist
(`.claude/rules/issue-workflow.md`). The type (`Bug`, `Feature`, `Task`), the
priority (`Urgent`, `High`, `Medium`, `Low`) and the effort of an issue are the
FerroHEALTH organisation's native issue type and issue fields, never labels:
file every issue with `scripts/gh/fields.sh new <type> <priority> <effort>
...` and change one with `scripts/gh/fields.sh`. A `Task` carries exactly one
work-kind label (documentation/chore/refactor/perf/test/ci), plus domain and
area labels as needed. Read an issue with `gh issue view <n> --json
title,body,comments`, never `--comments`. Milestones are releases. Record
progress on the issue (tick criteria, comment); a PR declares `Closes #N` and
arms auto-merge at once with `gh pr merge <n> --auto`, which puts it in the
merge queue. A user-visible change adds a fragment under `changelog.d/`, never
an edit of `CHANGELOG.md`. New work found while working an issue is filed and
fixed before the next unit starts (fix-first cadence). Native sub-issue and
dependency edges are set only with `scripts/gh/rel.sh`; the roadmap board is a
view over the tracker, written only with `scripts/gh/project.sh`
(`.claude/rules/issue-relationships.md`, `project-board.md`). The SessionStart
hook prints the open issue list with `<Type/Priority>` after each title.

## Working discipline (`.claude/`)

Path-scoped rules load on demand when files in their scope are read; the rest
apply always. Read the relevant one before working in that area.

- `.claude/rules/writing-style.md`: no AI tells in any prose (the top priority
  for docs and comments).
- `.claude/rules/rust-style.md`, `reliability.md`, `comments.md`, `testing.md`:
  the Rust engineering discipline (idiomatic style, safety posture, comment
  budgets, test discipline; oracles are Snowstorm/Hermes and the ECL grammar).
- `.claude/rules/spec-adherence.md`: the FHIR and SNOMED/ECL specs are the
  oracle; strictness; cite the spec.
- `.claude/rules/fhir-terminology.md`: FHIR wire conformance (operations,
  versioning, `$expand` paging, `OperationOutcome`, language), scoped to the wire
  crates.
- `.claude/rules/snomed-terminology.md`: the SNOMED URI standard, implicit value
  sets and concept maps, ECL 2.2, RF2 handling, scoped to the SNOMED crates.
- `.claude/rules/codegen.md`, `vendored-inputs.md`: how the generated
  `fhir-types` crate is consumed and the vendored-input /
  SNOMED-content-never-committed rules.
- `.claude/rules/ci-cd.md`, `ai-code-review.md`: the CI/CD and supply-chain
  discipline (SLSA L3, signed SBOM, pinned actions) and the advisory-Sonar
  policy.
- `.claude/rules/crates-publishing.md`: the published `crates/*` line (plain
  names on crates.io, the lockstep bump rule, Trusted Publishing lanes).
- `.claude/rules/leptos-ui.md`: the viewer discipline (client-side rendering,
  the FHIR-client boundary, code-system neutrality, reactivity, accessibility),
  scoped to `app/ferroterm-viewer`. The design is `docs/viewer.md`.
- `.claude/rules/issue-workflow.md`, `issue-relationships.md`, `project-board.md`:
  the tracker work-style.
- Skills: `/spec-lookup` (find the authoritative spec answer in oracle order),
  `/next-task`, `/phase-done`, `/phase-status` (the issue loop),
  `/leptos-lookup` and `/ui-gates` (the viewer).
- Agents: `spec-researcher`, `fhir-conformance-reviewer`, `implementer`,
  `leptos-reviewer`, `ui-implementer` (all on Opus 5).

**Spec-oracle precedence:** (1) the FHIR normative spec for the served wire
version, (2) the SNOMED CT URI and ECL specifications, (3) the SNOMED-on-FHIR IG,
(4) the HL7 `fhir-tx-ecosystem-ig`. Reference servers (Snowstorm, Ontoserver,
tx.fhir.org) are behavioural oracles for spec-silent edge cases only.

## References

- @docs/architecture.md: the design authority (the four decisions and citations).
- @docs/implementation.md: the scope checklist. The project has no fixed version
  scope; this list is what the server must do, and release contents are decided
  during development. See `.claude/rules/writing-style.md` §Scope framing.
- @docs/ci-cd.md: the CI/CD and supply-chain design.
- `docs/viewer.md`: the viewer design (the packaging decision, the stack, the
  screen inventory, the build checklist).
- `docs/release.md`: the release checklist (the cut, in order).
- @docs/VERSIONS.md: the pinned version matrix.
- `website/book`: the mdBook documentation site (`website/book/src`).
- `website/landing`: the landing page.
- The FHIR terminology module (per version) and the ECL specification are the
  spec oracles; the FHIR model itself is the `fhir-types` crate from
  crates.io.
