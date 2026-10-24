<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Code systems

FerroTERM serves more than one code system through one engine (`architecture.md`
§5). What is served, with each system's URI, versions, build command, and
licence position, is the table in `website/book/src/evaluate/code-systems.md`
(published at <https://ferroterm.eu/docs/evaluate/code-systems.html>), the
single source the README and the landing page follow. This file holds the
design notes: for each code system, the identity FHIR gives it, how its
content is distributed and licensed, what hierarchy it has, what the FHIR
specification defines for it (filters, properties, implicit value sets), and
where it sits in the build order. Every fact carries its source; a fact
marked "unverified" was read from a search result or an archived page, not the
live source, and is re-checked when the provider is built.

Two rules apply to every system:

- **Content is never committed.** The repository ships no code system content,
  licensed or open. A deployment brings its own release; test fixtures are
  synthetic unless the section below says real codes are permitted and how.
- **The FHIR page is the contract.** What a provider filters on, which
  properties `$lookup` returns, and which implicit value sets it parses is what
  the FHIR specification defines for that system, nothing more and nothing
  less (`.claude/rules/spec-adherence.md`). For R4B that is the per-system page
  in the specification; for R5 the specification delegates to the HL7
  Terminology "Using X with HL7 Standards" pages
  (<https://hl7.org/fhir/R5/terminologies-systems.html#external>).

## Build order

The order weighs the tx-ecosystem test suite (what a conformant server must
pass), the Dutch deployment context, and how much each system exercises the
engine. Each row is a tracker issue under the program issue.

| Order | Code system | Why here | Provider shape |
|---|---|---|---|
| 1 | SNOMED CT | The hardest case: polyhierarchy, ECL, refsets, editions. Shapes the engine. Licensed data available for development. | `rf2` loader, ECL, SNOMED implicit forms |
| 2 | FHIR `CodeSystem` resources (HL7 Terminology, custom systems, supplements) | Passes the tx-ecosystem `general` mode on its own (667 of 1,174 tests use synthetic FHIR code systems); carries every code system published as a FHIR resource. | Generic provider over the FHIR package loader (`crates/fhir-terminology/src/fhir_codesystem/load.rs`) |
| 3 | LOINC | Second most exercised system in the suite (2,152 references); the Dutch lab code set builds on it; the nl-NL linguistic variant exists. | CSV loader; parts hierarchy; the LOINC filters and `/vs/` implicit sets |
| 4 | UCUM | 2,267 references in the suite; grammar-defined, so it proves the non-enumerable provider shape. | Expression parser over `ucum-essence.xml`; no store |
| 5 | ICD-10 (WHO), ICD-10-NL, ICD-10-CM | The Dutch diagnosis classification; `classified-with` hierarchy proves the mono-hierarchy shape. | ClaML loader (WHO, NL), tab-delimited loader (CM) |
| 6 | BCP 47, BCP 13, ISO 3166 | Registry and grammar systems the suite exercises (500, 62, and 790 references). | Grammar or table providers; BCP 47 and BCP 13 enumerate only a bounded selection |
| 7 | RxNorm | Typed drug relationships as filters; US context, open subset. | RRF loader (prescribable subset) |
| 8 | ATC, ICPC, ICD-11 | Classification systems with one parent per code; ICD-11 needs the WHO API or local deployment. | Table and ClaML loaders; API export for ICD-11 |
| 9 | The Dutch national code systems (DHD thesauri, G-Standaard, Labcodeset) | Licence-gated deployment demand; each rides on the SNOMED, LOINC, and UCUM providers or a table loader. | Per section |
| later | CPT, CVX, NDC, MDC, UNII, NCI Metathesaurus | US registries with thin FHIR definitions; each is a table provider when a deployment needs it. | Table providers |

## SNOMED CT

- **Identity.** System `http://snomed.info/sct`. Version is the edition URI
  `http://snomed.info/sct/[sctid]/version/[YYYYMMDD]`, at minimum
  `http://snomed.info/sct/[sctid]`; a date without an edition SHOULD be refused
  (<https://hl7.org/fhir/R4B/snomedct.html>, §Versions). Codes are concept ids
  or compositional-grammar expressions; description ids and terms are not
  codes. The grammar SNOMED CT defines is `CodeSystem.compositional`; this
  server evaluates no expression, so
  `TerminologyCapabilities.codeSystem.version.compositional` is false and an
  expression is refused as a grammar the server does not serve.
- **Distribution and licence.** RF2 release files from SNOMED International or
  a national release centre (the Netherlands: Nictiz), under the SNOMED CT
  Affiliate Licence or a member-country licence
  (<https://mlds.ihtsdotools.org>). Content is never committed; fixtures are
  synthetic (`.claude/rules/vendored-inputs.md`).
- **Hierarchy.** Inferred is-a polyhierarchy plus typed attributes; the
  transitive closure is computed offline (`architecture.md` §Offline).
- **FHIR-defined behaviour.** Filters `concept is-a`, `concept in` (refset
  membership), `constraint =` (ECL), `expressions =` (post-coordination
  allowed), and in the R5-era page `concept descendant-of`
  (<https://terminology.hl7.org/SNOMEDCT.html>). Implicit value sets `?fhir_vs`,
  `?fhir_vs=isa/[sctid]`, `?fhir_vs=refset`, `?fhir_vs=refset/[sctid]`,
  `?fhir_vs=ecl/[ecl]`; implicit concept maps `?fhir_cm=[sctid]`. `$lookup`
  properties `inactive`, `sufficientlyDefined`, `moduleId`, `normalForm`,
  `normalFormTerse`, plus `effectiveTime` and `semanticTag` in the R5-era page,
  and every concept-model attribute by concept id. The rule file is
  `.claude/rules/snomed-terminology.md`.
- **Size.** International edition: about 360,000 concepts, 1.2 million
  descriptions, 1.5 million relationships.
- **Served.** `ferroterm-build --rf2 <release zip|dir> [--rf2-refset <package>]…
  --out <dir>` builds the Snapshot into the artifact layout, layering any
  derivative reference set package onto the edition it depends on, and beside
  it the reference set
  memberships of every concept-referencing reference set (`refsets.bin`), the
  attribute relationships with their role groups and concrete values
  (`attributes.bin`), the active member rows of those reference sets with
  their fields (`members.bin`, the OWL axiom reference sets left out), and the
  alternate identifiers (`identifiers.bin`), which the ECL evaluator reads. The
  provider
  answers the implicit value sets `?fhir_vs`, `?fhir_vs=isa/[sctid]`,
  `?fhir_vs=refset`, and `?fhir_vs=refset/[sctid]` on the system, edition, or
  version URI, and the `concept is-a`, `descendent-of`, and `in` (reference
  set membership) filters, `?fhir_vs=ecl/[ecl]` and the `constraint` filter
  through the ECL evaluator (`crates/sct-ecl`), and the implicit concept maps
  `?fhir_cm=[sctid]` over the association and map reference sets.

## FHIR `CodeSystem` resources (HL7 Terminology and custom systems)

- **Identity.** Each resource's `url` and `version`. HL7 v2 tables are
  `http://terminology.hl7.org/CodeSystem/v2-[table]`, v3 systems
  `.../v3-[Name]`; both are case sensitive
  (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
- **Distribution and licence.** HL7 Terminology (THO) is the FHIR package
  `hl7.terminology` (7.3.0 pinned and vendored; R5-based since 6.0.0, the R4
  rendering is `hl7.terminology.r4`), CC0-1.0
  (<https://terminology.hl7.org/license.html>). Custom code systems arrive as
  FHIR `CodeSystem` JSON or an npm package. Real THO codes may appear in
  fixtures. THO 7.3.0 holds 928 code systems, 2,533 value sets, 675 naming
  systems, and 20,379 concepts; 611 code systems declare `hierarchyMeaning =
  is-a`, and designations exist in nine languages (Dutch coverage is thin:
  50 designations).
- **Hierarchy.** As declared: nested `concept` children, or a `subsumedBy` /
  `parent` property; subsumption applies only when `hierarchyMeaning` is
  `is-a` (<https://hl7.org/fhir/R4B/codesystem.html>).
- **FHIR-defined behaviour.** The generic filters on `concept` and `code` (`=`,
  `is-a`, `descendent-of`, `is-not-a`, `regex`, `in`, `not-in`, `generalizes`,
  `exists`; R5 adds `child-of` and `descendent-leaf`) and the resource's own
  `property` and `filter` declarations; `caseSensitive`, `content`,
  `versionNeeded`, `compositional` honoured; supplements add designations and
  properties only. This provider alone passes the tx-ecosystem `general` mode
  (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/testcases.html>).
- **Loading.** `FERROTERM_CODESYSTEMS` names the directories; every
  `CodeSystem` in them is served, the version from the package manifest,
  and a supplement is layered over the system it names.

## openEHR archetype terminology

An openEHR archetype constrains most of its coded fields with an
archetype-local `at`-code list, and nothing publishes those lists as FHIR
resources, so no terminology server can address them. FerroTERM serves them,
and it does so **without reading an openEHR artefact**.

- **The scope decision (#445).** This server ingests the FHIR resources a
  producer derives from an archetype, and does not read operational templates
  or archetypes itself. The alternative would give a FHIR/SNOMED project an
  openEHR dependency it does not have (`docs/VERSIONS.md`), and it would buy
  nothing: the producer already holds the archetype, and the derivation is a
  mapping onto resources this server already serves. FerroFORMS (formerly FerroCHART) is the
  producer side (`docs/architecture.md` §7.2 in that repository).
- **The shape.** One archetype's `at`-codes become a `CodeSystem` whose
  concepts carry the rubric as the display and the description as the
  definition, each language a designation; an `ac`-code or an inline code list
  becomes a `ValueSet` over it; `term_bindings` become a `ConceptMap` onto the
  external system each binds to.
- **Identity.** **No openEHR specification defines a canonical URI for an
  archetype's local terminology.** The archetype id is globally unique (the
  Archetype Object Model 2 specification, §3.2 Archetype Identification), so
  whoever produces the resources mints the URL from it under a domain they
  own. This server reads that URL as an opaque canonical and attaches no
  meaning to its shape.
- **Resolution order, and why a minted URL cannot collide with a release.** A
  code system is identified by its canonical, so two producers of one archetype
  mint two canonicals under their own domains and both are served side by side.
  A minted URL cannot collide with a loaded release because a release's systems
  carry the canonicals their publishers own (`http://snomed.info/sct`,
  `http://loinc.org`), and a producer mints under a domain it owns. Where two
  directories do name one canonical at one version, the load refuses rather
  than picking a winner, so a collision is reported and never silently
  resolved.
- **Loading.** `FERROTERM_CODESYSTEMS`, the same directories as any other
  supplied resource. Nothing about the request shape is openEHR-specific: the
  ordinary `$lookup`, `$expand`, `$validate-code`, and `$translate` answer over
  them (`app/ferroterm-server/tests/it/openehr.rs`).

## LOINC


- **Identity.** System `http://loinc.org`, version `2.82` style; codes
  `nnnnn-n` with a mod-10 check digit, part codes `LP…`, answer list codes
  `LL…`, answer codes `LA…`, all in the same system and not case sensitive;
  display is `SHORTNAME` or `LONG_COMMON_NAME`; inactive means `STATUS =
  DEPRECATED` (<https://terminology.hl7.org/LOINC.html>).
- **Distribution and licence.** One archive (`Loinc_2.82.zip`, 80 MB) from
  <https://loinc.org/downloads/> behind a free account, released every
  February and August, monthly from 2027 (unverified). The licence grants use
  and redistribution in perpetuity at no cost, with the LOINC copyright notice
  and version, each code kept with a LOINC display name, no changes to field
  names or contents, and third-party-copyright terms either licensed or removed
  (<https://loinc.org/kb/license/>). Real LOINC codes may appear in fixtures
  under those conditions; content is still never committed as a release.
- **Format.** CSV: `LoincTable/Loinc.csv` (the term table, 40 columns
  including the six axes `COMPONENT`, `PROPERTY`, `TIME_ASPCT`, `SYSTEM`,
  `SCALE_TYP`, `METHOD_TYP`, plus `CLASS`, `STATUS`, `CLASSTYPE`, `ORDER_OBS`,
  `SHORTNAME`, `LONG_COMMON_NAME`), `MapTo.csv` (replacements),
  `PartFile/Part.csv` and `LoincPartLink_Primary.csv` (over a million rows in
  the supplementary file), `ComponentHierarchyBySystem/…csv` (`PATH_TO_ROOT`,
  `IMMEDIATE_PARENT`, `CODE`), `AnswerFile/AnswerList.csv` and
  `LoincAnswerListLink.csv`, `LinguisticVariants/nlNL22LinguisticVariant.csv`
  (the Dutch translation by the NVKC), and `loinc.xml`, LOINC's own FHIR
  `CodeSystem` definition (<https://loinc.org/kb/users-guide/loinc-database-structure/>).
- **Hierarchy.** The Component Hierarchy by System: parts are branches, terms
  are leaves, and a term may sit under more than one path. THO treats it as the
  basis for subsumption; tx.fhir.org's provider returns no is-a for LOINC. FerroTERM
  materializes the part hierarchy as a closure and answers `parent` and
  `ancestor` filters from it; whether `$subsumes` between two LOINC terms is
  ever `subsumes` is decided against the LOINC CodeSystem's
  `hierarchyMeaning = is-a` when the provider is built.
- **FHIR-defined behaviour.** Filters: any `Loinc.csv` field with `=` or
  `regex`; `copyright = LOINC | 3rdParty`; `parent` and `ancestor` with `=` or
  `in` taking part codes. Implicit value sets `http://loinc.org/vs`,
  `http://loinc.org/vs/[LL id]` (an answer list), `http://loinc.org/vs/[part
  code]` (everything under a part). `$lookup` properties `STATUS`, the six axes
  as codes, `CLASS`, `CONSUMER_NAME`, `CLASSTYPE`, `ORDER_OBS`,
  `DOCUMENT_SECTION`, and the version. Translations are designations.
- **Served.** `ferroterm-build --loinc <Loinc_x.yy.zip|dir> --out <dir>` reads
  the release by column name (`Loinc.csv`, `Part.csv`, the Component
  Hierarchy by System, the primary part links, the answer lists and links,
  every linguistic variant) into the same artifact layout as SNOMED CT: terms,
  parts (the class parts only the hierarchy names among them, with its text as
  their name), answer lists, and answers are the codes (compared without case),
  the multiaxial hierarchy is the graph, every table field is a property and a
  filter (`=`, `regex`), the six axes carry their linked part as the value
  (the page types them as `Coding`) and match a filter by the part's code or
  name, and the names and translations are designations indexed for search. The
  provider answers `copyright`, `parent`, and `ancestor`, the three implicit
  value set forms, and `LONG_COMMON_NAME` as the display (a translation when
  the language asks for one); `STATUS = DEPRECATED` is inactive. The server
  opens a LOINC artifact from `FERROTERM_INDEX` like an edition; the manifest
  says which system a directory serves.
- **Size.** 108,248 terms in 2.81 (96,241 active).

## UCUM

- **Identity.** System `http://unitsofmeasure.org`, version `2.2`; "there is
  no need to use version in the Coding data type". A code is any valid
  case-sensitive UCUM expression; the code is its own display; curly-brace
  annotations are discouraged (<https://terminology.hl7.org/UCUM.html>).
- **Distribution and licence.** `ucum-essence.xml` (83 KB: 24 prefixes, 7 base
  units, 305 units) from <https://github.com/ucum-org/ucum>, royalty-free with
  no registration, no modification of content
  (<https://ucum.org/license>). It may be vendored verbatim with its notice.
- **Hierarchy.** None. Validation is a grammar and dimensional-analysis
  problem: the expression syntax "generates an infinite number of codes"
  (<https://ucum.org/ucum>), so the provider parses, canonicalizes, and refuses
  enumeration. `octofhir-ucum` (Apache-2.0, tested against the official
  UCUM suite) is the shortlisted dependency for the parser; the choice is made
  when the provider is built.
- **Served.** The provider parses expressions against the RFC-style grammar
  of the specification over the vendored `ucum-essence.xml` (2.2), reduces
  them to a magnitude over the seven base units, and interns each valid
  expression as its own code (case sensitive, the expression is its display,
  an English name composed from the essence as a designation). `$subsumes`
  answers `equivalent` for the same unit (dimensions, magnitude, special
  function) and `not-subsumed` otherwise; `canonical` and `property` are
  answered as properties and filters; enumeration is refused.
- **FHIR-defined behaviour.** Filters `property =` (base-unit property of the
  canonical form) and `canonical = | in` (comparable expressions). Implicit
  value sets `http://unitsofmeasure.org/vs` and
  `http://unitsofmeasure.org/vs/[expression]`. No lookup properties. A server
  not supporting the whole grammar says so in
  `TerminologyCapabilities.codeSystem.version.compositional`.

## ICD-10 (WHO) and ICD-10-NL

- **Identity.** WHO ICD-10 is `http://hl7.org/fhir/sid/icd-10` (OID
  2.16.840.1.113883.6.3); the Dutch translation is
  `http://hl7.org/fhir/sid/icd-10-nl` (OID 2.16.840.1.113883.6.3.2); ICD-10-CM
  is `http://hl7.org/fhir/sid/icd-10-cm`. Codes carry the period (`J21.8`);
  dual coding is space-separated (`J21.8 B95.6`, or with dagger and asterisk);
  the version is the year (<https://hl7.org/fhir/R4B/icd.html>,
  <https://terminology.hl7.org/ICD.html>). The zib `ProbleemNaamCodelijst`
  value sets use the `icd-10-nl` system.
- **Distribution and licence.** WHO licenses ICD-10 per use class
  (commercial, internal, non-commercial), with no modification of codes or
  titles; the machine form is ClaML XML behind a WHO account (unverified: the
  download host refused connections), with a CSV of about 14,000 codes and
  titles for licensees, and the ICD API serving the 2019, 2016, and 2010
  releases (<https://icd.who.int/docs/icd-api/SupportedClassifications/>).
  Version 2019 ended WHO's regular update cycle. The Dutch translation "ICD-10
  2021" comes from the WHO-FIC Collaborating Centre at RIVM as ClaML under a
  user licence that forbids passing the file on
  (<https://www.whofic.nl/familie-van-internationale-classificaties/referentie-classificaties/icd-10>).
  ICD-10-CM is a free download from NCHS: PDF, XML, and tab-delimited code
  files per fiscal year, 74,719 codes in FY2026, without periods in the file
  (<https://www.cdc.gov/nchs/icd/icd-10-cm/files.html>). Content is never
  committed; fixtures are synthetic.
- **Hierarchy.** Chapter, block, three-character category, four-character
  subcategory, each code with one parent: FHIR's `classified-with`
  (<https://terminology.hl7.org/ICD.html>). Inclusion and exclusion notes are
  content a `$lookup` can expose as properties.
- **FHIR-defined behaviour.** No filters and no implicit value sets are
  defined ("No need ... identified yet"), so the provider offers the generic
  filters only, with `is-a` and `descendent-of` over the classification tree.
- **Served.** One reader for ClaML (`ferroterm-build --claml <xml|zip> --system
  <uri> [--claml-version <v>]`, WHO ICD-10, ICD-10-NL, and any other ClaML
  classification) and one for the NCHS release (`ferroterm-build --icd10cm
  <dir|zip> [--icd10cm <dir|zip>]`, the tabular XML and the order file across
  the given paths) produce the same artifact layout as SNOMED CT and LOINC.
  Chapters, blocks, categories, and subcategories are the codes, with the
  period; the single-parent tree is the graph (`classified-with`); titles,
  inclusion terms, and the ICD-10-CM short descriptions are designations
  indexed for search; every other rubric or note kind (`exclusion`,
  `excludes1`, `codeFirst`, ...) is a property and a filter beside `kind`,
  `usage` (dagger, asterisk), and `valid` (the order file's header flag).
  ClaML modifiers expand onto the leaves they apply to (`S02.0` with the
  open/closed modifier gives `S02.00` and `S02.01`); the seventh-character
  codes of ICD-10-CM hang under their stem. The provider answers the generic
  filters over the tree, `$subsumes` from the closure, and the title in the
  requested language as the display. The Nictiz terminology server serves
  ICD-10 licence-free, which makes it the reference for the Dutch variant.


## ICD-11

- **Identity.** MMS linearization `http://id.who.int/icd/release/11/mms`;
  release ids are year and month (`2026-01`); foundation entities have their
  own URIs (<https://terminology.hl7.org/CodeSystem-ICD11MMS.html>,
  <https://icd.who.int/docs/icd-api/APIDoc-Version2/>).
- **Distribution and licence.** CC BY-ND 3.0 IGO: software may embed the
  classification if code, title, and URI travel together; translations and
  maps need a separate WHO agreement (<https://icd.who.int/en/docs/ICD11-license.pdf>).
  There is no ClaML; content comes through the ICD API (OAuth2, JSON-LD) or
  WHO's local deployment (Docker, no authentication), plus spreadsheet exports.
  Annual releases with fourteen MMS languages in 2026-01; a Dutch dataset is
  covered by the WHO-FIC NL terms, availability unverified.
- **Hierarchy and shape.** 24 disease chapters plus special-purpose,
  traditional-medicine, functioning, and extension-code chapters; stem codes,
  extension codes, and postcoordination clusters; about 17,000 categories and
  over 100,000 index terms (<https://icd.who.int/en/docs/icd11factsheet_en.pdf>).
- **FHIR-defined behaviour.** None beyond identity. The tx-ecosystem suite has
  an `icd-11` mode (50 tests) that a provider should pass.
- **Served.** `ferroterm-build --icd11 <cache> --icd11-api http://127.0.0.1:80
  [--icd11-release 2026-01] [--icd11-languages en,fr]` walks a local
  deployment of the ICD-API (`docker run -e acceptLicense=true -e
  include=2026-01_en whoicd/icd-api`, no authentication) into a cache of the
  entity JSON it serves (the MMS from 29 calls for the ids and one per entity
  and language) and builds `<out>/mms`, `<out>/icf`, and `<out>/entity`: three
  artifacts, one per code system, each entity a concept keyed by its short
  code or, without one, its URI, with a key table for the URI forms, the
  parent edges as the graph (the Foundation a polyhierarchy), titles, fully
  specified names, inclusions, and index terms as designations by language,
  `id`, `classKind`, `notSelectable`, `definition`, `exclusion`, `source`, and
  `browserUrl` as properties, and the postcoordination scales beside the
  store. The provider follows the HL7 terminology ecosystem cases for ICD-11:
  a code is a short code, an entity URI in the unversioned or versioned form,
  or a postcoordination expression (`1A00&XN8P1`, `1D01.0Y/1G41/1G40`, the
  ICF `d5409.qp3`, the URI form), validated against the stem's axes (an
  unfilled axis first, then a required one, then WHO's order; a `/` member
  that fits an unfilled axis is a value, else a new stem) with `stem` and
  `postcoordinationValues` reporting the binding; `parent` and `child` are
  URIs; a stem's scale is the implicit value set
  `<uri>/postcoordinationScale/<axis>`; `$subsumes` follows the tree. The
  cache and the artifacts stay local under the CC BY-ND terms.


## ATC/DDD (WHO)

- **Identity.** `http://www.whocc.no/atc` (OID 2.16.840.1.113883.6.73); no
  FHIR filters, properties, or implicit value sets; THO holds no content
  (<https://terminology.hl7.org/CodeSystem-v3-WC.html>).
- **Distribution and licence.** The annual index (January) is purchased from
  the WHO Collaborating Centre as a spreadsheet or XML (about EUR 200,
  unverified); copying for commercial purposes and changing the material are
  not allowed (<https://atcddd.fhi.no/copyright_disclaimer/>). In the
  Netherlands the G-Standaard carries ATC with Dutch descriptions in files 801
  and 802. Content is never committed.
- **Hierarchy.** Five levels under fourteen anatomical main groups
  (`A`, `A10`, `A10B`, `A10BA`, `A10BA02`), one parent each: `classified-with`
  (<https://atcddd.fhi.no/atc/structure_and_principles/>).
- **Served.** `ferroterm-build --atc <index.csv|BST801T> --atc-version <year>
  --out <dir>` reads the WHO index exported as CSV (`ATC code`, `ATC level
  name`, `DDD`, `U`, `Adm.R`, `Note`; the delimiter detected from the header)
  or the G-Standaard file `BST801T` (fixed-length records, the Dutch and
  English names, the indicator; removed records skipped) into the
  classification layout: the five levels as `kind`, each code under the
  parent its prefix names, the names as designations by language, every DDD
  a `ddd` property (`2 g O`, the note after a semicolon), the indicator an
  `indicator` property. The classification provider serves it: the generic
  tree filters, `kind` and `ddd` filters, `$subsumes` from the closure. The
  G-Standaard DDD file (`BST802T`) is not read: its units and routes are
  thesaurus references whose file is not described here.

## ICPC-2 and ICPC-1 NL

- **Identity.** ICPC-2 is `http://hl7.org/fhir/sid/icpc-2` (OID
  2.16.840.1.113883.6.139), ICPC-1 NL is `http://hl7.org/fhir/sid/icpc-1-nl`
  (OID 2.16.840.1.113883.2.4.4.31.1); Dutch general practice uses ICPC-1 NL
  (NHG-Tabel 24) with the NHG thesaurus (<https://www.nhg.org/praktijkvoering/informatisering/icpc/>).
- **Distribution and licence.** WONCA holds the ICPC-2 copyright; ICPC-2e
  v7.0 ships as Excel and ClaML, free for non-commercial use, fee for
  commercial or national use (unverified). NHG-Tabel 24 sits behind an NHG
  licence (annual fee or per practice), no copying, released on the NHG
  schedule (<https://referentiemodel.nhg.org/licenties>). Both licence-gated;
  never committed.
- **Hierarchy.** 17 chapters by 7 components, a biaxial structure; NL adds
  sub-rubrics.
- **FerroTERM plan.** The ClaML loader for ICPC-2e; a table loader for the NHG
  source data when a licence holder deploys it. The Nictiz server serves
  NHG-24 and NHG-45 to licence holders.

## RxNorm

- **Identity.** System `http://www.nlm.nih.gov/research/umls/rxnorm`, version
  the release date as in the file names (`08032026`); codes are RXCUIs with
  `SAB = RXNORM`; display is the `RXNORM` string of the `SCD` or `SBD` term
  (<https://terminology.hl7.org/RxNorm.html>).
- **Distribution and licence.** Monthly full release (UTS account and UMLS
  licence) and the "Current Prescribable Content" subset with no licence
  (<https://www.nlm.nih.gov/research/umls/rxnorm/docs/prescribe.html>).
  NLM-created names and RXCUIs are public domain with attribution; other
  UMLS sources carry their own category. The prescribable subset (75 MB, RRF
  pipe-delimited: `RXNCONSO` 246,041 rows, `RXNREL` 2.58 million,
  `RXNSAT` 3.36 million) is the loader's input. Real RXCUIs may appear in
  fixtures with attribution.
- **Hierarchy.** None defined by FHIR; typed relationships (`REL`: SY, SIB,
  RN, PAR, CHD, RB, RO; `RELA`: `has_ingredient`, `tradename_of`,
  `has_dose_form`, `isa`, and the rest of Appendix 1) are the filter surface.
- **FHIR-defined behaviour.** Filters `STY`, `SAB`, `TTY`, `[REL]`, `[RELA]`
  with `=` or `in`; no implicit value sets beyond `/vs` (all CUIs); lookup
  properties "yet to be done" in the specification, so any FerroTERM exposes are
  our own design and marked so.
- **Served.** `ferroterm-build --rxnorm <RxNorm_full_prescribe_MMDDYYYY.zip|dir>
  [--rxnorm-version <MMDDYYYY>] [--rxnorm-sources <SAB,...>]` reads the `RRF`
  tables streaming (`RXNCONSO`, `RXNREL`, `RXNSAT`, and `RXNSTY` when the
  release has it): the RXCUIs with an `RXNORM` atom are the codes, the
  `RXNORM` string of the most preferred term type (`SCD`, `SBD`, then the
  packs, groups, forms, components, ingredients, brand names, dose forms) is
  the display, every atom of the kept sources is a designation with its term
  type as the use, and `TTY`, `SAB`, `STY`, and the `RXNORM` attributes
  (`NDC`, `RXN_AVAILABLE_STRENGTH`, ...) are properties. The `RXNORM`
  relationships are typed edges in both directions (`relations.bin`), the atom
  identifiers a table (`atoms.bin`), so `has_ingredient = CUI:1191` and the
  other `REL` and `RELA` filters answer by index and `AUI:` values resolve.
  Only the unrestricted sources (`RXNORM`, `MTHSPL`) are kept unless
  `--rxnorm-sources` names the licensed ones a full release carries. The
  prescribable subset (September 2026) builds in 11 s into 81,468 concepts and
  1.13 million edges; `$lookup` of an ingredient with hundreds of relationships
  answers in under 1 ms once the codes are held in memory.


## BCP 47, BCP 13, ISO 3166

- **BCP 47** (`urn:ietf:bcp:47`, languages) and **BCP 13** (`urn:ietf:bcp:13`,
  media types) are grammar and registry systems; the FHIR value sets
  `all-languages` and `mimetypes` "cannot be expanded because ... an infinite
  number of members" (<https://hl7.org/fhir/R4B/valueset-all-languages.html>).
  The providers validate by grammar (and the IANA registries). Neither system
  enumerates as a whole; each enumerates the bounded selections its registry
  gives it, BCP 13 under `registered = true` and BCP 47 under a fixed primary
  language subtag, and marks the result unclosed. R5 no longer lists either on
  the systems page; THO lists BCP 47 only.
- **Served.** All four ship with the server and need no configuration (UCUM
  is below):
  BCP 47 validates a tag by the RFC 5646 grammar and the IANA Language Subtag
  Registry (well-formed with an unregistered subtag is not a code), BCP 13 by
  the RFC 6838 grammar with `registered` and `base` filters and subsumption by
  parameters (`text/plain` subsumes `text/plain; charset=utf-8`), and ISO
  3166-1 as a table from Unicode CLDR (alpha-2 codes, English names, `alpha3`
  and `numeric` properties, the user-assigned ranges displayed as
  `User-assigned`). The registry data is vendored under
  `crates/fhir-terminology/data/` with provenance.
- **ISO 3166** (`urn:iso:std:iso:3166`, `:-2`, `:-3`): codes upper case, compared
  case-insensitively; one filter, `code regex`; version is the year
  (<https://terminology.hl7.org/ISO3166.html>). A small table provider.

## Dutch national code systems

Every Dutch national artefact is licence-gated (NHG, DHD, Z-Index, WHO-FIC
NL), so none ships with the server; only the SNOMED CT Netherlands edition
licence is free. They are listed here because a Dutch deployment needs them and
each has a clear provider shape.

- **SNOMED CT Netherlands edition.** Edition URI
  `http://snomed.info/sct/11000146104` (module id read from the SNOMED docs,
  partially unverified), monthly RF2 releases since September 2024 through
  Nictiz as National Release Centre, free licence via MLDS; 379,548 active
  concepts of which 280,618 had Dutch translations in March 2024
  (<https://nictiz.nl/nieuws/nictiz-publiceert-nieuwe-snomed-release/>). This
  is the SNOMED provider with the Dutch extension modules; the Module
  Dependency refset assembles it (`.claude/rules/snomed-terminology.md`).
- **DHD Diagnosethesaurus and Verrichtingenthesaurus.** SNOMED-based term
  lists (over 25,000 diagnosis terms, about 10,000 procedures) with their own
  concept ids (OID 2.16.840.1.113883.2.4.3.120.5.1), SNOMED links, and ICD-10,
  DBC, and ZA derivations; exchanged as the SNOMED refset `31000147101` and
  delivered to licensees as CSV zips ("Uitleverformaat 5.0", two-monthly)
  (<https://www.dhd.nl/assets/uploads/Uitleverformaat-Thesauri-5.0-v1.0.pdf>).
  **Served.** `ferroterm-build --dhd <delivery.zip|dir> [--dhd-version <v>]
  --out <dir>` reads the Uitleverformaat 5.0 tables (`ThesaurusConcept`,
  `ThesaurusTerm`, `ThesaurusConceptRelaties`, `ThesaurusConceptRol`,
  `Parapluterm`, `AfleidingICD10`, `AfleidingDBC`, `AfleidingZA`,
  `CodeMapping`; found by their table suffix, the version from the delivery
  name) into the classification layout as a flat table under
  `urn:oid:2.16.840.1.113883.2.4.3.120.5.1`: the concept type as `kind`,
  the preferred term, synonyms, patient-friendly terms, fully specified names,
  and search terms as designations by language, concepts and terms ended
  before the delivery date inactive or skipped, and the SNOMED CT identifier,
  the ICD-10, DBC, and ZA derivations, the roles, the code mappings,
  replacements, splits, and umbrella terms as properties. The build also
  writes `conceptmaps/dhd-to-snomed.json` and `dhd-to-icd10.json` (FHIR R4B
  `ConceptMap`, `equivalent` and `wider`) for `FERROTERM_CODESYSTEMS`, so
  `$translate` answers the links. The refset `31000147101` rides on the SNOMED
  provider as `?fhir_vs=refset/31000147101`.
- **G-Standaard (Z-Index).** The Dutch medication database: about 80
  ASCII fixed-length files, monthly, paid subscription; the product ladder GPK,
  PRK, HPK, artikel (OIDs `2.16.840.1.113883.2.4.4.1`, `.10`, `.7`, `.8`) plus
  ATC (801), DDD (802), and the thesauri (902) used by the zib medication
  building blocks (<https://www.z-index.nl/documentatie/bestandsbeschrijvingen>,
  <https://zibs.nl/wiki/FarmaceutischProduct-v2.2.1(2024NL)>). No FHIR
  representation is published; systems are `urn:oid` URIs. **Served.**
  `ferroterm-build --gstandaard <dir> --gstandaard-version <release> --out
  <dir>` reads `BST711T` (GPK), `BST052T` (PRK), `BST031T` (HPK), and
  `BST004T` (articles) at the published positions, the names through
  `BST020T` (the full name as display, the short and label names as
  designations) and the coded form, route, and unit through `BST902T`, into
  four flat classifications under `<out>/{gpk,prk,hpk,artikel}`. The rungs
  above a concept are properties (`gpk`, `prk`, `hpk`), as are the ATC code,
  substance, strength, form, route, brand, and firm; removed records are
  skipped and an article with a removal date is inactive. `BST801T` builds
  ATC (above). The release is given on the command line; the files carry
  none.
- **Nederlandse Labcodeset.** Over 5,000 laboratory determinations: LOINC
  concepts with Dutch names, SNOMED materials and outcome lists, UCUM units,
  published as one XML document and on the Nictiz server
  (<https://www.nictiz.nl/wat-we-doen/activiteiten/terminologie/nederlandse-labcodeset/>).
  Provider shape: a value set and supplement over the LOINC, SNOMED, and UCUM
  providers, not a code system of its own. `ferroterm-build --labcodeset`
  reads the publication (`crates/labcodeset`) and writes the value set, the
  LOINC supplement, and the ordinal outcome value sets as FHIR resources.
- **Identifier namespaces** (BSN, UZI, URA, BIG, AGB, UZOVI) are
  `NamingSystem`s, not code systems; nothing to load.

## The reference deployment: Nictiz Nationale Terminologieserver

The Dutch national terminology server (`https://terminologieserver.nl/fhir`) is
Ontoserver 6.25 serving FHIR R4 (4.0.1) behind SMART-on-FHIR, with `$lookup`,
`$validate-code`, `$subsumes`, `$find-matches`, `$expand`, `$translate`, and
`$closure` (its CapabilityStatement, fetched 2026-09-02). It serves ICD-10,
UCUM, HPO, HGNC, and ART-DECOR content licence-free, and SNOMED CT, LOINC, the
Labcodeset, ICF, and the NHG tables to licence holders
(<https://www.nictiz.nl/document/nts-manual-for-new-users-12-03-2024pdf>). It is
the behavioural oracle for the Dutch variants (ICD-10-NL, the NL edition, the
Labcodeset) in the sense of `.claude/rules/spec-adherence.md`: a reference for
spec-silent edge cases, never the specification.

## US registries: CPT, CVX, NDC, MDC, UNII, NCI Metathesaurus

Each has a FHIR page with an identity and little else (no filters or implicit
value sets, except the THO-era CPT filters `modifier`, `kind`, `modified`,
`code in` ranges, `telemedicine`, `orthopox`, and the NCI Metathesaurus
`STY`/`SAB`/`TTY`/`[REL]`/`[RELA]` filters). CPT is AMA-licensed and "HL7 does
not distribute a pre-built CodeSystem"; NDC changes daily and "cannot be
versioned completely" (<https://terminology.hl7.org/CPT.html>,
<https://terminology.hl7.org/NDC.html>). These become table providers when a
deployment needs them; none is on the near-term roadmap.

## Adding a code system

1. Read the FHIR page for the system (R4B page, or the THO page for R5) and
   record its section here: identity, distribution, licence and fixture policy,
   hierarchy, FHIR-defined filters, properties, and implicit value sets.
2. File the tracker issue as a sub-issue of the program issue, in build order.
3. Add a loader crate (`crates/ferroterm-<system>`) that maps the release into the
   substrates, and a provider that declares the system's capabilities through
   the FHIR `CodeSystem` metadata. Nothing in `fhir-terminology` changes.
4. Add a rule file `.claude/rules/<system>-terminology.md` if the system has
   spec-facing behaviour beyond the generic provider.
5. Run the tx-ecosystem suite in the system's mode where one exists.
