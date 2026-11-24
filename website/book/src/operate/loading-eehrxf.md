<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Loading the EEHRxF code systems

The European Electronic Health Record exchange Format (EEHRxF, the
implementing act under Article 15 of Regulation (EU) 2025/327) binds its
data elements to code systems beyond SNOMED CT, LOINC, and ICD-10. This page
covers the loaders for those systems. Each takes the release file its
publisher distributes, in the publisher's own format, and builds it into an
index the server reads like any other (`FERROTERM__TERMINOLOGY__INDEX`).

<!-- toc -->

## You supply every release

> [!WARNING]
> FerroTERM ships none of these releases. You download each from its
> publisher under the terms that publisher sets, and you hold any licence it
> requires. The tests in the repository run against small synthetic files in
> each format, never a real release.

Every build below writes one artifact directory. Without `--out`, the
build writes into a child of `--index-root` (or `FERROTERM_INDEX_ROOT`) named
in the section. The release's version is recorded as the `CodeSystem`
version, so `$lookup`, `$validate-code`, and `$expand` answer for the version
you loaded and refuse any other with an `OperationOutcome` that names it.

## NPU

The IFCC publishes the NPU code file, `npu-codes-latest.csv`, at
<https://npu-terminology.org/npu-database/>. The file carries no release
name, so give the month you downloaded it as the version:

```console
$ ferroterm-build --npu npu-codes-latest.csv --npu-version 2026-11 --out /path/to/npu
```

The system is `http://npu-terminology.org` (HL7 Terminology,
`NamingSystem-NPU`). NPU is a flat list: the short definition is the
display, the full definition a designation, and the system, component, kind
of property, unit, speciality, scale type, and replacement links are
properties. A row whose `active` column is `0` is served inactive. The
default child is `npu`.

## ICD-O-3

WHO publishes ICD-O-3 as a topography table and a morphology table; the
ICD-O-3.2 morphology workbook is distributed by the IACR for WHO
(<http://www.iacr.com.fr/index.php?option=com_content&view=article&id=149:icd-o-3-2&catid=80&Itemid=545>).
Give each table, as the workbook or its tab, comma, or semicolon export, with
the revision as the version:

```console
$ ferroterm-build --icdo Topoenglish.txt --icdo ICD-O-3.2_morphology.xlsx \
    --icdo-version 3.2 --out /path/to/icd-o-3
```

Each table needs a code column (`Kode`, `Code`, or `ICDO3.2`), a term column
(`Title`, `Term`, or `Label`), and may carry a level column (`Lvl` or
`Level`). A level of `3`, `4`, or `Preferred` names the title; `incl` and
`Synonym` name synonyms; `Related` names related terms. The system is
`http://terminology.hl7.org/CodeSystem/icd-o-3` (HL7 Terminology,
`NamingSystem-icd-o-3`). Topography codes carry their period (`C50.9`) and
nest in their three-character category; morphology codes carry the slash
before the behaviour digit (`8500/3`). The default child is `icd-o-3`.

## EMDN

The European Commission publishes the European Medical Device Nomenclature as
a workbook at <https://webgate.ec.europa.eu/dyna2/emdn/>, free of charge. The
release year is read from the sheet name (`EMDN_v.2026`):

```console
$ ferroterm-build --emdn "EMDN v2026_EN.xlsx" --out /path/to/emdn
```

Pass `--emdn-version` when the sheet does not name the year. HL7 Terminology
registers no URI for EMDN; the system is
`https://smt.esante.gouv.fr/terminologie-emdn`, the canonical of the one
published FHIR `CodeSystem` for it (`ans.fr.terminologies`). Each code nests in
the code its prefix names; the level and the terminal flag are properties.
The default child is `emdn`.

## ISCO-08

The ILO publishes the ISCO-08 structure as a workbook, `ISCO-08 EN Structure
and definitions.xlsx`
(<https://ilostat.ilo.org/methods/concepts-and-definitions/classification-occupation/>):

```console
$ ferroterm-build --isco "ISCO-08 EN Structure and definitions.xlsx" --out /path/to/isco-08
```

The version is `2008` unless you pass `--isco-version`. The system is
`urn:oid:2.16.840.1.113883.2.9.6.2.7` (HL7 Terminology,
`NamingSystem-ISCO08`). The four levels nest by code prefix; the definition,
the tasks, and the included and excluded occupations are properties. The
default child is `isco-08`.

## ORPHAcodes

Orphanet publishes its nomenclature through Orphadata as one XML file per
language, `<lang>_product1.xml`
(<https://www.orphadata.com/orphanet-nomenclature/>). Give the file of the
display language first and any other languages after it:

```console
$ ferroterm-build --orpha en_product1.xml --orpha fr_product1.xml --out /path/to/orphanet
```

The version is the date in the file's `JDBOR` element (`2026-06-23`) unless
you pass `--orpha-version`. The system is `https://www.orpha.net` (HL7
Terminology, `NamingSystem-orphanet`). The nomenclature is a flat list: the
preferred name is the display in each language, synonyms are designations,
and the disorder type, group, and flags are properties. An entity flagged
`Inactive` (obsolete, deprecated, or moved) is served inactive. The default
child is `orphanet`.

## DICOM

NEMA publishes the DICOM Controlled Terminology (PS3.16 Annex D, the `DCM`
coding scheme) as OWL with each edition, `dcm.owl.zip`
(<https://dicom.nema.org/medical/dicom/resources/ontology/>):

```console
$ ferroterm-build --dicom dcm.owl.zip --out /path/to/dicom
```

The version is the edition the ontology names (`2026e_20261121`) unless you
pass `--dicom-version`. The system is
`http://dicom.nema.org/resources/ontology/DCM` (HL7 Terminology,
`NamingSystem-v3-DCM`). The code meaning is the display; the definition and
notes are properties; a retired code is inactive, with its replacement as
`replaced-by`.

The PS3.16 context groups are published by NEMA as FHIR `ValueSet` resources,
one per CID, at
<https://dicom.nema.org/medical/dicom/resources/valuesets/fhir/json/>. Download
the ones you need into a directory and point
`FERROTERM__TERMINOLOGY__CODE_SYSTEMS` at it. Each group expands and
validates over the codes it includes: the `DCM` codes from this build, and
SNOMED CT, LOINC, or UCUM codes from those systems when you load them. The
DICOM Standard is free to implement, but NEMA holds the copyright of its text
and reproduction needs NEMA's permission, so FerroTERM ships neither the
terminology nor the context groups. The default child is `dicom`.

## EDQM Standard Terms

The EDQM distributes the Standard Terms database through its API, behind a
free account (<https://standardterms.edqm.eu>, "Help API"). Save the
response of `full_data_by_class` for all classes, or one response per class,
as JSON, and build them:

```console
$ ferroterm-build --edqm edqm-all.json --out /path/to/edqm
```

The database names no release number, so the version is the newest
modification date among the terms (`2026-10-14`) unless you pass
`--edqm-version`. HL7 Terminology registers no URI for EDQM; the system is
`http://standardterms.edqm.eu`, the one the HL7 Europe guides and the
International Patient Summary use. The English term is the display and each
translation a designation in its language; the class, domain, status,
definition, and the links to terms of other classes (`link-bdf`, `link-roa`,
and so on) are properties. A term whose status is not `Current` is served
inactive. The EDQM's terms of use apply to the data you load. The default
child is `edqm`.

## EMA SPOR

The EMA distributes its SPOR master data through the SPOR API, behind an
account you register with the EMA
(<https://www.ema.europa.eu/en/human-regulatory-overview/research-development/data-medicines-iso-idmp-standards-overview/substance-product-organisation-referential-spor-master-data>).
Save the responses and build each kind on its own; each RMS list is a code
system of its own, so build each list separately:

```console
$ ferroterm-build --spor rms-200000000004-terms.xml --out /path/to/spor-dose-forms
$ ferroterm-build --spor oms-organisations.json --out /path/to/spor-organisations
$ ferroterm-build --spor sms-substances.json --out /path/to/spor-substances
$ ferroterm-build --spor pms-products.json --out /path/to/spor-products
```

| Kind | Response | System |
|---|---|---|
| RMS terms of one list | SPOR API v1 XML (`controlled-term`) | `https://spor.ema.europa.eu/v1/lists/<list id>` |
| OMS organisations | SPOR API v1 JSON (`organisation-id`, `name`, `status`) | `https://spor.ema.europa.eu/v1/organisations` |
| SMS substances | SPOR API v2 FHIR `Bundle` of `SubstanceDefinition` | `https://spor.ema.europa.eu/v2/SubstanceDefinition` |
| PMS medicinal products | SPOR API v2 FHIR `Bundle` of `MedicinalProductDefinition` | `http://ema.europa.eu/fhir/MPID` |

HL7 Terminology registers none of these. The list URIs are those of the HL7
Europe guides, the substance URI that of the MyHealth@EU substance value set,
and the product URI the MPID identifier system of the SPOR API v2
specification. A term or organisation that is not current or active is
served inactive; a term's names in each language are designations. The
version is the newest modification date among the records unless you pass
`--spor-version`. Give several files of one kind to build them as one. The
default child is `spor`.

## SNOMED CT Global Patient Set

SNOMED International publishes the Global Patient Set (GPS) freeset as a
tab-separated file with the concept id, the active flag, the fully specified
name, and the US English preferred term
(<https://docs.snomed.org/implementation-guides/gps-implementation-guide/global-patient-set-content>).
The build writes it as a value set over SNOMED CT:

```console
$ ferroterm-build --gps SnomedINTL_GPSRelease_PRODUCTION_20260101T120000Z.txt --out /path/to/gps-root
```

The value set lands in `gps/` under the output, with the canonical
`https://ferroterm.eu/fhir/ValueSet/snomed-gps` (FerroTERM's own; the
freeset has none on the FHIR wire) and the release date in the file name as
its version (`--gps-version` overrides it). Point
`FERROTERM__TERMINOLOGY__CODE_SYSTEMS` at that directory beside a SNOMED CT
edition in `FERROTERM__TERMINOLOGY__INDEX`: the value set holds the active
concepts and expands and validates over whichever edition you load. If you
hold the GPS RF2 refset package instead, layer it onto the edition with
`ferroterm-build --rf2 <edition> --rf2-refset <package>`; it is then served
as `http://snomed.info/sct?fhir_vs=refset/<refset id>`.

## EHDS identifier types

The EEHRxF act (Annex X, `EHDSPatient.identifier.type`) binds the identifier
type to the HL7 identifier type extended with NI (national unique
identifier), HI (health identifier), HIN (health insurance number), SS
(social security identifier), REGPID (regional person identifier), and
REGION (region identifier). FerroTERM serves this binding on every FHIR
version with nothing to load:

- the value set `https://ferroterm.eu/fhir/ValueSet/ehds-identifier-type`,
  which includes the FHIR `identifier-type` value set, NI and SS from
  `http://terminology.hl7.org/CodeSystem/v2-0203`, and the four codes below;
- the code system `https://ferroterm.eu/fhir/CodeSystem/ehds-identifier-type`
  with HI, HIN, REGPID, and REGION, which the HL7 v2 table 0203 does not
  define.

Both are FerroTERM's own resources. The value set expands fully when HL7
Terminology (`hl7.terminology`, CC0) is loaded through
`FERROTERM__TERMINOLOGY__CODE_SYSTEMS`, since the HL7 identifier type and
table 0203 come from there.
