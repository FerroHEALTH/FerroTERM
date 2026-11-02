# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/FerroHEALTH/FerroPULSE> (private; Cadasto B.V., the Licensor of
  this repository too)
- Commit: f83df34cc968f5e0f916834508336c8eeb4c9e41
- Fetched: 2026-11-02 by `scripts/vendor/ferropulse-schemas.sh`
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
