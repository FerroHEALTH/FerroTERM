<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# rxnorm-rrf

The RxNorm release reader. Hand-written; the RxNorm Technical Documentation
(<https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html>) and the
FHIR RxNorm page (<https://hl7.org/fhir/R4B/rxnorm.html>) are the authority.

- The files are pipe-delimited without a header; the columns are the
  documented positions of `RXNCONSO`, `RXNREL`, `RXNSAT`, and `RXNSTY`, read
  streaming (millions of rows) into typed rows.
- A relationship row states that the second concept has the relationship to
  the first (UMLS convention); the typed row keeps the file's columns and the
  build turns them into directed edges.
- The release itself is never committed; the "Current Prescribable Content"
  subset is licence-free, the full release needs a UMLS licence. Fixtures are
  shaped like the files with invented identifiers.
