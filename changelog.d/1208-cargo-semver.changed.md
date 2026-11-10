- The published crates (`rf2`, `concept-graph`, `sct-ecl`,
  `fhir-terminology` and the rest of the line) follow The Cargo Book's
  SemVer rules, judged by cargo-semver-checks 0.51.0 on every pull request
  and merge-queue entry that moves the line and again in the release plan
  against crates.io: a compatible change moves the patch, and a breaking
  change, a public dependency moved to another compatibility line included,
  moves the minor at patch 0, so a caret requirement such as `rf2 = "0.1"`
  never takes a break. Upgrade step: none.
