# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/FerroHEALTH/FerroPULSE> (private; Cadasto B.V., the Licensor of
  this repository too)
- Commit: 095ccb5718a1b5b0adb088f4d8cb143ba3f5f101
- Fetched: 2026-10-16 by `scripts/vendor/ferropulse-schemas.sh`
- Licence: upstream declares `LicenseRef-Proprietary`; Cadasto B.V. holds the
  copyright of both repositories and publishes this copy under this
  repository's BUSL-1.1
- Contents: `schemas/report-envelope.v2.json` (JSON Schema 2020-12) and
  `schemas/examples/report-envelope.v2/` (the contract's own valid and
  invalid examples), copied verbatim

The usage report tests in `app/ferroterm-server/tests/it/usage_report.rs`
check the validator against the examples, then validate every report the
server renders against the schema. Never hand-edit these files; change the pin
in `docs/VERSIONS.md` and re-run the script.
