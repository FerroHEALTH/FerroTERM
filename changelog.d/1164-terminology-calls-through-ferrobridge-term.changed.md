- The console's `$lookup`, `$validate-code`, `$subsumes`, `$translate` and
  `$expand` requests and readings, and the sync service's revalidation, now
  go through FerroBRIDGE's `ferrobridge-term` client, which writes each
  input under the name the serving release's `OperationDefinition` gives it,
  in the order it declares them. The address the console shows for a run
  lists its parameters in that order, and an answer that leaves out a
  required output (`result`, `outcome`, or a `$lookup` `name` or `display`)
  is shown as an answer that did not read. The sync service takes the FHIR
  release it writes in from the last segment of `fhir_base_url` (`r4`,
  `r4b`, `r5` or `r6`), and reports the revalidation as not run when that
  segment names none.
