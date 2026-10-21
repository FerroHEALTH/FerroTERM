<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# loinc

The LOINC release loader. Hand-written; the LOINC Users' Guide and the FHIR
LOINC page (<https://hl7.org/fhir/R4B/loinc.html>) are the authority.

- Reads the release files by column name, never by position, so a column
  added by Regenstrief does not move the ones the loader needs.
- Every code is checked against the LOINC Mod 10 check digit.
- The release itself is never committed; fixtures are shaped like the
  release, under the LOINC licence terms for the codes they carry.
