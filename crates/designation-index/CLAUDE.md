<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# designation-index

The designation search index. Hand-written; no spec governs the index format
(our own design). The FHIR `$expand` `filter` parameter semantics and each code
system's search guidance (SNOMED's per-word prefix matching, LOINC's long and
short names) govern what a query must return.

- Code-system-neutral: per-word prefix matching over an `fst` set; posting
  lists are roaring bitmaps of designation ordinals. Language and
  designation-use filters are bitmap intersections, never a post-filter loop
  over strings.
- Result order is deterministic: matched-term length, then designation
  ordinal. `$expand` paging depends on it.
- Fixtures are invented terms over synthetic ids only.
