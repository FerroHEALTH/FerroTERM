- The server serves `NamingSystem` read and search on every version: it keeps
  the `NamingSystem` resources of each package directory beside its
  terminology (`ferroterm-build --fhir-package` imports them too), stores
  written ones with a resource database, and answers the search parameters
  `kind`, `value`, `type`, `id-type` and `status`, so a client resolves an
  OID, a URI or a local name such as an HL7 v2 assigning authority to its
  namespace. The capability statement declares the resource.
