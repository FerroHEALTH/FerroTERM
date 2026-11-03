- The console reads each root's `CapabilityStatement` and
  `TerminologyCapabilities` through `fhir-types`, decoding at the release of
  the root that served it. A document that lacks a required element, or
  carries a member its release does not define, draws the screen's error
  state, which names the release, the resource and the element path, where
  the console used to draw whatever it could read.
