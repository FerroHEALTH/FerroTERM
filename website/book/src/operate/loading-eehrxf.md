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
