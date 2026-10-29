- Every error FerroTERM answers outside its FHIR bases is now an RFC 9457
  problem, `application/problem+json`, with the status codes unchanged: the
  server's admin listener (`POST /reload` and its refusals), a path of the
  FHIR listener that names no FHIR base, the sync service's admin listener,
  and the refusals the HTTP stack writes itself on those routes. Each
  FerroTERM problem type links to its section of the book's new Problem
  types page, and a problem quotes nothing from the request. Under `/r4`,
  `/r4b`, `/r5` and `/r6` errors stay FHIR `OperationOutcome`s. The console
  draws an admin route's problem as its issue.
