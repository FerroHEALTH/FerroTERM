<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# concept-store

The disk-backed concept store. Hand-written; no spec governs the on-disk
layout (our own design, `docs/architecture.md` decision 3). `redb` owns the
file I/O and its page cache, and maps nothing: it has had no memory-mapped
backend since 0.14.0 dropped one that could not be proven sound. The cache is
capped at 64 MiB rather than redb's default gibibyte, which a store answering
designation rows at random would fill and hold (#322).

Ordinal-keyed data is a dense column read into memory when the store opens
(concepts, displays, properties, acceptability): a dense key already says
where its value is, and a b-tree spends its time deciding that again, on both
the read and the write. Each column is its own file beside the database,
`<stem>.<column>.col` from `Column::file`, read once through a buffered reader
into the vectors it is served from. Held in the database a column arrived
twice, as the value redb materialized and as the copy the store served from,
and the pages of the first stayed with the process: 168 MB of a Dutch
edition's footprint, which the side files removed (#641). The designation text
stays in the database, one row per concept, because as a column it measured
239 MB resident per SNOMED edition and the read it would speed up already
costs three microseconds (#338). `redb` keeps what a b-tree is for: the
string-keyed code index, `META`, and the small vocabulary tables.

Opening takes one read snapshot and keeps the code index and the designation
rows open on it for the store's life. A request path then pays one b-tree
descent instead of a transaction, a table-name lookup, and a descent. Measured
paired in one process over the NL edition, medians of thirty interleaved
passes: `ordinal` 1001 ns to 622 ns and `designations` 1516 ns to 1137 ns, so
about 380 ns a read either way (#314). `META` and the vocabulary tables are
read when a provider opens, never on a request path, so they keep opening
their own transaction.

- Modules: `column` (the dense column, its side file, and the one reader every
  opener goes through), `tables` (the table set and `META` keys), `record` (the byte
  encodings of concepts, designations, and typed property values, decoded with
  typed errors), `builder` (one write transaction per artifact, the
  precomputed preferred designations, a deterministic commit), `store`
  (read-only point reads).
- Code-system-neutral: a store holds one code system version keyed by dense
  ordinal with the system's native code as the string key; property keys,
  designation uses, language reference sets, and acceptabilities are small
  vocabulary tables the loader fills. SNOMED's SCTIDs, LOINC's codes, and
  ICD's categories all fit the same tables.
- Values are byte strings decoded by `record`, never redb-typed structs:
  redb's `from_bytes` cannot fail, and a damaged artifact must be a typed
  error, not a panic.
- Point reads only on the request path; the one scan (`vocabulary_ordinal`)
  walks a table of a few dozen rows. Whole-table work is the offline build.
- A layout change bumps `LAYOUT_VERSION`, so an artifact of another layout is
  refused either way round rather than read as garbage, and the editions are
  rebuilt.
- The store is opened read-only by the server (`ReadOnlyDatabase`); the
  writer is the offline build.
- Fixtures are synthetic stores in a temporary directory; the ignored
  `local_edition` test builds the licensed release under `data/`.
