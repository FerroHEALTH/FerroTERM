# Provenance: registry data

Vendored verbatim as the input of the registry code systems (BCP 47, BCP 13,
ISO 3166); never edit a file here, re-run `scripts/vendor/registries.sh`.

## IANA Language Subtag Registry (BCP 47)

- File: `iana/language-subtag-registry` (record-jar, RFC 5646 §3.1)
- Source: <https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry>
- Registry File-Date: 2026-09-17
- Fetched: 2026-11-02
- License: public domain (IANA protocol registries, <https://www.iana.org/help/licensing-terms>)

## IANA Media Types registry (BCP 13)

- Files: `iana/media-types/<type>.csv` for the ten top-level types
- Source: <https://www.iana.org/assignments/media-types/media-types.xhtml> (the per-type CSV exports)
- Fetched: 2026-11-02
- License: public domain (IANA protocol registries, <https://www.iana.org/help/licensing-terms>)

## Unicode CLDR (ISO 3166-1)

- Files: `cldr/codeMappings.json` (alpha-2 to alpha-3 and numeric), `cldr/territories.json` (English names), `cldr/LICENSE`
- Source: <https://github.com/unicode-org/cldr-json>, ref `49.0.0` at commit `a4b67247b446fd51b475991017a2481911210c60`
- CLDR version: 49
- Fetched: 2026-11-02
- License: Unicode License v3 (`cldr/LICENSE`)

## UCUM essence

- Files: `ucum/ucum-essence.xml` (the unit definitions), `ucum/LICENSE.md`
- Source: <https://github.com/ucum-org/ucum>, ref `ef4c31cd7d3bc81de1a1bf2cc8414bf502b6304f` at commit `ef4c31cd7d3bc81de1a1bf2cc8414bf502b6304f`
- UCUM version: 2.2
- Fetched: 2026-11-02
- License: the UCUM licence (`ucum/LICENSE.md`), verbatim redistribution with the notice
