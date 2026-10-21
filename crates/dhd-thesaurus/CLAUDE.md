<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# dhd-thesaurus

The reader for the DHD Diagnosethesaurus and Verrichtingenthesaurus
deliveries. Hand-written; the DHD "Uitleverformaat 5.0" document
(<https://www.dhd.nl/assets/uploads/Uitleverformaat-Thesauri-5.0-v1.0.pdf>)
is the authority for the tables and their columns.

- A delivery is a zip of UTF-8 CSV files with headers, one per table, named
  `<timestamp>_uitleverformaat<version>_<Table>.csv` (`_VT_` for the
  Verrichtingenthesaurus); files are found by their table suffix.
- The thesaurus is a flat table of concepts with terms in two languages;
  its relations (replacement, splitting, umbrella terms) and derivations
  (ICD-10, DBC, ZA, code mappings) are properties, and the SNOMED CT and
  ICD-10 links are also concept maps the build writes.
- The deliveries are licensed by DHD and never committed; fixtures are shaped
  like the tables with invented identifiers and terms.
