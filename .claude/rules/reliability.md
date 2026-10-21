<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Reliability & safety hard rules (safety-critical Rust)

A terminology server answers clinical questions: a wrong subsumption result,
a mis-expanded value set, or a silently dropped code becomes a wrong answer
in a downstream clinical system. Silent wrong answers are worse than loud
failures. The principles below are the standing bar for hand-written code;
each names the lint or check that enforces it. A rule without a failing check
is a wish, so when the workspace is stood up every lint here is wired into
the root `Cargo.toml [workspace.lints]` in the same change. Principles from
the Rust API Guidelines checklist, the Rust Book's error-handling/overflow
chapters, and the Clippy book.

## Enforcement tiers (strongest first)

1. **Compile property:** the type system makes the violation
   unrepresentable (newtypes, `#[must_use]`, sealed traits, `forbid`).
2. **Workspace lint at `deny`/`forbid`:** fails every `cargo clippy`, local
   and CI (`Cargo.toml [workspace.lints]`). `forbid` cannot be relaxed by any
   attribute; escaping it is an owner decision, not an `#[allow]`.
3. **Deny, with `clippy::all` and `clippy::pedantic` at `deny`:** every
   pedantic lint is a hard rule locally and in CI (including
   `missing_errors_doc` / `missing_panics_doc`). The table is FerroEHR's
   (the owner's rule, 2026-09-04): the two repositories hold the same bar,
   with `as_conversions`, `pub_use`, `dead_code`, `map_err_ignore`,
   `missing_assert_message`, `unused_qualifications`, `rc_buffer`,
   `create_dir`, `exit`, and the feature-name lints among the additions.
4. **A committed check script / CI job:** currently
   `scripts/checks/comment-style.sh` (per-edit via the hook); more are added
   as CI is stood up.
5. **Review-enforced** (weakest; minimize): only for properties no tool can
   check, each marked below.

## The rules

- **No `unsafe`, ever** (`unsafe_code = "forbid"`, tier 2). No exceptions; a
  need for `unsafe` is a design defect to solve differently.
  `unnecessary_safety_comment` / `unnecessary_safety_doc` deny a `// SAFETY:`
  comment or `# Safety` doc section on safe code. (Memory-mapping is provided
  by `redb`/`fst`, which own the `unsafe` internally; the server code stays
  safe.)
- **Fail loud, never wrap**: release builds run with `overflow-checks = true`
  so integer overflow panics instead of silently wrapping into a wrong
  answer. On load-bearing arithmetic (SCTID/ordinal math, bitmap offsets,
  `$expand` pagination) prefer explicit `checked_*`/`saturating_*` with a
  typed error; a panic is the backstop, not the design. Numeric-honesty
  lints back this at deny tier: `float_cmp_const`, `lossy_float_literal`,
  `as_underscore`, `fn_to_numeric_cast_any`, `ambiguous_negative_literals`
  (+ `integer_division` at warn).
- **The panic strategy is `unwind`, pinned in `[profile.release]`, never
  `abort`**: a clean 500 from the HTTP layer relies on
  `std::panic::catch_unwind`, which "only catches unwinding panics", and
  Cargo documents that tests IGNORE the `panic` setting, so an abort
  regression would be untestable by construction
  (https://doc.rust-lang.org/cargo/reference/profiles.html#panic). Keep
  `debug = "line-tables-only"` so a production panic names its file:line.
- **No `unwrap`/`expect`/`panic!`/`unreachable!`/`unimplemented!`/`todo!` in
  application code** (deny-tier lints). Tests keep the `clippy.toml`
  `allow-*-in-tests` scoping; a panicking assertion is how a test fails.
  Recoverable failures return typed errors (`thiserror` in libraries,
  `anyhow` only in the binary), the Book ch9 split: panic is for states that
  cannot happen, `Result` for everything that can.
- **The ONE sanctioned escape for a logically-impossible `Err`/`None`** (Book
  ch9): a narrowly-scoped `#[expect(clippy::expect_used, reason = "…")]` on
  the smallest item, whose reason states the inspection proving
  unreachability, plus a *should*-phrased message (`.expect("the closure
  bitmap should exist for a classified concept")`). Dodging the lint with
  `unwrap_or_default()` instead is FORBIDDEN: that converts a loud
  impossible state into a silent wrong answer, the exact failure class this
  file exists to prevent.
- **No panicking indexing on request paths** (deny tier: `indexing_slicing`,
  `string_slice`): `.get(..)` / pattern matching over `x[i]` and `&s[a..b]`.
  `string_slice` panics on a UTF-8 boundary and SNOMED descriptions and FHIR
  text are full of multi-byte content. Tests are scoped out via `clippy.toml`;
  a hot-path site PROVEN in-bounds uses the `#[expect]` escape above.
- **Guards are never silently dropped**: `let _ = lock/handle;` is denied
  (`let_underscore_drop` + `let_underscore_lock`): bind guards to named
  variables that live to scope end. `unused_result_ok` (deny) closes the
  `.ok();` variant. Edition-2024 corollary (review-enforced): a guard/borrow
  produced in an `if let` scrutinee is dropped BEFORE the `else` branch runs
  (https://doc.rust-lang.org/edition-guide/rust-2024/temporary-if-let-scope.html);
  rewrite as `match` when the guard must span both arms.
- **An error carries its cause** (RFC 0201; `Error::source`). Every wrapping
  variant uses `#[source]` / `#[from]`; a stringified cause
  (`map_err(|e| Variant(e.to_string()))`) cannot be walked, matched, or
  logged structurally, the same silent-context-loss class this file
  legislates for `Result → Option`. There is no lint for it; new code carries
  the source and reviewers check for it. Two verified thiserror foot-guns:
  `#[source]` over an `Option<Box<…>>` yields the smart pointer as the source
  hop, not the error inside it (hand-write `Display`+`Error` returning
  `self.source.as_deref()`), and `#[error(transparent)]` removes its own type
  from the chain, so a test walking the cause chain asserts the ROOT cause,
  not an intermediate wrapper type.
- **`Result → Option` inside a chain is a DECISION** (review-enforced; no
  lint can make the distinction). `.filter_map(|x| f(x).ok())`,
  `f(x).ok()?` turn an error into a missing element with no trace. The rule: a
  fallible conversion whose failure means "the input is DEFECTIVE" propagates
  a typed error; only one whose failure means "this input is legitimately
  ABSENT / not of this form" may become `Option`, and it carries a `// NOTE:`
  saying so. (An ECL grammar probe where a parse failure IS the answer is the
  legitimate shape; a code silently swallowed in the FHIR codec is the defect
  shape, and they read identically, which is why judgment carries it.)
- **Determinism is lint-backed** (deny tier: `iter_over_hash_type`):
  HashMap/HashSet iteration order is undefined; anything that feeds generated
  code, a canonical FHIR serialization, or an ordered `$expand`/search result
  iterates ordered structures (`BTreeMap`/sorted vecs / roaring bitmaps,
  which iterate in sorted order). Byte-determinism is a codegen emitter
  invariant (`codegen.md`) and a wire-parity requirement.
- **No debug/print output from libraries** (`dbg_macro`, `print_stdout`,
  `print_stderr` denied): libraries speak `tracing`; only the binaries and
  `tools/*` write to stdio (crate-root `#![allow]`/`#![expect]` relaxations
  there, each with a reason).
- **Banned APIs are compile-time bans** (`clippy.toml`
  `disallowed-methods`/`disallowed-types`): `std::time::SystemTime::now`
  (wall-clock time for FHIR dates comes from `jiff`; `Instant` stays fine for
  latency), the `chrono::*` types (`jiff` is the one time library), and
  `Option::as_slice`/`as_mut_slice` (on an `Option<Vec<T>>` receiver they
  yield `&[Vec<T>]`, a slice of 0-or-1 vectors, not `&[T]`, and keep
  compiling after a field flips between `Vec<T>` and `Option<Vec<T>>`; spell
  it `.as_deref().unwrap_or_default()` or match). A legitimate exception site
  carries a scoped `#[expect(clippy::disallowed_methods, reason)]`.
- **Errors are types, not strings, at every boundary that branches**
  (C-GOOD-ERR): a caller that needs to distinguish outcomes gets an enum
  variant, not a substring match. String context belongs in the display text,
  not the discriminant. (Review-enforced; status-mapping tests pin the wire
  outcome.)
- **Ids are distinct types where confusion is fatal** (C-NEWTYPE): a
  `ConceptId` (SCTID), a `DescriptionId`, and a `RefsetId` are distinct
  newtypes over their integer keys, so the type system rejects a
  swapped-argument mistake at compile time. Never pass a bare integer where a
  typed id belongs, and never add a function that takes two adjacent bare-id
  parameters.
- **Every public item: documented, `Debug`, with concrete
  `# Errors`/`# Panics` sections** (C-DOC, C-DEBUG, C-FAILURE):
  `missing_docs`, `missing_debug_implementations`, `missing_errors_doc` /
  `missing_panics_doc`. The generated `fhir-types` crate gets its docs from
  its generator, which is not in this repository (`codegen.md`).
- **Visibility is deliberate** (C-STRUCT-PRIVATE): private by default, scoped
  visibility only at real module boundaries, zero re-exports (every import
  names its defining module), `unreachable_pub` watched at CI. Struct fields
  private unless the type IS a plain record.
- **Constructors and conversions follow the standard shapes** (C-CTOR,
  C-CONV): `new`/`with_*` builders, `From`/`TryFrom` over ad-hoc `to_x()`
  where the conversion is total/fallible, getters without `get_` prefixes.
- **Blocking never hides in async**: no `std::sync` locks held across `.await`
  (clippy `await_holding_lock`), no synchronous I/O on the runtime.
  `spawn_blocking` for the rare CPU-heavy transform. (The build tool
  `ferroterm-build` is offline and synchronous; that discipline is the server's,
  not the tool's.)
- **Dependencies are pinned, locked, and vetted**: workspace-table only, no
  new dependency for what the pinned set already provides, verify the version
  against crates.io/docs.rs at the moment of adding. CI builds run `--locked`.
  A `cargo deny` / `cargo audit` gate is added when CI is stood up.
- **Comment style is machine-enforced** (`comments.md`, RFC 505 + RFC 1574):
  line comments only, `// TODO(#NNNN):` (issue reference mandatory), `// NOTE:`
  as a citation + one sentence (≤3 lines), plain `//` runs ≤8 lines,
  `// SAFETY:` reserved for `unsafe`, no unsanctioned marker vocabulary.
  Enforcement: `scripts/checks/comment-style.sh` (per-edit hook) +
  `clippy::too_long_first_doc_paragraph`.
- **An HTTP status is compared as a `StatusCode`, never a number**
  (`rust-style.md` §HTTP statuses). Rendering the number stays legal; only
  comparison against a numeric literal is refused.
- **A field's default value lives inline in its struct's `Default` impl**
  (`rust-style.md` §Default values, the RFC 3681 shape). The per-field
  `#[serde(default = "path")]` form, zero-argument `fn default_x()`, and a
  single-reader `const DEFAULT_X` are all banned.

## Deviations from the API Guidelines (deliberate)

- **C-PERMISSIVE is not followed: the Business Source License 1.1 for the
  project's own code** (the owner's decision, 2026-09-04): free
  for non-production and non-commercial production use, a commercial licence
  for other production use, Apache 2.0 four years after each version. SNOMED
  CT content is licensed separately by SNOMED International and is NEVER distributed here
  (`vendored-inputs.md`). The vendored FHIR packages keep their upstream HL7
  terms, vendored verbatim with provenance as codegen input.
- **C-STABLE: pre-1.0 dependencies are acceptable while the crates are
  unpublished.** If any crate is ever published, re-adjudicate every pre-1.0
  public dependency in its API first.

## When a lint fights a legitimate case

**`#[expect(lint, reason = "…")]` is the default suppression:** it
self-reports the moment the expectation stops being fulfilled
(`unfulfilled_lint_expectations`), so stale suppressions cannot accumulate.
`#[allow(lint, reason = "…")]` is reserved for cases where the lint fires only
in SOME configurations (cfg/feature-dependent code, macro expansions). Both
forms MUST carry `reason = "…"` (`allow_attributes_without_reason` = deny).
Scope every suppression to the smallest item; a file- or crate-level
suppression needs the owner's sign-off in the PR.
