---
paths: ["crates/**/*.rs", "addons/**/*.rs", "app/**/*.rs", "tools/**/*.rs"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Rust style: idiomatic application code

Applies to hand-written `.rs`: the SNOMED engine and its crates
(`rf2`, `concept-graph`, `concept-store`, `designation-index`, `sct-ecl`,
`fhir-terminology`), the server (`ferroterm-server`), and the tooling
(`ferroterm-build`). **The engine is modern idiomatic Rust of our own design,
built on the generated `fhir-types` crate.** The FHIR and SNOMED CT / ECL
specifications are the authority; Snowstorm, Ontoserver, and Hermes are prior
art only.

**The FHIR model is generated elsewhere and consumed here.** `fhir-types`
comes from crates.io, published by the FerroBRIDGE repository; a shape that is
wrong or missing is a generator fix there, never a local re-model. Full
discipline: `codegen.md`.

## Build idiomatic, compiling, tested code

- **Consume the generated `fhir-types` types directly** as the FHIR model
  (per-version R4/R4B/R5/R6). Never re-model or re-serialize FHIR by hand.
- **Use proper crates; don't hand-roll** what the ecosystem provides:
  `axum`/`tower-http`, `redb`, `roaring`, `fst`, `logos`+`chumsky`/`winnow`,
  `csv`, `jiff`, `serde`/`serde_json`. Add deps only from
  `Cargo.toml [workspace.dependencies]` (`dep.workspace = true`); verify each
  version against crates.io/docs.rs at the moment it is added, never from
  memory.
- **Every crate you touch compiles + is clippy-clean + tested before you move
  on.**
- **Read the governing spec first for any spec-facing behaviour:** the FHIR
  operation page for the version, the ECL grammar section, the SNOMED CT
  docs (`spec-adherence.md`).
- **The specs are the authority; design the bespoke logic ourselves** (the
  ECL evaluator, the materialized graph, RF2 loading, `$expand` paging),
  verified against Snowstorm/Hermes as reference servers and the ECL ANTLR
  grammar. Consult prior art when useful; never port it blindly.

## Comments & documentation

The comment/doc-comment discipline lives in **`comments.md`** (RFC 505 +
RFC 1574): line comments only, `// TODO(#NNNN):` / `// NOTE:` / `// SAFETY:`
as the only markers, NOTE = citation + one sentence (≤3 lines), `//` runs
≤8 lines, doc-comment summary-line + section conventions. Enforced by
`scripts/checks/comment-style.sh` (hook) and
`clippy::too_long_first_doc_paragraph`.

## Default values live in the struct's `Default` impl (RFC 3681 shape)

The shape is RFC 3681's
(<https://rust-lang.github.io/rfcs/3681-default-field-values.html>), written by
hand: **one** `impl Default` per struct, every default value inline, and
container-level `#[serde(default)]` so serde fills omitted fields from it.

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExpandOptions {
    pub count: usize,
    pub offset: usize,
    pub active_only: bool,
}

impl Default for ExpandOptions {
    fn default() -> Self {
        Self {
            count: 100,
            offset: 0,
            active_only: true,
        }
    }
}
```

The RFC's own syntax (`count: usize = 100`) is **nightly-only** (feature
`default_field_values`, tracking issue
<https://github.com/rust-lang/rust/issues/132162>), and this project pins
stable, so the expansion above is the form we write. Do not reach for
nightly to get it.

Three forms are banned, because each one puts a field's default value
somewhere other than the field's own struct:

- **`#[serde(default = "path")]`:** the per-field path form. The default
  then lives in a function, so `Default::default()` and a deserialized value
  can silently disagree about the same field. Container-level
  `#[serde(default)]` (no path) is the required form and reads the one
  `Default` impl.
- **`fn default_x() -> T`:** a zero-argument constructor that exists to be
  one field's default.
- **`const DEFAULT_X` with a single reader:** a constant that nothing shares
  is a default value spelled far from its struct.

A `const` with MORE THAN ONE consumer stays a constant and may be referenced
from inside the `Default` impl.

## HTTP statuses are compared as types

An HTTP status is a `StatusCode`, and it is compared as one:

```rust
if status == StatusCode::OK { … }          // yes
if status.as_u16() == 200 { … }            // no
```

`http::StatusCode` names every registered code, so there is always a
constant. A numeric comparison throws away the type the crate exists to
provide, and a bare literal tells a reader nothing about which member of a
family was meant: `403` versus `404` is a one-character typo the compiler
cannot catch. Rendering the number (a log field, a metric label) stays legal;
only comparison against a numeric literal is refused.

## RFC-grounded conventions

| RFC | The rule | Enforcement |
|---|---|---|
| 0505 + 1574 | comment/doc conventions | `comments.md` + `scripts/checks/comment-style.sh` + `too_long_first_doc_paragraph` |
| 3681 | a field's default lives inline in its `Default` impl | §Default values above |
| 3107 | `#[derive(Default)]` + `#[default]` where the default is a VARIANT | `clippy::derivable_impls` (deny), hand-write `impl Default` only for VALUES |
| 0199 / 0344 / 0430 | `as_`/`to_`/`into_` cost conventions, naming | `clippy::wrong_self_convention` + rustc naming lints |
| 1940 | `#[must_use]` on functions whose result is the point | `clippy::must_use_candidate` (pedantic) |
| 2383 | every suppression carries `reason = "…"` | `clippy::allow_attributes_without_reason` (deny) |
| 1946 | intra-doc links over bare paths | `rustdoc::broken_intra_doc_links` (deny) + the CI doc job |
| 0201 + 0236 | an error carries its cause (`Error::source`) | `#[source]`/`#[from]` on every wrapping variant; review-enforced |

The lints named above are configured in the root `Cargo.toml
[workspace.lints]` when the workspace is stood up; until then they are the
standing bar for hand-written code.

## Type and error conventions

- `thiserror` error enums in library crates; `anyhow` only in the
  `ferroterm-server` binary. No `unwrap`/`expect` outside `#[cfg(test)]`;
  `todo!()`/`unimplemented!()` are banned: an unready dependency gets a typed
  error or real code, never a panic placeholder.
- Model closed sets as Rust `enum`s; trait objects only for genuinely open,
  runtime polymorphism.
- Back-references use `Weak<..>` or an index, never an owning reference;
  recursive containment is boxed. The materialized graph is integer-keyed
  (SCTID) CSR arrays, not a pointer graph; see `docs/architecture.md`.
- `std::sync::LazyLock` (edition 2024) for statics, not `once_cell`.
- **No `use X as Y` import renaming.** Import types under their direct names.
  An alias papers over a naming problem: if the name is bad, FIX THE NAME at
  its definition; if two imports genuinely collide, qualify one at the use
  site (full path) instead of renaming. Only alias in highly exceptional
  cases where no other solution exists, with a comment saying why. (Trait
  imports as `use Trait as _;` are not renames and are fine.)
- Edition 2024, resolver v3. `cargo fmt` clean; run `cargo clippy` on the
  crate you touched before considering it done.
- **Suppressions are `#[expect(lint, reason = "…")]`**, scoped to the
  smallest item; `#[allow(lint, reason = "…")]` only for cfg/feature-
  conditional fire (full policy: `reliability.md`).

## Documentation (missing_docs is enforced workspace-wide)

- Every public item carries a doc comment (rustc `missing_docs`; the
  generated `fhir-types` crate gets its docs from its generator, upstream).
  Shape, sections, and summary-line rules:
  `comments.md` (RFC 1574).
- Intra-doc links (`[`Type`]`) resolve in the scope of the module where the
  item is DEFINED. `rustdoc::broken_intra_doc_links` is deny; the CI doc job
  is the gate. Literal square brackets in prose are escaped `\[…\]`.

## Edition-2024 standing guidance (behaviour that compiles fine and differs)

- **`if let` scrutinee temporaries drop before `else`:** the guard rule
  lives in `reliability.md`; rewrite as `match` when a guard must span arms.
- **Never-type fallback**: `f()?;` on a fn generic over the `Ok` type can now
  infer `!`; annotate the turbofish/binding type at such call sites instead
  of leaning on inference.
- **RPIT captures every in-scope lifetime by default:** when a returned
  `impl Trait` must NOT capture one, say so with precise capturing (`use<…>`).
- **`Future`/`IntoFuture` are in the prelude:** a collision with a local
  trait method is resolved with fully-qualified syntax, never an import
  rename.
- The `unsafe_*` 2024 items are all moot under `unsafe_code = "forbid"`.

## Naming

A name says what the thing is in its domain, never which language, layer, or
pipeline stage produced it (`VersionModule`, `TypeDef`, `ConceptId`, not
`RustModel`, `GenType`, `SctidValue`). Spec-defined things keep the
specification's names verbatim (`StructureDefinition`, `OperationDefinition`,
`ValueSet`), FHIR camelCase becoming Rust snake_case for fields. When two views
of one thing would collide, the module path disambiguates (`fhir::` versus
`lower::`), never a prefix. Modules are named for their contents (`fhir`,
`snapshot`, `closure`), verbs only for a stage that is genuinely a
transformation (`lower`, `render`, `emit`).

## No Python, anywhere

The tooling languages are **bash and Rust**. Python is banned across the
repository, in standalone scripts, and especially embedded in shell (a heredoc
nested inside a command substitution makes bash scan the *Python* for quote
pairs, so a single apostrophe breaks the whole script pointing at the wrong
line). Reach for:

| Job | Tool |
|---|---|
| JSON read/build | `jq` |
| Line scans, field extraction | `awk`, `sed`, `grep` |
| Anything with real data structures | Rust, in `tools/` |

## What not to do

- Do not port JVM plumbing from Snowstorm (Elasticsearch mappings, Spring
  context, Lucene analyzers): design the idiomatic Rust equivalent (the
  `redb` store, the `fst`+`roaring` index, tower middleware) or register it
  as its own tracker issue with a `// TODO(#NNNN):`.
- Do not re-model what `fhir-types` provides.
