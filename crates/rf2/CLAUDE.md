<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# rf2

The SNOMED CT RF2 loader and the typed component model. Hand-written; the
RF2 release file specification is the authority
(<https://docs.snomed.org/snomed-ct-specifications/release-file-specification>).

- Modules: `id` (SCTID with Verhoeff check, partition, namespace; the
  `ConceptId`, `DescriptionId`, `RelationshipId`, `RefsetId`, `ModuleId`, and
  `MemberId` newtypes), `time` (`effectiveTime`), `file` (the file-name
  grammar and the release scan), `reader` (one streaming reader with header
  validation and file, line, column errors), `component` (concept,
  description, relationship, concrete value, alternate identifier rows),
  `refset` (members typed by the file name's `c`/`i`/`s` pattern, with typed
  views per reference set), `constants` (published SCTIDs, each check-digit
  tested), `edition` (the edition and version URI from the module
  dependency refset).
- Stream every RF2 file with `csv` (tab-delimited, no quoting); never load a
  whole file into memory.
- Never pass a bare integer where a typed id belongs
  (`.claude/rules/reliability.md`); a published SCTID is spelled once, in
  `constants`, and tested.
- Snapshot semantics: one row per component id, `active` and
  `effectiveTime` carried as typed fields, never dropped.
- The relationship and concrete value rows carry their `characteristicTypeId`,
  and `Rows::inferred` is the one seam that narrows a file to
  `900000000000011006 |Inferred relationship|`: the rest of the enumeration is
  not part of a concept's definition, so a caller building a hierarchy or an
  attribute value reads through it. The adapter counts what it left out
  (`InferredRows::skipped`) and a caller reports that count; a row it drops is
  never a row the reader failed to parse, which still surfaces as an error.
- Fixtures are shaped, synthetic RF2 only, minted with valid check digits in
  an invented namespace. The licensed release under `data/` is read only by
  the ignored `local_edition` test (`.claude/rules/vendored-inputs.md`).
