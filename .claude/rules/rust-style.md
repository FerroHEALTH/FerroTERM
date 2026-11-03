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

## Code is written once

No specification governs this: our own design (#950, ported from
VernumBOEK's rule of the same name, as FerroEHR did).

- **Share knowledge and decisions, never text that only looks alike.** Two
  copies that change for the same reason (one business rule, one wire rule,
  one security rule) move to one owner in the same change. Two pieces with
  different reasons to change stay apart, however alike they read.
- **The owner is the module that owns the concern.** Across crates it is the
  crate lower in the layering (a substrate such as `concept-graph`,
  `concept-store` or `designation-index` below `fhir-terminology`,
  `fhir-terminology` below `app/ferroterm-server`, and
  `terminology-syndication` below an add-on). Within a crate it is the module
  whose part of the domain the code serves. It is never a `util`, `utils`,
  `helpers` or `common` module, whose name says nothing about what it holds,
  and never a function that takes a flag to choose between two behaviours.
- **One copy is fine, two are looked at, three are extracted.**
- **The ratchet measures it.** `scripts/checks/duplication.sh` runs jscpd
  (pinned by version and image digest in `docs/VERSIONS.md`, CI tools) over
  `addons/*/src/`, `app/*/src/`, `crates/*/src/` and `tools/*/src/` in the
  `duplication` CI job and holds the total of duplicated lines to the count
  in `scripts/checks/duplication.count`. A rise fails and names both places
  of each new block. A fall passes with a one-line note, and your pull
  request leaves the count alone: the `Ratchet tighten` workflow runs
  `scripts/checks/duplication.sh --update` on `main` each day and opens one
  pull request with the lower count and
  `scripts/checks/duplication-baseline.txt`, the guard's own key of every
  counted block: its two paths and the sha256 of the lines at each, with no
  line number, so a change that leaves a block's lines alone leaves its key
  alone. A pull request that rewrote the shared count files would conflict
  with every other open one (#954). `--update` never raises the count; a
  rise that has to stay is a hand edit of the file in a change that says
  why. Locally the guard takes a `jscpd` on `PATH` of the pinned version,
  then the pinned image through docker or podman, then the release binary
  `scripts/checks/duplication.sh --install` puts under `target/tools/`.
- **The threshold is 50 tokens over at least 5 lines**, jscpd's own
  defaults. The scan leaves out the `use` declarations, a file that opens
  with `// @generated`, a file from its first column-one `#[cfg(test)]` line
  on (the inline test module), and `tests/`. Measured on 2026-11-01 at
  `1cc74120` with those exclusions, over 307 files and 122,696 lines:

  | Threshold | Blocks | Duplicated lines | Blocks across two files | Blocks in the console |
  | --- | --- | --- | --- | --- |
  | 50 tokens, 5 lines | 321 | 4,081 | 204 | 53 |
  | 70 tokens, 5 lines | 196 | 2,990 | 127 | 20 |
  | 100 tokens, 5 lines | 88 | 1,763 | 65 | 7 |

  The last column counts the blocks whose first place is in
  `app/ferroterm-console`, spread over its pages, the most in the code
  system editor and the browse screen. Raising the threshold drops the short blocks (two getters,
  two `format!` helpers, two match arms written side by side) and keeps
  whole functions, yet each short block is code written twice as well, so 50
  it is. Raising the threshold is a change to this section and to the guard
  together, with the table measured again.
- **A hand-written file carries at most 800 lines**, its inline test module
  included. `scripts/checks/file-size.sh` measures every tracked `.rs` file
  under `addons/*/src/`, `app/*/src/`, `crates/*/src/` and `tools/*/src/`
  that does not open with `// @generated`, in the `file-size` CI job and per
  edit through `.claude/hooks/rust_fmt_clippy.sh`. The 34 files over the
  budget when the guard landed stand in `scripts/checks/file-size.list` with
  their length; a listed file may shrink and never grow. A shrink, a listed
  file back within the budget and a listed file that is gone pass with a
  one-line note, and the `Ratchet tighten` workflow lowers or removes the
  entry on `main`; your pull request leaves the list alone. A file over the
  budget is split by concern.

## No function only forwards to another

No specification governs this: our own design (#972, the rule FerroEHR
adopted in FerroHEALTH/FerroEHR#4663).

- **A function exists because it decides something.** It validates,
  branches, converts, owns a transaction, or names a concept its callers
  need. A function whose body is a single call, with its arguments passed
  through or merely borrowed and at most `.await`, `?`, `Ok(..)`,
  `Some(..)`, `.into()` or an error-only `.map_err` around it, is a
  forwarder. It adds a name and a hop and decides nothing, so its callers
  call the target directly, and internal and crate APIs may break to get
  there. A forwarder that fixes an argument is still one: its callers pass
  the argument.
- **Three kinds are not forwarders.**
  - The methods of a trait implementation (`Display`, `From`, `Deref` and
    the rest), which the trait requires. A binary's `main`, which the
    language requires, and a function a specification defines, whose doc
    comment cites its `.adoc` page, stand with them.
  - A `pub` function in a crate's root API that re-exposes a private
    module's function under the crate's own name, kept only where the
    baseline records why.
  - A constructor (`new`, `with_*`, or an associated function that returns
    `Self`) that fixes an argument.
- **The ratchet measures it.** `tools/forwarders` parses every `.rs` file
  under `addons/*/src/`, `app/*/src/`, `crates/*/src/` and `tools/*/src/`
  with `syn`, leaving out a file that opens with `// @generated`, anything
  under `#[cfg(test)]`, and `tests/`. It reports a free function or inherent
  method whose body is one call, to a function or to a method of `self`,
  whose arguments are parameters, literals and constants, or references,
  borrows and clones of those. A call that reads a field of `self`, or calls
  a method of a field or a parameter, reaches what the caller cannot, so an
  accessor is never reported; nor are the `#[cfg]` arms of one function.
  `scripts/checks/forwarders.sh` runs it in the `forwarders` CI job and per
  edit through `.claude/hooks/rust_fmt_clippy.sh`, and holds the total to
  `scripts/checks/forwarders.count`. `scripts/checks/forwarders-baseline.txt`
  keys each counted forwarder by its path, owner and target call, with no
  line number, and a key may carry a tab and the reason the forwarder stays.
  A rise fails and names each new forwarder, its target and its place. A
  fall passes with a note, and the `Ratchet tighten` workflow lowers the
  count on `main`, as it does for the duplication ratchet. `--update` never
  raises the count and keeps each reason while its key stands. The 35
  forwarders on `main` when the guard landed stand in the baseline; folding
  them is the refactor work that follows.

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
