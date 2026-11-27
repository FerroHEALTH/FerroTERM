- The README shows a badge for each tx-ecosystem mode run by hand (`snomed`,
  `icd-11`, `icf`, `loinc`, `tx.fhir.org`, `mimetypes`, `omop`), with its pass
  count, the cases it ran and the day it was measured, and one naming the FHIR
  versions served. The badges come from the committed pass lists and
  `conformance/tx-ecosystem/hand-run.tsv`, and a CI check fails when the
  README's badges or the pass-list table disagree with them.
