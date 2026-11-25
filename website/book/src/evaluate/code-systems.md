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
| SNOMED CT | `http://snomed.info/sct` | Any RF2 edition; the version is the edition URI (`http://snomed.info/sct/[module]/version/[date]`) | `ferroterm-build --rf2 <release zip or dir>` into `FERROTERM__TERMINOLOGY__INDEX` | Licensed by SNOMED International; free in member countries through the national release centre |
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
| Nederlandse Labcodeset | `https://ferroterm.eu/fhir/ValueSet/nl-labcodeset` and the LOINC supplement `https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc` | Any Nictiz publication, as a value set and supplement over LOINC, SNOMED CT, and UCUM | `ferroterm-build --labcodeset <zip, document, or dir>` into `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` | Licensed by Nictiz |
| NHG ICPC-1 to SNOMED CT map | `http://terminologie.nictiz.nl/ns/ConceptMap/icpc1nl-snomed` (source `http://hl7.org/fhir/sid/icpc-1-nl`) | Any Nictiz release of the FHIR `ConceptMap` | A directory with the `ConceptMap` and a `package.json`, in `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` | Licensed by the NHG |
| NPU | `http://npu-terminology.org` | Any IFCC code file; you name the release | `ferroterm-build --npu <npu-codes-latest.csv> --npu-version <release>` | Published by the IFCC and the Danish Health Data Authority; you download the release |
| ICD-O-3 | `http://terminology.hl7.org/CodeSystem/icd-o-3` | Any WHO revision (`3.2` style), topography and morphology | `ferroterm-build --icdo <topography> --icdo <morphology> --icdo-version <revision>` | Licensed by WHO; you supply the tables |
| EMDN | `https://smt.esante.gouv.fr/terminologie-emdn` | Any yearly Commission release (`2026` style) | `ferroterm-build --emdn <EMDN workbook>` | Free from the European Commission; you download the release |
| ISCO-08 | `urn:oid:2.16.840.1.113883.2.9.6.2.7` | The ILO structure workbook | `ferroterm-build --isco <ISCO-08 workbook>` | Published by the ILO; you download the release |
| ORPHAcodes | `https://www.orpha.net` | Any Orphadata nomenclature release, in the languages you load | `ferroterm-build --orpha <lang>_product1.xml` | Published by Orphanet (Orphadata); you download the release |
| DICOM Controlled Terminology (DCM) | `http://dicom.nema.org/resources/ontology/DCM` | Any edition (`2026e` style); the PS3.16 context groups as NEMA's FHIR value sets | `ferroterm-build --dicom <dcm.owl.zip>`, the context groups in `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` | Copyright NEMA; you download the release |
| EDQM Standard Terms | `http://standardterms.edqm.eu` | Any download from the Standard Terms API | `ferroterm-build --edqm <API response JSON>` | Under the EDQM terms of use; you download the data |
| EMA SPOR | `https://spor.ema.europa.eu/v1/lists/<list id>` (RMS), `https://spor.ema.europa.eu/v1/organisations` (OMS), `https://spor.ema.europa.eu/v2/SubstanceDefinition` (SMS), `http://ema.europa.eu/fhir/MPID` (PMS) | Any download from the SPOR API, one list or kind per build | `ferroterm-build --spor <API response>` | Under the EMA's SPOR terms; you download the data |
| SNOMED CT Global Patient Set | `https://ferroterm.eu/fhir/ValueSet/snomed-gps` over `http://snomed.info/sct` | Any GPS freeset release, over the edition you load | `ferroterm-build --gps <freeset .txt>` into `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` | CC BY-ND 4.0 from SNOMED International; you download the release |
| UDI device identifiers | `http://hl7.org/fhir/NamingSystem/gs1-di`, `…/hibcc-diI`, `…/iccbba-blood-di`, `…/iccbba-other-di`, `…/ifa-gmbh-di` | Any export you make, one issuing entity per build | `ferroterm-build --udi <device table> --udi-issuer <issuer> --udi-version <date>` | Under the terms of the database you export from; you supply the records |
| EHDS identifier types | `https://ferroterm.eu/fhir/ValueSet/ehds-identifier-type` and `https://ferroterm.eu/fhir/CodeSystem/ehds-identifier-type` | The Annex X binding of the EEHRxF act, built in | Nothing to load; HL7 Terminology completes the expansion | FerroTERM's own |
| FHIR `CodeSystem`, `ValueSet`, and `ConceptMap` resources | Each resource's own `url` | HL7 Terminology and any FHIR package or directory of JSON, supplements included | `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` | Per resource; HL7 Terminology is CC0 |
<!-- code-systems:end -->

Every system reaches the operations through one provider seam, so nothing in
an operation is a special case for one system. The repository ships no code
system content, licensed or open: you bring the release you are licensed for,
and the build tool turns it into an index. The
[loading page](../operate/loading-snomed.md) shows each build, and the
[EEHRxF page](../operate/loading-eehrxf.md) the systems the European exchange
format binds.
