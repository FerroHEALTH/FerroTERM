<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Code systems served

One table of every code system FerroTERM serves: the canonical URI each is
served under, the versions and editions handled, how you bring its content,
and its licence position. This page is the single source; the README and the
landing page carry the same names, and a check in CI fails when a row here is
missing from either. The design notes per system (identity rules, filters,
properties, implicit value sets, with their citations) are in
`docs/terminologies.md` in the repository.

<!-- code-systems:begin -->
| Code system | Canonical URI | Versions and editions | How you load it | Licence |
|---|---|---|---|---|
| SNOMED CT | `http://snomed.info/sct` | Any RF2 edition; the version is the edition URI (`http://snomed.info/sct/[module]/version/[date]`) | `ferroterm-build --rf2 <release zip or dir>` into `FERROTERM_INDEX` | Licensed by SNOMED International; free in member countries through the national release centre |
| LOINC | `http://loinc.org` | Any release (`2.82` style), with its linguistic variants | `ferroterm-build --loinc <Loinc_x.yy.zip or dir>` | Free under the LOINC licence |
| UCUM | `http://unitsofmeasure.org` | The `ucum-essence.xml` grammar vendored in the binary | Nothing to load | Free (UCUM licence) |
| BCP 47 | `urn:ietf:bcp:47` | The IANA Language Subtag Registry vendored in the binary | Nothing to load | Open (IETF) |
| BCP 13 | `urn:ietf:bcp:13` | The RFC 6838 grammar and the IANA media type registry, vendored | Nothing to load | Open (IETF) |
| ISO 3166-1 | `urn:iso:std:iso:3166` | The country codes from Unicode CLDR, vendored | Nothing to load | Open (CLDR) |
| ICD-10 (WHO) | `http://hl7.org/fhir/sid/icd-10` | Any ClaML release (`2019` style) | `ferroterm-build --claml <xml or zip> --system http://hl7.org/fhir/sid/icd-10` | Licensed by WHO; national translations by their publishers |
| ICD-10-NL | `http://hl7.org/fhir/sid/icd-10-nl` | The Dutch ClaML release (WHO-FIC NL) | `ferroterm-build --claml <xml or zip> --system http://hl7.org/fhir/sid/icd-10-nl` | Licensed by WHO-FIC NL |
| ICD-10-CM | `http://hl7.org/fhir/sid/icd-10-cm` | Any CMS fiscal-year release (tabular and order files) | `ferroterm-build --icd10cm <dir or zip>` | Public domain (US) |
| ICD-11 MMS | `http://id.who.int/icd/release/11/mms` | Any WHO release (`2026-01` style), in the languages fetched | `ferroterm-build --icd11 <cache> --icd11-api <local ICD-API>` | CC BY-ND 3.0 IGO |
| ICD-11 ICF | `http://id.who.int/icd/release/11/icf` | The same WHO release | The same `--icd11` build | CC BY-ND 3.0 IGO |
| ICD-11 Foundation | `http://id.who.int/icd/entity` | The same WHO release | The same `--icd11` build | CC BY-ND 3.0 IGO |
| ATC/DDD | `http://www.whocc.no/atc` | Any annual index (`2026` style) | `ferroterm-build --atc <index.csv or BST801T> --atc-version <year>` | Purchased from the WHO Collaborating Centre |
| ICPC-2 | `http://hl7.org/fhir/sid/icpc-2` | Any ClaML release | `ferroterm-build --claml <xml or zip> --system http://hl7.org/fhir/sid/icpc-2` | Licensed by WONCA |
| RxNorm | `http://www.nlm.nih.gov/research/umls/rxnorm` | Any monthly release, or the licence-free prescribable subset | `ferroterm-build --rxnorm <RRF zip or dir>` | UMLS licence for the full release; the prescribable subset is free |
| DHD Diagnosethesaurus and Verrichtingenthesaurus | `urn:oid:2.16.840.1.213883.2.4.3.120.5.1` | Any Uitleverformaat 5.0 delivery, with its SNOMED CT and ICD-10 concept maps | `ferroterm-build --dhd <delivery zip or dir>` | Licensed by DHD |
| G-Standaard | `urn:oid:2.16.840.1.213883.2.4.4.1` (GPK), `.10` (PRK), `.7` (HPK), `.8` (article) | Any monthly release | `ferroterm-build --gstandaard <dir> --gstandaard-version <release>` | Z-Index subscription |
| Nederlandse Labcodeset | `https://ferroterm.eu/fhir/ValueSet/nl-labcodeset` and the LOINC supplement `https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc` | Any Nictiz publication, as a value set and supplement over LOINC, SNOMED CT, and UCUM | `ferroterm-build --labcodeset <zip, document, or dir>` into `FERROTERM_CODESYSTEMS` | Licensed by Nictiz |
| NHG ICPC-1 to SNOMED CT map | `http://terminologie.nictiz.nl/ns/ConceptMap/icpc1nl-snomed` (source `http://hl7.org/fhir/sid/icpc-1-nl`) | Any Nictiz release of the FHIR `ConceptMap` | A directory with the `ConceptMap` and a `package.json`, in `FERROTERM_CODESYSTEMS` | Licensed by the NHG |
| FHIR `CodeSystem`, `ValueSet`, and `ConceptMap` resources | Each resource's own `url` | HL7 Terminology and any FHIR package or directory of JSON, supplements included | `FERROTERM_CODESYSTEMS` | Per resource; HL7 Terminology is CC0 |
<!-- code-systems:end -->

Every system reaches the operations through one provider seam, so nothing in
an operation is a special case for one system. The repository ships no code
system content, licensed or open: you bring the release you are licensed for,
and the build tool turns it into an index. The
[loading page](../operate/loading-snomed.md) shows each build.
