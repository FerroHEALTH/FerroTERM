---
paths: ["crates/**/*.rs", "addons/**/*.rs", "app/**/*.rs", "tools/**/*.rs"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Comments & documentation (RFC 505 + RFC 1574)

The authority is the official Rust API documentation conventions: RFC 505
(https://rust-lang.github.io/rfcs/0505-api-comment-conventions.html) and
RFC 1574 (https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html),
plus the rustdoc book. Comments are the fastest-rotting artifact in a
repo; essays in code are banned. This file is the single home of the comment
rules; `rust-style.md` points here.

## The prime rule: a comment earns its lines

Code says WHAT; a comment exists only for what the code cannot show: the
spec citation, the non-obvious why, the constraint. Everything else already
has a durable home and goes there, not into the source:

| Content | Home |
|---|---|
| Adjudications, spec-conflict essays, history | the PR description / tracker issue |
| Design decisions | `docs/architecture.md`, the crate `CLAUDE.md` |
| API usage, contracts, examples | doc comments (`///`) |
| What changed and why it is correct | the PR, never the code |

**No change-narration in comments**: "previously…", "now correctly…",
"before this refactor…", "the former X is retired" is PR text; git history
carries it. A comment describes the code as it IS.

## The file header

Every first-party file in a format that takes a comment opens with two SPDX
lines, in that format's comment syntax, before anything but a shebang, a
doctype, a Dockerfile parser directive or a Markdown front matter:

```rust
// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
```

`crates/rf2` says `Apache-2.0`. Vendored trees (a directory with a
`PROVENANCE.md`), JSON and the changelog fragments carry none.
`scripts/checks/spdx-headers.sh` fails a file without the pair and
`scripts/checks/copyright-holder.sh` fails a header naming another holder.
The header does not count toward the budgets below.

## Budgets

- `// NOTE:` = a citation + ONE sentence, **max 3 physical lines**. The full
  adjudication lives on the issue/PR.
- A plain `//` comment run is **max 8 physical lines**. Longer prose is
  either API documentation (move it into the item's `///` docs) or a record
  (move it to the PR/issue).
- Block comments (`/* … */`, `/** … */`) are banned: "Avoid block comments.
  Use line comments instead" (RFC 505). `/* arg */` parameter labels become
  named locals.

## Doc comments (`///`)

- `///` documents items; `//!` is ONLY for crate- and module-level docs
  ("nothing else", RFC 1574); above a `mod` block prefer `///` outside it.
- **Summary line**: the first line is a single short sentence, third person
  singular present indicative ("Returns…", "Creates…", never "Return" or
  "This function returns"), properly punctuated, followed by a blank `///`
  line before any detail (`clippy::too_long_first_doc_paragraph`).
- Full sentences over fragments; American English spelling.
- Markdown with `#` top-level section headings, in this order where present:
  `# Examples` (always plural, even for one example), `# Panics`,
  `# Errors`, `# Safety` (`missing_errors_doc` / `missing_panics_doc`
  enforce presence; `unnecessary_safety_doc` bans `# Safety` on safe code).
- Code fragments in backticks (`clippy::doc_markdown`); longer examples in
  triple-backtick blocks, non-Rust blocks explicitly tagged (```text):
  rustdoc tests every untagged block as Rust. Doctest shapes: `testing.md`
  §Test shapes.
- Name generic types fully (`Option<T>`, not `Option`); link with intra-doc
  links (`[`Type`]`) and reference-style links; bare URLs in `<…>`.
- Doc comments state the CURRENT contract only, with no history and no
  adjudication trail. Citations follow `spec-adherence.md` (the FHIR /
  SNOMED CT / ECL specs + official external docs only).
- The `fhir-types` crate gets its docs from its generator, which lives in the
  FerroBRIDGE repository: a doc defect there is fixed and released upstream,
  never patched here (`codegen.md`).

## Annotation vocabulary (the only sanctioned markers)

- `// TODO(#NNNN): <what is missing>`, pending work, ALWAYS with its
  tracker issue. Deferred work is a TODO, never prose ("lands later",
  "deferred to X" is banned); phase/plan markers (A5/P16/W-nn) are banned.
- `// NOTE: <citation + one sentence>`, a SETTLED decision: the spec
  citation, or the explicit flag "no FHIR/SNOMED spec governs this: our own
  design". If it describes something not yet done, it is a TODO.
- `// SAFETY:`, reserved for `unsafe` (forbidden anyway;
  `unnecessary_safety_comment` denies misuse).
- No other marker form exists: `FIXME`, `HACK`, `XXX`, `WIP`, and any
  bespoke vocabulary fail the guard.

## Enforcement register

- `scripts/checks/comment-style.sh`: block comments, TODO(#N) form, banned
  marker vocabulary, NOTE ≤ 3 lines (≤ 8 in doc comments), `//` runs ≤ 8
  lines, punctuation-only comment lines (sweep residue), backtick-quoted
  markers used AS markers on doc lines, and empty `# Errors`/`# Panics` doc
  sections. Runs per-edit (the `rust_fmt_clippy.sh` PostToolUse hook); a CI
  job running `--all` over the whole tree is added when CI is stood up.
- `clippy::too_long_first_doc_paragraph` (CI `-D warnings`): the RFC 1574
  summary line. Configured in `[workspace.lints]` when the workspace exists.
- Companion doc lints (`doc_markdown`, `missing_errors_doc`,
  `missing_panics_doc`, `unnecessary_safety_comment`, `unnecessary_safety_doc`,
  the `[workspace.lints.rustdoc]` table + the CI doc job): the same file.
- Review-enforced (no tool can judge them): change-narration, prose
  deferrals, third-person summary phrasing, and essays relocated into doc
  comments to dodge the `//` budget.
