- FerroTERM runs on `ferrohealth-server` 0.4.1, which keeps a product's own
  fallback and the layers around it when the stack merges its routes
  (FerroHEALTH/FerroSYS#199). FerroTERM was not exposed to the regression
  0.4.1 fixes: the FHIR listener, the admin listener and the `ferroterm-sync`
  admin listener each declare their own fallback inside their layers, so a
  path no route matches was already answered there, and on the admin listener
  an anonymous request to such a path is refused with `401` before and after.
