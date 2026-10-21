<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The differential check against a reference server

`requests.json` is the sample both servers answer: SNOMED CT identifiers, an
operation, and the projection to diff, and nothing else. It carries no term, no
definition, and no hierarchy, so it distributes no SNOMED CT content
(`.claude/rules/vendored-inputs.md`).

`scripts/checks/differential.sh` replays the sample against FerroTERM and
against the reference server over the same licensed edition and diffs one
projection per answer (`display`, `result`, `outcome`, `codes`, `targets`),
because the two servers return the same answer in different shapes. Where the
reference server and the specification disagree the specification wins, and the
divergence is recorded (`.claude/rules/testing.md`).

Before replaying anything, the harness reads the version URI each server
reports. That URI names the edition's module and the release date
(<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard/2-snomed-ct-uri-space>).
Two servers on different release dates hold different concepts, so nothing
after that would compare behaviour and the run stops. The module is an answer
the two servers compute, so a difference there is a divergence like any other
and is recorded, not a reason to stop.

Run it over a locally built index and a reference server you already have:

```bash
cargo build --release -p ferroterm-server
scripts/checks/differential.sh --index artifacts/int --snowstorm http://127.0.0.1:8080/fhir
```

The harness never starts the reference server. Snowstorm proper wants
Elasticsearch and 16 to 32 GB of RAM; Snowstorm Lite runs in one container on
about 500 MB once its index is built, and `runs/` records how to bring it up.

`.github/workflows/differential.yml` runs the same script weekly and files an
issue on divergence. It skips itself until two repository secrets are set:
`SNOMED_RF2_URL`, one licensed RF2 release zip the runner can fetch, and
`SNOWSTORM_URL`, the FHIR base of a reference server loaded with that same
release. Each run uploads a `differential-report` artifact holding every answer
and the divergences. Those runs are the recorded evidence behind the manual
differential criterion on issue #39; issues #15 and #19 name the Nictiz
Nationale Terminologieserver instead, and #247 runs this same harness against
it.

## Recorded runs

Each file under `runs/` is one pass: what was compared, the version of each
server, the release both served, the count per operation, and the adjudication
of every divergence against the specification.

| Run | Reference server | Edition | Result |
|---|---|---|---|
| [2026-09-05](runs/2026-09-05-snowstorm-lite-int-20260901.md) | Snowstorm Lite 2.5.2 | International 20260901 | 23 of 26 agree; 2 FerroTERM defects (#359, #368), 1 Snowstorm Lite gap |
