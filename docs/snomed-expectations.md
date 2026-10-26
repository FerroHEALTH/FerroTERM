<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# SNOMED CT terminology service expectations

What SNOMED International and HL7 expect of a SNOMED CT terminology service,
checked one by one against what FerroTERM actually answered, over two
licensed editions.

Every verdict here comes from a request made to a running server, not from
reading the code. The server was `cargo build --release -p ferroterm-server`
at `68e66c767` on `main`, release 0.1.0, bound to `127.0.0.1:8211`, serving
`/r4b`. It was run three times over the artifacts the owner's licensed
releases build into:

| Run | `FERROTERM__TERMINOLOGY__INDEX` | Edition served | Concepts | Reference sets | Languages |
|---|---|---|---|---|---|
| NL | `artifacts/nl` | `http://snomed.info/sct/11000146104/version/20260630` | 548,949 | 75 | en, nl |
| INT | `artifacts/int` | `http://snomed.info/sct/900000000000207008/version/20260901` | 535,502 | 25 | en |
| Both | both directories | both of the above | | | |

Both editions matter, and they answer differently. The NL edition carries
Dutch designations, 50 more reference sets, and content the International
edition does not have. Every expectation about editions, versions, or
language reference sets below was checked on both.

A verdict is one of three:

- **Met.** A request was made and the answer was right. The row names the
  test that asserts the behaviour, and the detail sections show the response.
- **Partially met.** The row names exactly what is missing and the issue that
  tracks it.
- **Not offered.** FerroTERM does not do this, and the row says why.

37 expectations were checked: 35 met, 2 not offered. Some appear in two
sources, and where they do the detail says so.

A row that a later change fixes is re-checked the same way, on a server built
from that change over the same two artifact directories, and the detail says
which issue moved it. [C22](#c22-detail) was re-checked on issue #361,
[C5](#c5-detail) on issues #367 and #638, [B1](#b1-detail) on issue #359, and
[C10](#c10-detail), [C23](#c23-detail), and [C24](#c24-detail) on issues #360,
#363, and #362.

## Where the expectations come from

Four sources, in the precedence `.claude/rules/spec-adherence.md` sets.

1. SNOMED International, "Terminology services"
   (<https://www.implementation.snomed.org/terminology-services>). The page
   states what a SNOMED CT terminology service does, in four capability
   bullets and one statement about loading a release.
2. The SNOMED CT URI Standard
   (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard>),
   §2 SNOMED CT URI Space and §3 SNOMED CT URIs in Use.
3. The SNOMED on FHIR implementation guide. **This source could not be
   read.** Both published locations answer `404` as of 2026-09-05:
   `https://build.fhir.org/ig/IHTSDO/snomed-ig/introduction.html` and
   `https://hl7.org/fhir/uv/snomed-ig/`. Nothing below is derived from it,
   and no expectation is attributed to it.
4. The FHIR SNOMED CT page, §4.3.1.0 Using SNOMED CT with FHIR
   (<https://hl7.org/fhir/R4B/snomedct.html>). This is the normative HL7 text
   for the version FerroTERM serves at `/r4b`, and it carries most of the
   detail the first two sources leave general.

The ECL specification governs what an expression constraint means. It is not
a separate section here, because source 1 states the expectation ("Executing
Expression Constraint Queries (ECL) on a particular SNOMED CT edition") and
the observed ECL coverage sits under [A3](#a3) as its evidence.

## Source 1: SNOMED International, "Terminology services"

| # | Expectation | Verdict | Evidence |
|---|---|---|---|
| <a id="a1"></a>A1 | "Searching for SNOMED CT content using term matching, the hierarchy, or defining relationships" | **Met** | [A1](#a1-detail) · `crates/fhir-terminology/tests/it/snomed.rs::search_reads_the_designation_index`, `::the_hierarchy_answers_subsumption_and_the_filters_from_the_closure` |
| <a id="a2"></a>A2 | "Retrieving information about a given concept, including descriptions for a given dialect, supertypes, subtypes and defining relationships" | **Met** | [A2](#a2-detail) · `crates/fhir-terminology/tests/it/snomed.rs::properties_follow_the_snomed_on_fhir_list`, `::designations_carry_the_snomed_use_codings_and_filter_by_language` |
| <a id="a3"></a>A3 | "Executing Expression Constraint Queries (ECL) on a particular SNOMED CT edition" | **Met** | [A3](#a3-detail) · `crates/fhir-terminology/tests/it/ecl.rs` (13 tests), `crates/fhir-terminology/tests/it/snomed.rs::ecl_arrives_as_the_constraint_filter_and_the_ecl_implicit_value_set` |
| <a id="a4"></a>A4 | "Accessing maps to/from SNOMED CT and other reference set information" | **Met** | [C23](#c23-detail), [C26](#c26-detail) · `crates/fhir-terminology/tests/it/snomed.rs::a_language_reference_set_is_listed_and_its_member_forms_are_refused` |
| <a id="a5"></a>A5 | "Terminology servers can either import data from a SNOMED CT release package, or periodically synchronise its content", and make the content "available … using a convenient API" | **Met** | [A5](#a5-detail) · `crates/rf2/tests/it/local_edition.rs::the_local_edition_loads_and_identifies_itself`, `tools/ferroterm-build/tests/it/pipeline.rs::the_manifest_records_the_edition_and_the_counts` |

## Source 2: the SNOMED CT URI Standard

| # | Expectation | Verdict | Evidence |
|---|---|---|---|
| <a id="b1"></a>B1 | §2 URIs for Editions and Versions: an edition is `http://snomed.info/sct/{sctid}` and a version `http://snomed.info/sct/{sctid}/version/{timestamp}`, with the International Edition at `900000000000207008` (Table 2.1, and the §3 CTS2 table) | **Met** | [B1](#b1-detail), re-checked on issue #359 |
| <a id="b2"></a>B2 | §3 Identifying SNOMED CT Versions in HL7 FHIR: the code system URI is `http://snomed.info/sct` and the version string is `http://snomed.info/sct/{sctid}/version/{timestamp}` | **Met** | [C1](#c1-detail), [C16](#c16-detail) |
| <a id="b3"></a>B3 | §2 URIs for components (`http://snomed.info/id/{sctid}`), modules (`http://snomed.info/module/{sctid}`), and RF2 fields (`http://snomed.info/field/{table}.{field}`) | **Not offered** | [B3](#b3-detail) |

## Source 4: FHIR §4.3.1.0, Using SNOMED CT with FHIR

| # | Expectation (section) | Verdict | Evidence |
|---|---|---|---|
| <a id="c1"></a>C1 | §.1 System: "The URI `http://snomed.info/sct` identifies the SNOMED CT code system" | **Met** | [C1](#c1-detail) · `crates/fhir-terminology/tests/it/snomed.rs::identity_and_declaration_follow_the_manifest` |
| <a id="c2"></a>C2 | §.1 Version: a URI for a specific Edition published on a particular date | **Met** | [B1](#b1-detail) |
| <a id="c3"></a>C3 | §.1 Code: Concept IDs are valid in the `code` element | **Met** | [C3](#c3-detail) · `::locate_accepts_valid_sctids_only` |
| <a id="c4"></a>C4 | §.1 Code: "SNOMED CT Terms and Description Identifiers are not valid as codes in FHIR" | **Met** | [C3](#c3-detail) · `::locate_accepts_valid_sctids_only` |
| <a id="c5"></a>C5 | §.1, §.5 Code: SNOMED CT Expressions in Compositional Grammar are valid in the `code` element | **Met** | [C5](#c5-detail) · `crates/fhir-terminology/tests/it/snomed_expression.rs::a_well_formed_expression_over_defined_concepts_validates`; `app/ferroterm-server/tests/it/snomed_expression.rs::validate_code_accepts_an_expression_on_every_served_version` |
| <a id="c6"></a>C6 | §.1 Display: "The best display is the preferred term in the relevant language or dialect, as specified in the associated language reference set" | **Met** | [C6](#c6-detail) · `::display_is_the_preferred_term_of_the_language_with_a_stated_fallback` |
| <a id="c7"></a>C7 | §.1 Inactive: "Inactive codes are identified using the 'inactive' property" | **Met** | [C7](#c7-detail) · `::properties_follow_the_snomed_on_fhir_list` |
| <a id="c8"></a>C8 | §.1 Subsumption: "based on the \|is a\| relationship defined by SNOMED CT" | **Met** | [C8](#c8-detail) · `::the_hierarchy_answers_subsumption_and_the_filters_from_the_closure` |
| <a id="c9"></a>C9 | §.3: "Servers SHOULD regard provision of the date only for the version (without an sctid) as an error, and refuse to process the interaction or operation" | **Met** | [C9](#c9-detail) |
| <a id="c10"></a>C10 | §.3: "At minimum the URI SHOULD contain the sctid", and the service "may default to the most recent version of the named SNOMED CT distribution" | **Met** | [C10](#c10-detail) · `crates/fhir-terminology/tests/it/snomed_editions.rs::an_edition_uri_without_a_date_names_the_greatest_release_of_that_edition` |
| <a id="c11"></a>C11 | §.3: with no version URI the service "may default to the most recent version of the SNOMED CT International Edition available on the service" | **Met** | [C11](#c11-detail) |
| <a id="c12"></a>C12 | §.4: designations in additional languages, the language a BCP 47 code from the RF2 `languageCode`, the description type in `designation.use`, returned under `includeDesignations` | **Met** | [C12](#c12-detail) · `::designations_carry_the_snomed_use_codings_and_filter_by_language` |
| <a id="c13"></a>C13 | §.7: the `inactive`, `sufficientlyDefined`, and `moduleId` properties | **Met** | [C13](#c13-detail) · `::properties_follow_the_snomed_on_fhir_list` |
| <a id="c14"></a>C14 | §.7: the `normalForm` and `normalFormTerse` properties | **Met** | [C14](#c14-detail) · `::the_normal_form_renders_the_inferred_view_of_a_concept`, `::the_normal_form_is_answered_only_where_a_request_asks_for_it` |
| <a id="c15"></a>C15 | §.7: "SNOMED CT relationships, where the relationship type is subsumed by 410662002 \|Concept model attribute\|, also automatically become properties", named by concept id | **Met** | [C13](#c13-detail) · `::properties_follow_the_snomed_on_fhir_list` |
| <a id="c16"></a>C16 | §.7: "when a `$lookup` operation is performed on a SNOMED CT concept, servers SHALL return the URI for the edition and version being used … in the `version` property" | **Met** | [C16](#c16-detail) |
| <a id="c17"></a>C17 | §.8.1 By Subsumption: property `concept`, operator `is-a` | **Met** | [C17](#c17-detail) · `::the_hierarchy_answers_subsumption_and_the_filters_from_the_closure` |
| <a id="c18"></a>C18 | §.8.2 By Reference Set: property `concept`, operator `in` | **Met** | [C17](#c17-detail) |
| <a id="c19"></a>C19 | §.8.3 By SNOMED Expression Constraint: property `constraint`, operator `=` | **Met** | [C17](#c17-detail) · `::ecl_arrives_as_the_constraint_filter_and_the_ecl_implicit_value_set` |
| <a id="c20"></a>C20 | §.8.4 By whether post-coordination is allowed: property `expressions`, operator `=`, values true or false | **Met** | [C17](#c17-detail) · `crates/fhir-terminology/tests/it/snomed_expression.rs::the_expressions_filter_decides_whether_a_value_set_admits_an_expression` |
| <a id="c21"></a>C21 | §.9: the five query forms `?fhir_vs`, `?fhir_vs=isa/[sctid]`, `?fhir_vs=refset`, `?fhir_vs=refset/[sctid]`, `?fhir_vs=ecl/[ecl]`, the ECL URI-encoded | **Met** | [C21](#c21-detail) · `::the_implicit_value_sets_follow_the_snomed_ct_page`, `::malformed_and_unknown_implicit_value_sets_are_refused` |
| <a id="c22"></a>C22 | §.9: "The base URL is either `http://snomed.info/sct`, or the URI for the edition version" | **Met** | [C22](#c22-detail) · `crates/fhir-terminology/tests/it/snomed_editions.rs` (7 tests) |
| <a id="c23"></a>C23 | §.9: "`?fhir_vs=refset` - all concept ids that correspond to reference sets that are explicitly defined in the specified SNOMED CT edition" | **Met** | [C23](#c23-detail) · `::a_language_reference_set_is_listed_and_its_member_forms_are_refused` |
| <a id="c24"></a>C24 | §.9: "the content of the resource must conform to the template provided" (`url`, `version`, `name`, `description`, `copyright`, `status`, `compose`) | **Met** | [C24](#c24-detail) · `::an_implicit_value_set_carries_the_page_s_template`, `app/ferroterm-server/tests/it/value_set.rs::an_implicit_snomed_value_set_carries_the_page_s_template` |
| <a id="c25"></a>C25 | §.9: "If no version or edition is specified, the terminology service SHALL use the latest version available for its default edition" | **Met** | [C11](#c11-detail) |
| <a id="c26"></a>C26 | §.10: implicit concept maps `?fhir_cm=[sctid]` for the POSSIBLY EQUIVALENT TO, REPLACED BY, SAME AS, and ALTERNATIVE association reference sets, with the relationship the table gives each | **Met** | [C26](#c26-detail) · `crates/fhir-terminology/tests/it/snomed_concept_map.rs::each_association_reference_set_translates_with_the_relationship_the_page_gives_it` |
| <a id="c27"></a>C27 | §.10: "Simple Map Reference Sets (reference sets which are descendants of 900000000000496009 "Simple map") also define an implicit concept map" | **Met** | [C26](#c26-detail) · `::a_map_reference_set_translates_to_its_target_code_with_the_rf2_columns` |
| <a id="c28"></a>C28 | §.10: the concept map template | **Met** | `::an_association_map_carries_the_page_s_template` |
| <a id="c29"></a>C29 | §.2 Copyright and Licenses: SNOMED CT content is SNOMED International's and implementers need an Affiliate licence | **Met** | [C29](#c29-detail) |

## The detail

Every response below was copied from the run, with the parameter names
FHIR gives them. Concept identifiers appear only where they carry the
evidence: `74400008 |Appendicitis|` is the worked example of the SNOMED CT
URI Standard itself (§2, Table 2.2), and the rest are metadata concepts
published in the specifications.

### Met

<a id="a1-detail"></a>**A1. Search by term, by the hierarchy, and by defining
relationships.** All three answered on both editions, and the two editions
gave different counts, which is the point of the expectation.

Term matching, on the NL edition:

```
GET /r4b/ValueSet/$expand?url=http://snomed.info/sct?fhir_vs&filter=appendicitis
-> 200, expansion.total 60
```

The same request on the International edition returns 57. Scoping the search
to a subtree combines term matching with the hierarchy:

```
GET /r4b/ValueSet/$expand?url=http://snomed.info/sct?fhir_vs=isa/404684003&filter=appendic
-> 200, expansion.total 64        (NL)
-> 200, expansion.total 61        (International)
```

Search by defining relationship goes through the attribute graph:

```
GET /r4b/ValueSet/$expand
  ?url=http://snomed.info/sct?fhir_vs=ecl/%3C%20404684003%20%3A%20363698007%20%3D%2066754008
-> 200, expansion.total 103       (NL)
-> 200, expansion.total 102       (International)
```

<a id="a2-detail"></a>**A2. Concept retrieval: descriptions for a dialect,
supertypes, subtypes, defining relationships.** One `$lookup` returns all
four. On the International edition, `74400008`:

```
display        Appendicitis
designation    en 900000000000013009 Appendicitis
designation    en 900000000000003001 Appendicitis (disorder)
property       parent  18526009
property       parent  302168000
property       child   5596004
property       child   8744003            (and 13 more)
property       116676008  409774005       (associated morphology)
property       363698007  66754008        (finding site)
```

The same concept on the NL edition adds the Dutch descriptions, and
`displayLanguage` selects between them:

```
GET …$lookup?system=http://snomed.info/sct&code=74400008&displayLanguage=nl
-> display "appendicitis"
GET …$lookup?system=http://snomed.info/sct&code=74400008&displayLanguage=en
-> display "Appendicitis"
```

<a id="a3-detail"></a>**A3. ECL, on a particular edition.** Every construct
below was sent as `?fhir_vs=ecl/[ecl]` and answered `200` on both editions.
The totals differ per edition wherever the content does, which is what
"on a particular SNOMED CT edition" asks for.

| Construct | Expression | NL | International |
|---|---|---|---|
| `<<` descendantOrSelfOf | `<< 74400008` | 35 | 35 |
| `<` descendantOf | `< 74400008` | 34 | 34 |
| `>` ancestorOf | `> 74400008` | 33 | 33 |
| `>>` ancestorOrSelfOf | `>> 74400008` | 34 | 34 |
| `OR` | `74400008 OR 5596004` | 2 | 2 |
| `MINUS` | `<< 74400008 MINUS << 5596004` | 34 | 34 |
| `AND`, grouped | `(< 404684003 : 363698007 = *) AND << 74400008` | 35 | 35 |
| Refinement | `< 404684003 : 363698007 = << 66754008` | 111 | 110 |
| Cardinality | `< 404684003 : [1..*] 363698007 = *` | 93,241 | 89,734 |
| `^` memberOf | `^ 447562003` | 137,425 | 137,678 |
| Reverse | `< 404684003 : R 363698007 = << 66754008` | 0 | 0 |
| Term filter | `< 404684003 {{ term = "appendic" }}` | 62 | 59 |
| Dialect filter | `< 404684003 {{ term = "appendic", dialect = nl-nl }}` | 41 | 41 |
| Description filter | `< 404684003 {{ D term = "blindedarm" }}` | 67 | 0 |
| Concept filter | `< 404684003 {{ C definitionStatus = primitive }}` | 48,375 | 46,979 |
| Member filter | `^ 447562003 {{ M mapTarget = "K37" }}` | 9 | 9 |
| Term annotation | `<< 74400008 \|Appendicitis\|` | 35 | 35 |
| History supplement | `<< 74400008 {{ +HISTORY }}` | 51 | 51 |

The Dutch description filter is the clearest edition split: 67 concepts on
the NL edition and 0 on the International, because the International edition
carries no Dutch descriptions.

Malformed ECL and valid ECL naming an unknown identifier are told apart,
which `.claude/rules/snomed-terminology.md` [S-ECL-4] requires:

```
?fhir_vs=ecl/<< (((        -> 422 invalid, vs-invalid
   expected a concept reference, '*', an alternate identifier, or '(' at byte 6,
   found the end of the expression
?fhir_vs=ecl/<< 99999999999 -> 400 code-invalid, invalid-code
   code `99999999999` is invalid: not a concept of the edition
```

The malformed answer also states where the parser stopped as data. Its
`issue.expression` names the input (`http.url` for the `url` query parameter,
`Parameters.parameter[i].valueUri` in a body, and
`Parameters.parameter[i].resource.compose.include[j].filter[k].value` for a
filter of an inline value set). The `operationoutcome-issue-line` extension
carries 1, and `operationoutcome-issue-col` carries the 1-based column,
counted in characters into that value as the client sent it, so the
percent-encoding of a query value counts.

<a id="a5-detail"></a>**A5. Load a release package, serve it over an API.**
Both editions were built offline by `ferroterm-build` from the owner's
licensed RF2 releases and served read-only. The manifest records the edition,
the version, the counts, and the languages, and the server prints them at
start:

```
serving code system  id snomed.info-sct-11000146104-version-20260630
  system http://snomed.info/sct
  version http://snomed.info/sct/11000146104/version/20260630
  concepts 548949  languages en,nl
```

The release type is chosen by the caller (`Release::open` takes a
`ReleaseType`, `crates/rf2/src/file.rs`) and both artifacts were built from
the Snapshot. The manifest does not record which release type produced the
state, so a served artifact cannot be asked. `.claude/rules/snomed-terminology.md`
[S-RF2-1] asks for that record. It is a build-artifact gap rather than a
served-behaviour one, and the API expectation the source states is met.

<a id="c1-detail"></a>**C1, B2. The system URI.** Every response names
`http://snomed.info/sct` and nothing else. `$lookup`, `$validate-code`, the
expansion members, and `$translate` all carry it, on both editions.

<a id="c3-detail"></a>**C3, C4. What is a valid code.** A concept id is; a
description identifier is not. On the International edition:

```
$lookup code=74400008    -> 200
$lookup code=123558018   -> 400 invalid-code
   code `123558018` is not in code system `http://snomed.info/sct`
   version `http://snomed.info/sct/449080006/version/20260901`
```

`123558018` is the description identifier the URI Standard uses in its own
example (§2, Table 2.2), and the page says description identifiers are not
valid codes.

<a id="c5-detail"></a>**C5. Post-coordinated expressions.** Served since issue
#638.

SNOMED CT Expressions in Compositional Grammar are valid codes per §.1 and are
"subject to the same rules as precoordinated concepts", so an expression
locates like any other code. `$lookup` answers the expression in one canonical
order with a generated display, `$validate-code` checks the syntax and every
concept reference, `$subsumes` takes an expression on either side, and
`$closure` relates one to the concepts a table holds.

`TerminologyCapabilities.codeSystem.version.compositional` is "If the
compositional grammar defined by the code system is supported", which this
server now is, and every served version carries `compositional: true` for
SNOMED CT.

Three boundaries are deliberate and each is recorded in the code.

- **No concept model check.** §7.3 Validating of the Compositional Grammar
  specification lists three checks: the syntax, the concept model, and that
  "All concept references included in the expression must be valid". The first
  and the third run here; MRCM validation does not.
- **No close-to-user transformation.** An expression is read as written. No
  transformation moves a loose attribute into a group or distributes a
  laterality, so an expression whose refinement repeats what its focus concept
  already states is accepted and compared as written.
- **Subsumption is sound and not complete.** SNOMED International withdrew
  normal-form comparison as a subsumption test in the July 2019 International
  Edition and directs implementers to "a description logic classifier"
  (<https://docs.snomed.org/snomed-international-documents/snomed-ct-glossary/n/normal-form>),
  and no reasoner runs here. What the server answers `subsumes` or
  `subsumed-by` holds in the edition's inferred view; what it answers
  `not-subsumed` is what that view does not state.

<a id="c6-detail"></a>**C6. Display is the preferred term of the language.**
Shown under [A2](#a2-detail). The NL edition returns the Dutch preferred term
for `displayLanguage=nl` and the English one for `displayLanguage=en`. The
International edition, asked for `nl`, falls back to English rather than
failing, which no section of the page forbids.

`$validate-code` states the whole choice set when a display is wrong, which
is how you can see the acceptability the language reference set gave each
term. On the NL edition:

```
$validate-code code=74400008 display="Not the term"
-> result false
   Wrong Display Name 'Not the term' for http://snomed.info/sct#74400008.
   Valid display is one of 7 choices: 'Appendicitis', 'Appendicitis' (en),
   'Appendicitis, NOS' (en), 'Appendicitis (disorder)' (en), 'appendicitis' (nl),
   'appendicitis (aandoening)' (nl) or 'blindedarmontsteking' (nl)
```

The International edition offers 4 choices for the same concept, all English.

<a id="c7-detail"></a>**C7. Inactive.** The property is returned on every
`$lookup`, and inactive concepts are served rather than hidden, which
[S-DIS-1] requires. On the NL edition, over one term search:

```
$expand …&filter=appendicitis&activeOnly=false -> total 60
$expand …&filter=appendicitis&activeOnly=true  -> total 38
```

<a id="c8-detail"></a>**C8. Subsumption on `is a`.** All four outcomes, on
both editions:

```
$subsumes codeA=74400008 codeB=5596004  -> subsumes
$subsumes codeA=5596004  codeB=74400008 -> subsumed-by
$subsumes codeA=74400008 codeB=74400008 -> equivalent
$subsumes codeA=74400008 codeB=125605004 -> not-subsumed
```

<a id="c9-detail"></a>**C9. A date-only version is refused.**

```
$lookup system=http://snomed.info/sct code=74400008 version=20260901
-> 404 not-found
   version `20260901` of code system `http://snomed.info/sct` is not served
```

The page says a server SHOULD treat this as an error and refuse to process
the operation. FerroTERM refuses.

<a id="c11-detail"></a>**C11, C25. Defaulting when no version is given.**
With both editions loaded, a request naming no version answers from the
International edition, the later of the two releases, and says so in the
`version` it returns:

```
$lookup system=http://snomed.info/sct code=74400008
-> version http://snomed.info/sct/449080006/version/20260901
```

The same defaulting applies to the bare `http://snomed.info/sct?fhir_vs`
base, which expands 535,502 concepts, the International count.

<a id="c12-detail"></a>**C12. Designations in other languages.** On the NL
edition, `includeDesignations` returns both languages with the SNOMED
description type in `use`:

```
$expand …includeDesignations=true, one concept
  display      appendicitis
  designation  en 900000000000013009 Appendicitis
  designation  en 900000000000013009 Appendicitis, NOS
  designation  en 900000000000003001 Appendicitis (disorder)
  designation  nl 900000000000013009 appendicitis
```

`900000000000003001` is Fully specified name and `900000000000013009` is
Synonym, the two the page names, and the language is a BCP 47 code taken
from the RF2 `languageCode`.

<a id="c13-detail"></a>**C13, C15. The properties.** `inactive`,
`sufficientlyDefined`, and `moduleId` are returned on every `$lookup`,
alongside `parent`, `child`, `effectiveTime`, and the concept model
attributes named by concept id (shown under [A2](#a2-detail)). The
capability statement lists 108 attribute properties for the NL edition and
107 for the International, each an sctid, which is what §.7 asks for
("Properties that represent SNOMED CT concept model attributes are referred
to using their concept id, rather than their human readable term").

`effectiveTime` is not a property the page defines. §.7 closes with "Other
properties are at the discretion of the server and the client", so it is
allowed.

<a id="c16-detail"></a>**C16. The `SHALL` on the version.** Every `$lookup`
returns the edition and version URI, on both editions:

```
NL   -> version http://snomed.info/sct/11000146104/version/20260630
INT  -> version http://snomed.info/sct/449080006/version/20260901
```

`$expand` states it too, as `expansion.parameter` `used-codesystem`:

```
used-codesystem  http://snomed.info/sct|http://snomed.info/sct/449080006/version/20260901
```

The International URI is the wrong sctid, which is [B1](#b1-detail). The
obligation this row states, that the server returns the edition and version
URI it used, is met.

<a id="c17-detail"></a>**C17, C18, C19, C20. The four filters.** Each was
sent as a `ValueSet.compose.include.filter` with the version pinned, on both
editions:

```
concept   is-a  74400008     -> 200, total 35        (both editions)
concept   in    723264001    -> 200, total 21431     (International)
constraint =    << 74400008 : 363698007 = 66754008
                             -> 200, total 33        (International)
expressions =   false        -> 200, total 535502    (International)
```

The transcript above was taken before post-coordination was served, when
`expressions = true` was refused with `422 vs-invalid`. Both values now select
every precoordinated concept of the version, and the value decides whether an
expression may join the selection
(`crates/fhir-terminology/tests/it/snomed_expression.rs::the_expressions_filter_decides_whether_a_value_set_admits_an_expression`).
Re-running the transcript against a licensed edition is the differential
harness's job. Pinning the version routes correctly: an
include pinned to the NL edition expands 548,949 concepts and one pinned to
the International expands 535,502.

<a id="c21-detail"></a>**C21. The five implicit forms.** All five parse and
evaluate, on both editions, with the ECL URI-decoded:

```
?fhir_vs                    -> 548,949 (NL)   535,502 (International)
?fhir_vs=isa/74400008       -> 35              35
?fhir_vs=refset             -> 75              25
?fhir_vs=refset/447562003   -> 137,425         137,678
?fhir_vs=ecl/%3C%3C%2074400008 -> 35           35
```

`?fhir_vs=isa/[sctid]` returns a nested expansion, the focus concept with its
descendants under `contains`, and flattens under `excludeNested=true`. Both
shapes are legal FHIR, and the totals match the `ecl` equivalent.

An unknown form is refused with an `OperationOutcome` rather than a 500,
which [S-IMP-1] requires:

```
?fhir_vs=nonsense/1 -> 422 invalid, vs-invalid
   `nonsense/1` is not a `fhir_vs` form
```

<a id="c22-detail"></a>**C22. The edition in the base picks the edition.**
Issue #361.

§.9 says "The base URL is either `http://snomed.info/sct`, or the URI for the
edition version", and that the membership "will depend on the edition used
when it is expanded". With the NL and the International editions loaded
together, each base answers from the edition it names, and the totals split
per edition:

```
                                 NL base      International base
?fhir_vs                         548,949      535,502
?fhir_vs=isa/74400008                 35           35
?fhir_vs=refset                       75           25
?fhir_vs=refset/447562003        137,425      137,678
?fhir_vs=ecl/%3C%3C%2074400008        35           35
```

Each expansion echoes the edition it used, and only the NL base returns the
Dutch preferred terms:

```
$expand url=…/11000146104/version/20260630?fhir_vs=isa/74400008 displayLanguage=nl
-> used-codesystem http://snomed.info/sct|http://snomed.info/sct/11000146104/version/20260630
   "acute obstructieve appendicitis", "atypische appendicitis"

$expand url=…/449080006/version/20260901?fhir_vs=isa/74400008 displayLanguage=nl
-> used-codesystem http://snomed.info/sct|http://snomed.info/sct/449080006/version/20260901
   "Acute obstructive appendicitis", "Atypical appendicitis"
```

The bare `http://snomed.info/sct` base still answers from the default
edition, shown under [C11](#c11-detail). An edition version no loaded edition
serves is that version missing, which is what the same URI already answers
for an explicit `version` parameter:

```
$expand url=http://snomed.info/sct/32506021000036107/version/20260101?fhir_vs
-> 404 not-found, not-found
   version `http://snomed.info/sct/32506021000036107/version/20260101` of code
   system `http://snomed.info/sct` is not served
```

The implicit concept maps resolve by the same rule: `?fhir_cm=` on either
edition's base answers, and on an edition the server does not hold it is the
same 404.

<a id="c26-detail"></a>**C26, C27. The implicit concept maps.** All four
association reference sets and the map reference sets answer `$translate`,
on both editions. An active concept with no association returns no match,
and an inactive one is routed to its successor. On the NL edition:

```
$translate url=…?fhir_cm=900000000000527005 code=74400008    (active)
-> result false, "No translations found"

$translate url=…?fhir_cm=900000000000527005 code=155728006   (inactive)
-> result true
   equivalence equal
   concept 74400008 Appendicitis
     version http://snomed.info/sct/11000146104/version/20260630
```

`900000000000527005` is SAME AS, which the §.10 table gives the relationship
`equal`, and that is the equivalence returned. A map reference set returns
its target in the code system the reference set maps to, with the RF2 columns
as `product` parts:

```
$translate url=…?fhir_cm=447562003 code=74400008
-> result true
   equivalence relatedto
   concept K37 in http://hl7.org/fhir/sid/icd-10
   product mapGroup 1, mapPriority 1, mapRule TRUE, mapAdvice "ALWAYS K37",
           correlationId 447561005, mapCategoryId 447637006
```

<a id="c29-detail"></a>**C29. Copyright and licences.** The repository ships
no SNOMED CT content. `git ls-files data` returns 10 `.gitkeep` files and
nothing else, and every fixture in the test suites is synthetic
(`tools/ferroterm-testkit`). Both editions checked here came from the
owner's own licensed releases and live outside the repository, under
`artifacts/`, which `.gitignore` excludes.

The SNOMED International copyright notice now travels on the implicit value
sets themselves, which is [C24](#c24-detail).

<a id="b1-detail"></a>**B1, C2. The edition and version URIs.** Re-checked on
issue #359.

The URI Standard names the International Edition `900000000000207008`, in
§2 Table 2.1 and again in the §3 CTS2 table, and the artifact is served
under it:

```
$lookup code=74400008 version=http://snomed.info/sct/900000000000207008
-> 200, version http://snomed.info/sct/900000000000207008/version/20260901
```

The NL edition resolves to `http://snomed.info/sct/11000146104`, the
published Netherlands edition URI.

<a id="c10-detail"></a>**C10. An edition URI without a date names the
edition's greatest loaded release.** Re-checked on issue #360.

§.3 tells clients "At minimum the URI SHOULD contain the sctid of the SNOMED
CT distribution: `http://snomed.info/sct/[sctid]`", and says the service
"may default to the most recent version of the named SNOMED CT
distribution". Both editions answer their own edition URI, on the run that
served both:

```
$lookup code=74400008 version=http://snomed.info/sct/900000000000207008
-> 200, version http://snomed.info/sct/900000000000207008/version/20260901
$lookup code=74400008 version=http://snomed.info/sct/11000146104
-> 200, version http://snomed.info/sct/11000146104/version/20260630
```

The two refusals the same section asks for stay refusals: the date-only
form, and an edition no loaded release belongs to.

```
$lookup code=74400008 version=20260901
-> 404 not-found  version `20260901` … is not served
$lookup code=74400008 version=http://snomed.info/sct/449080006
-> 404 not-found  version `http://snomed.info/sct/449080006` … is not served
```

The resolution is the registry's, so every operation that takes a system
version reads it the same way.

<a id="c23-detail"></a>**A4, C23. The reference set list carries the language
reference sets.** Re-checked on issue #363.

§.9 says the set is "all concept ids that correspond to reference sets that
are explicitly defined in the specified SNOMED CT edition", with no category
excluded, so a language reference set belongs to it like a metadata one.

```
$expand url=…/11000146104/version/20260630?fhir_vs=refset&count=200
-> 200, total 80      (NL, was 75; the five language reference sets)
$expand url=…/900000000000207008/version/20260901?fhir_vs=refset&count=200
-> 200, total 27      (International, was 25)
```

The Dutch and the two English sets are in the NL list:

```
31000146106  900000000000508004  900000000000509007
```

The member-facing forms stay refused, and say why. A language reference set
references descriptions, so §.9's "all concept ids in the specified
reference set" selects none of them:

```
?fhir_vs=refset/900000000000509007
-> 400 invalid  filter `concept` value `900000000000509007` is invalid:
   a language reference set references descriptions, not concepts
?fhir_vs=ecl/^ 900000000000509007
-> 400 code-invalid  code `900000000000509007` is invalid: not a reference
   set with concept members in the edition
```

The ECL refusal is the same reading: ECL 2.2 §memberOf returns the
components a reference set references, and descriptions are not concepts, so
the expression has no concept-set answer here.

<a id="c24-detail"></a>**C24. The implicit value set carries its template.**
Re-checked on issue #362.

§.9 prints a template per form and says "the content of the resource must
conform to the template provided". What comes back, with the definition
asked for:

```
$expand url=…/900000000000207008/version/20260901?fhir_vs=isa/74400008
        &includeDefinition=true&count=1
{
  "resourceType": "ValueSet",
  "status": "active",
  "url": "http://snomed.info/sct/900000000000207008/version/20260901?fhir_vs=isa/74400008",
  "version": "http://snomed.info/sct/900000000000207008/version/20260901",
  "name": "SNOMED CT Concept 74400008 and descendants",
  "description": "All SNOMED CT concepts for Appendicitis",
  "copyright": "This value set includes content from SNOMED CT, which is
     copyright © 2002+ International Health Terminology Standards Development
     Organisation (SNOMED International), and distributed by agreement between
     SNOMED International and HL7. Implementer use of SNOMED CT is not covered
     by this agreement",
  "compose": {"include": [{"system": "http://snomed.info/sct",
                           "version": "http://snomed.info/sct/900000000000207008/version/20260901",
                           "filter": [{"property": "concept", "op": "is-a",
                                       "value": "74400008"}]}]}
}
```

The `copyright` is the page's own text, character for character, in all four
templates it prints. `description` interpolates the preferred term where the
template says "[sctid or preferred description]". `description` and
`copyright` travel with the definition, as `publisher` does, so an expansion
without `includeDefinition` carries the identity and leaves them out.

The page prints no template for the bare `?fhir_vs`, so that form carries
only the two fields every template shares, the edition version and the
copyright, and no invented name.

### Not offered

<a id="b3-detail"></a>**B3. The component, module, and field URI spaces.**

The URI Standard §2 defines `http://snomed.info/id/{sctid}` for components,
`http://snomed.info/module/{sctid}` for modules, and
`http://snomed.info/field/{table}.{field}` for RF2 properties. FerroTERM
emits none of them, and serves nothing at those paths.

That is what the standard expects. §3 Resolving SNOMED CT URIs says
"SNOMED International resolves URIs for concepts from the SNOMED CT
International Edition (of the form `http://snomed.info/id/{SCTID}`) to the
public SNOMED CT browser", so resolution belongs to SNOMED International.
The FHIR page prescribes the form a terminology service uses instead: §.7
says concept model attribute properties "are referred to using their concept
id, rather than their human readable term", which is exactly what FerroTERM
returns. §.6 RDF describes `http://snomed.info/id/[concept-id]` and
`http://snomed.info/scg/[expression]` as the RDF ontological form of a
`system`/`code` pair, which is a representation question rather than a
terminology API one.

<a id="c14-detail"></a>**C14. `normalForm` and `normalFormTerse`.** §.7 defines
five SNOMED properties and all five are served.

```
$lookup code=74400008&property=normalForm
-> === 64572001 |Disease| : { 116676008 |Associated morphology| = … }

$lookup code=74400008&property=normalFormTerse
-> === 64572001 : { 116676008 = … }
```

The page defines the property and not the generation. The generation is a
rendering rather than a computation: the necessary normal form of a
precoordinated concept is what the release's inferred view already holds, and
it is the relationship file that carries it (the SNOMED CT Release File
Specification, Appendix D). So the focus concepts are the concept's inferred
`is a` parents and the attributes are its other inferred rows, both of which
the built index holds as point reads. No reasoner and no supertype walk runs,
and the read costs no more than the properties beside it.

The syntax is the SNOMED CT Compositional Grammar specification §5, the
definition status prefix its §6.7: `===` where the RF2 `definitionStatusId`
says the concept is sufficiently defined, `<<<` where it does not. The prefix
is optional in the grammar with `===` assumed when absent, so a primitive
written without one would assert an equivalence the terminology denies; both
are always written.

Three shapes are ours, because no specification fixes them, and each is marked
where it is decided. **Order**: the glossary says a canonical form arranges
the parts "in a standard order" and never states the order, so this server
sorts focus concepts and attributes by concept id and groups by what they
render to, which makes one concept render one way whatever order the index
held it in. **Term**: §5 admits "the term from any SNOMED CT description that
is associated with the concept", so `normalForm` carries the display this
server would otherwise answer with. **Layout**: no specification mandates or
suggests any, so the separators that the grammar makes optional are written
anyway.

The root of the hierarchy has no inferred parent and the grammar makes a focus
concept mandatory, so it has no expression; the property is absent for it
rather than invented. A request naming no property is answered without either,
which is what "if no properties are specified, the server chooses what to
return" admits.

The silent drop is closed for every code system, not only SNOMED CT. R5 and
R6 bound the `property` parameter to "any property codes defined by this
specification or by the CodeSystem"
(<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>), so a code
outside that set is a client error:

```
$lookup code=74400008&property=nonesuch
-> 400 invalid
   code system `http://snomed.info/sct` defines no property `nonesuch`
```

## What could not be checked

- **The SNOMED on FHIR implementation guide.** Both published locations
  answer `404` (see [Where the expectations come from](#where-the-expectations-come-from)).
  No expectation here is attributed to it, and the FHIR SNOMED CT page
  covers the same ground normatively for R4B.
- **Refusing a release with unmet module dependencies.**
  `.claude/rules/snomed-terminology.md` [S-RF2-4] asks the build to warn or
  refuse rather than serve a partial edition. Both licensed releases have
  consistent module dependency reference sets, so the branch was never
  reached. Checking it needs a deliberately broken release, which is build
  behaviour rather than served behaviour.
- **Full and Delta releases.** Both artifacts were built from the Snapshot.
  The loader takes the release type from its caller and the file naming
  parser handles all three (`crates/rf2/src/file.rs`), and neither of the
  other two was exercised end to end here.
- **Versions other than R4B.** The server bound `/r4b` only in these runs.
  The four version prefixes are covered by
  `app/ferroterm-server/tests/it/ecosystem.rs`, and this pass checked SNOMED
  behaviour rather than version routing.

## An incidental finding

A code system loaded from `FERROTERM__TERMINOLOGY__INDEX` is not readable as a
`CodeSystem` instance, though the server prints an id for it at start, the
capability statement declares `read` and `search-type`, and
`website/book/src/operate/configuration.md` documents both the id and an
instance-level operation path. Every type-level operation answers. This is
not a SNOMED CT expectation, so it holds no row above; it is issue #365.
