- `fhir-terminology` 0.1.132 serves SNOMED CT, LOINC, `RxNorm`, ICD-11 and
  the classifications through one `stored::system::StoredProvider`, generic
  over the `StoredSystem` each system implements (`snomed::Snomed`,
  `loinc::Loinc`, `rxnorm::RxNorm`, `icd11::Icd11`,
  `classification::Classification`). `SnomedProvider`, `LoincProvider`,
  `RxNormProvider`, `Icd11Provider` and `ClassificationProvider` name its
  five instances, open as before, and answer every operation as before. The
  open failures every artifact shares move to `stored::OpenError`, which each
  system's `OpenError` carries as `Artifact`, and the five `MANIFEST_VERSION`
  constants become `stored::MANIFEST_VERSION`. The default bodies of
  `CodeSystemProvider::inactive` and `filter_matches` are
  `filter::inactive_by_status` and `filter::selected_by`.
