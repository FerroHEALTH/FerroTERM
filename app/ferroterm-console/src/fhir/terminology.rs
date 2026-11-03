// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The part of a `TerminologyCapabilities` the overview renders.
//!
//! `GET [base]/metadata?mode=terminology` is the one document that says what a
//! deployment loaded (<https://hl7.org/fhir/R4B/terminologycapabilities.html>).
//! Reading it lives here, outside every component, so the screen is a
//! rendering of a value that plain unit tests can pin.

use std::collections::BTreeMap;

use crate::fhir::release::Reads;

/// The canonical FerroTERM declares a served version's artifact under.
///
/// No FHIR specification records which index a server read, so the server
/// states it as an extension on `codeSystem.version`, which is where it
/// belongs: one artifact holds one code system version, and two editions of
/// one system share a single `codeSystem` entry.
const ARTIFACT_EXTENSION: &str =
    "https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact";

/// The sub-extension carrying the artifact directory's own name.
const ARTIFACT_NAME: &str = "name";

/// The sub-extension carrying the release identifier the build recorded.
const ARTIFACT_RELEASE: &str = "release";

/// The canonical FerroTERM declares one implicit value set form under.
///
/// No element of `TerminologyCapabilities` says which implicit value sets a
/// server resolves, so the server states each form as an extension on
/// `codeSystem.version`: a URL template and what its placeholders take.
const IMPLICIT_EXTENSION: &str = "https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set";

/// The sub-extension carrying the URL template.
const IMPLICIT_PATTERN: &str = "pattern";

/// The sub-extension carrying what the template's placeholders take.
const IMPLICIT_ARGUMENT: &str = "argument";

/// What `GET [base]/metadata?mode=terminology` declares about a served root,
/// read from the `TerminologyCapabilities` its release decoded.
///
/// It carries the overview's cards, one per declared `codeSystem` in the
/// order the server declared them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TerminologyCapabilities {
    /// One card per `TerminologyCapabilities.codeSystem`, unmerged.
    systems: Vec<SystemCard>,
}

/// One code system, as the overview draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SystemCard {
    /// The system canonical, empty when the server declared none.
    pub(crate) url: String,
    /// The declared content mode, absent where the FHIR version has no such
    /// element.
    pub(crate) content: Option<String>,
    /// Whether the server answers subsumption for this system.
    pub(crate) subsumption: Option<bool>,
    /// The served versions, in the order the server declared them.
    pub(crate) versions: Vec<VersionRow>,
}

/// One served version of a code system, as the overview draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VersionRow {
    /// The version identifier, absent when the server declared none.
    pub(crate) code: Option<String>,
    /// Whether an unversioned request resolves to this version.
    pub(crate) is_default: bool,
    /// Whether the server reads the system's compositional grammar.
    pub(crate) compositional: Option<bool>,
    /// The designation languages this version holds.
    pub(crate) languages: Vec<String>,
    /// The filters `$expand` accepts for this version.
    pub(crate) filters: Vec<FilterRow>,
    /// The properties `$lookup` answers for this version.
    pub(crate) properties: Vec<String>,
    /// The artifact this version was read from, when the server read one.
    pub(crate) artifact: Option<Artifact>,
    /// The implicit value set forms this version resolves, in the order the
    /// server declared them.
    pub(crate) implicit_forms: Vec<ImplicitForm>,
}

/// One implicit value set form a served version resolves.
///
/// The viewer reads the template and the placeholders it holds; it never
/// knows which system a template belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImplicitForm {
    /// The URL template, each placeholder in square brackets.
    pub(crate) pattern: String,
    /// What the placeholders take.
    pub(crate) argument: ImplicitArgument,
}

/// What the placeholders of an implicit value set template take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImplicitArgument {
    /// The template has no placeholder and is the value set URL as it stands.
    None,
    /// The one placeholder takes a code of the system.
    Code,
    /// The placeholders take free text.
    Expression,
}

impl ImplicitArgument {
    /// The argument a declared `valueCode` names, or `None` for a code this
    /// viewer does not know.
    fn of(code: &str) -> Option<Self> {
        match code {
            "none" => Some(Self::None),
            "code" => Some(Self::Code),
            "expression" => Some(Self::Expression),
            _ => None,
        }
    }
}

// NOTE: only the composer fills a template, and the reader bundle carries no
// composer, so the filling is editor-only (no FHIR/SNOMED spec governs this: our own design).
impl ImplicitForm {}

/// The built index one served code system version was read from.
///
/// A system a deployment did not build an index for, such as a registry the
/// server carries or a `CodeSystem` resource posted through the API, declares
/// none, which is an ordinary answer rather than a missing one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Artifact {
    /// The artifact directory's own name, without the directories above it.
    pub(crate) name: Option<String>,
    /// The release identifier the offline build recorded for it.
    pub(crate) release: Option<String>,
}

/// One declared filter and the operators it takes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FilterRow {
    /// The filter property.
    pub(crate) code: String,
    /// The operator codes, as the server wrote them.
    pub(crate) operators: Vec<String>,
}

impl TerminologyCapabilities {
    /// The cards the overview renders, one per distinct code system canonical.
    ///
    /// Cards are ordered by canonical, so the rendered list is the same on
    /// every read and every card carries a key no other card shares. A
    /// document that repeats a canonical merges the repeats into one card and
    /// keeps the first declaration of the system-level facts.
    pub(crate) fn cards(&self) -> Vec<SystemCard> {
        let mut cards: BTreeMap<&str, SystemCard> = BTreeMap::new();
        for system in &self.systems {
            merge(
                cards
                    .entry(system.url.as_str())
                    .or_insert_with(|| opened(system)),
                system,
            );
        }
        cards.into_values().collect()
    }

    /// The card for one canonical, or `None` when this root declares no such
    /// system.
    ///
    /// A canonical the document does not name is an ordinary answer: a reader
    /// followed a link, switched FHIR version, or typed an address, and the
    /// screen says so rather than failing.
    pub(crate) fn card(&self, url: &str) -> Option<SystemCard> {
        let mut found: Option<SystemCard> = None;
        for system in self.systems.iter().filter(|system| system.url == url) {
            merge(found.get_or_insert_with(|| opened(system)), system);
        }
        found
    }
}

/// A card carrying `system`'s system-level facts and no version yet.
fn opened(system: &SystemCard) -> SystemCard {
    SystemCard {
        versions: Vec::new(),
        ..system.clone()
    }
}

/// Appends every version `system` declares to `card`, in declaration order.
fn merge(card: &mut SystemCard, system: &SystemCard) {
    card.versions.extend(system.versions.iter().cloned());
}

/// The trimmed text, or `None` when it names nothing.
fn named(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_owned)
}

/// Reads the `TerminologyCapabilities` of the `fhir-types` module `$module`:
/// `$id` is the element a version names itself in, `code` up to R5 and
/// `value` from the R6 ballot
/// (<https://hl7.org/fhir/6.0.0-ballot5/terminologycapabilities.html>), and
/// `$content` reads `codeSystem.content`, which R5 and R6 declare and R4 and
/// R4B do not.
macro_rules! reads_terminology {
    ($module:ident, $id:ident, $content:expr) => {
        impl Reads<TerminologyCapabilities>
            for fhir_types::$module::terminology_capabilities::TerminologyCapabilities
        {
            fn read(&self) -> TerminologyCapabilities {
                use fhir_types::$module::extension::{Extension, ExtensionValue};
                use fhir_types::$module::terminology_capabilities::TerminologyCapabilitiesCodeSystem;
                fn part<'a>(extension: &'a Extension, url: &str) -> Option<&'a ExtensionValue> {
                    extension
                        .extension
                        .iter()
                        .find(|part| part.url == url)
                        .and_then(|part| part.value.as_ref())
                }
                let content: fn(&TerminologyCapabilitiesCodeSystem) -> Option<String> = $content;
                let text = |extension: &Extension, url: &str| match part(extension, url) {
                    Some(ExtensionValue::String(value)) => value.value.clone(),
                    _ => None,
                };
                let code = |extension: &Extension, url: &str| match part(extension, url) {
                    Some(ExtensionValue::Code(value)) => value.value.clone(),
                    _ => None,
                };
                let systems = self
                    .code_system
                    .iter()
                    .map(|system| SystemCard {
                        url: system
                            .uri
                            .as_ref()
                            .and_then(|uri| uri.value.clone())
                            .unwrap_or_default(),
                        content: content(system),
                        subsumption: system.subsumption.as_ref().and_then(|flag| flag.value),
                        versions: system
                            .version
                            .iter()
                            .map(|version| VersionRow {
                                code: named(
                                    version.$id.as_ref().and_then(|id| id.value.as_deref()),
                                ),
                                is_default: version
                                    .is_default
                                    .as_ref()
                                    .and_then(|flag| flag.value)
                                    .unwrap_or_default(),
                                compositional: version.compositional.as_ref().and_then(|flag| flag.value),
                                languages: version
                                    .language
                                    .iter()
                                    .filter_map(|language| language.value.clone())
                                    .collect(),
                                filters: version
                                    .filter
                                    .iter()
                                    .map(|filter| FilterRow {
                                        code: filter.code.value.clone().unwrap_or_default(),
                                        operators: filter
                                            .op
                                            .iter()
                                            .filter_map(|op| op.value.clone())
                                            .collect(),
                                    })
                                    .collect(),
                                properties: version
                                    .property
                                    .iter()
                                    .filter_map(|property| property.value.clone())
                                    .collect(),
                                artifact: version
                                    .extension
                                    .iter()
                                    .find(|extension| extension.url == ARTIFACT_EXTENSION)
                                    .map(|declaration| Artifact {
                                        name: text(declaration, ARTIFACT_NAME),
                                        release: text(declaration, ARTIFACT_RELEASE),
                                    }),
                                // NOTE: no specification governs this: our own design; a form
                                // with no template or an argument this viewer does not know offers
                                // nothing it could fill, so it is left out.
                                implicit_forms: version
                                    .extension
                                    .iter()
                                    .filter(|extension| extension.url == IMPLICIT_EXTENSION)
                                    .filter_map(|declaration| {
                                        Some(ImplicitForm {
                                            pattern: named(text(declaration, IMPLICIT_PATTERN).as_deref())?,
                                            argument: ImplicitArgument::of(
                                                &code(declaration, IMPLICIT_ARGUMENT)?,
                                            )?,
                                        })
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect();
                TerminologyCapabilities { systems }
            }
        }
    };
}

reads_terminology!(r4, code, |_system| None);
reads_terminology!(r4b, code, |_system| None);
reads_terminology!(r5, code, |system| system.content.value.clone());
reads_terminology!(r6, value, |system| system
    .content
    .as_ref()
    .and_then(|content| content.value.clone()));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::version::FhirVersion;

    /// The document `GET /r4/metadata?mode=terminology` answered.
    const R4: &str = include_str!("../../fixtures/terminology-capabilities-r4.json");
    /// The document `GET /r4b/metadata?mode=terminology` answered.
    const R4B: &str = include_str!("../../fixtures/terminology-capabilities-r4b.json");
    /// The document `GET /r5/metadata?mode=terminology` answered.
    const R5: &str = include_str!("../../fixtures/terminology-capabilities-r5.json");
    /// The document `GET /r6/metadata?mode=terminology` answered.
    const R6: &str = include_str!("../../fixtures/terminology-capabilities-r6.json");

    /// Every recorded document, with the version whose root answered it.
    const RECORDED: [(FhirVersion, &str); 4] = [
        (FhirVersion::R4, R4),
        (FhirVersion::R4B, R4B),
        (FhirVersion::R5, R5),
        (FhirVersion::R6, R6),
    ];

    /// `json` decoded at `version`, as the client decodes an answer.
    fn parse(version: FhirVersion, json: &str) -> TerminologyCapabilities {
        version
            .decode(json)
            .expect("the fixture is a valid document at its version")
    }

    /// The members `json` names, in an otherwise minimal document of
    /// `version`.
    fn inline(version: FhirVersion, json: &str) -> TerminologyCapabilities {
        let mut document: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(json).expect("the fixture is a JSON object");
        for (member, value) in [
            ("resourceType", "TerminologyCapabilities"),
            ("status", "active"),
            ("date", "2026-01-01"),
            ("kind", "instance"),
        ] {
            document
                .entry(member)
                .or_insert_with(|| serde_json::Value::from(value));
        }
        parse(version, &serde_json::Value::Object(document).to_string())
    }

    fn card_of<'a>(cards: &'a [SystemCard], url: &str) -> &'a SystemCard {
        cards
            .iter()
            .find(|card| card.url == url)
            .expect("the fixture declares this system")
    }

    #[test]
    fn every_served_version_declares_the_same_systems() {
        let mut seen: Vec<(FhirVersion, Vec<String>)> = Vec::new();
        for (root, document) in RECORDED {
            let urls: Vec<String> = parse(root, document)
                .cards()
                .into_iter()
                .map(|card| card.url)
                .collect();
            assert!(
                urls.len() > 1,
                "{root} declares more than one system, so the screen is exercised"
            );
            seen.push((root, urls));
        }
        let first = seen.first().expect("four roots were recorded").clone();
        for (root, urls) in &seen {
            assert_eq!(
                urls, &first.1,
                "{root} declares the same systems as {}",
                first.0
            );
        }
    }

    #[test]
    fn the_cards_are_ordered_by_canonical_on_every_version() {
        for (root, document) in RECORDED {
            let urls: Vec<String> = parse(root, document)
                .cards()
                .into_iter()
                .map(|card| card.url)
                .collect();
            let mut sorted = urls.clone();
            sorted.sort();
            assert_eq!(urls, sorted, "{root} renders in a stable order");
        }
    }

    #[test]
    fn the_r6_version_identifier_is_read_from_its_renamed_element() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            let versions: Vec<Option<String>> = cards
                .iter()
                .flat_map(|card| card.versions.iter().map(|version| version.code.clone()))
                .collect();
            assert!(
                versions.iter().filter(|code| code.is_some()).count() > 1,
                "{root} names its versions, whichever element it uses"
            );
        }
    }

    #[test]
    fn the_default_version_is_marked() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            for card in &cards {
                assert_eq!(
                    card.versions
                        .iter()
                        .filter(|version| version.is_default)
                        .count(),
                    1,
                    "{root} marks exactly one default for {}",
                    card.url
                );
            }
        }
    }

    #[test]
    fn the_content_mode_is_declared_only_where_the_version_has_the_element() {
        let r4b = parse(FhirVersion::R4B, R4B).cards();
        assert!(
            r4b.iter().all(|card| card.content.is_none()),
            "R4B has no codeSystem.content, so the screen states its absence"
        );
        let r5 = parse(FhirVersion::R5, R5).cards();
        assert!(
            r5.iter().all(|card| card.content.is_some()),
            "R5 declares a content mode for every system"
        );
    }

    #[test]
    fn subsumption_is_read_per_system_rather_than_assumed() {
        let cards = parse(FhirVersion::R5, R5).cards();
        let declared: Vec<Option<bool>> = cards.iter().map(|card| card.subsumption).collect();
        assert!(
            declared.contains(&Some(true)) && declared.contains(&Some(false)),
            "the recorded document has systems on both sides: {declared:?}"
        );
    }

    #[test]
    fn a_version_that_names_no_identifier_reads_as_absent() {
        let cards = parse(FhirVersion::R6, R6).cards();
        let card = card_of(&cards, "urn:ietf:bcp:13");
        assert_eq!(
            card.versions
                .first()
                .and_then(|version| version.code.clone()),
            None,
            "an empty version string names nothing, so the screen states the absence"
        );
    }

    #[test]
    fn an_r6_version_that_names_itself_in_the_r5_element_is_refused() {
        let error = FhirVersion::R6
            .decode::<TerminologyCapabilities>(
                r#"{"resourceType":"TerminologyCapabilities","status":"active",
                    "date":"2026-01-01","kind":"instance","codeSystem":[{
                    "uri":"https://terminology.example/x","version":[{"code":"2.0"}]}]}"#,
            )
            .expect_err("R6 renamed `code` to `value`, so `code` is a member R6 does not define");
        assert!(error.to_string().contains("code"), "{error}");
    }

    #[test]
    fn a_version_that_declares_no_language_reads_as_an_empty_list() {
        let cards = parse(FhirVersion::R4B, R4B).cards();
        let card = card_of(&cards, "urn:ietf:bcp:47");
        assert_eq!(
            card.versions.first().map(|version| version.languages.len()),
            Some(0),
            "the screen states the absence rather than drawing an empty row"
        );
    }

    #[test]
    fn a_declared_filter_keeps_its_operators() {
        let cards = parse(FhirVersion::R5, R5).cards();
        let card = card_of(&cards, "http://example.org/fhir/CodeSystem/animals");
        let filters = &card
            .versions
            .first()
            .expect("the fixture serves one version")
            .filters;
        let legs = filters
            .iter()
            .find(|filter| filter.code == "legs")
            .expect("the fixture declares a system-specific filter");
        assert_eq!(legs.operators, ["=", "in"]);
    }

    #[test]
    fn an_invented_code_system_renders_with_no_change_here() {
        let document = inline(
            FhirVersion::R5,
            r#"{"codeSystem":[
                {"uri":"https://terminology.example/invented","content":"fragment",
                 "subsumption":false,
                 "version":[{"code":"2031-02-01","isDefault":true,"compositional":true,
                             "language":["cy"],
                             "filter":[{"code":"kind","op":["=","in"]}]}]}]}"#,
        );
        let cards = document.cards();
        let card = card_of(&cards, "https://terminology.example/invented");
        assert_eq!(card.content.as_deref(), Some("fragment"));
        assert_eq!(card.subsumption, Some(false));
        let version = card.versions.first().expect("one version was declared");
        assert_eq!(version.code.as_deref(), Some("2031-02-01"));
        assert!(version.is_default, "the only version is the default");
        assert_eq!(version.compositional, Some(true));
        assert_eq!(version.languages, ["cy"]);
        assert_eq!(
            version.filters.first().map(|filter| filter.code.clone()),
            Some("kind".to_owned()),
            "a system the server has never served before needs no code here"
        );
    }

    #[test]
    fn a_system_that_declares_nothing_still_renders_a_card() {
        let cards = inline(
            FhirVersion::R4B,
            r#"{"codeSystem":[{"uri":"https://terminology.example/bare"}]}"#,
        )
        .cards();
        let card = card_of(&cards, "https://terminology.example/bare");
        assert_eq!(card.content, None);
        assert_eq!(card.subsumption, None);
        assert!(
            card.versions.is_empty(),
            "the screen states that no version was declared"
        );
    }

    #[test]
    fn a_system_without_a_canonical_is_kept_rather_than_dropped() {
        let cards = inline(
            FhirVersion::R4B,
            r#"{"codeSystem":[{"version":[{"code":"1"}]}]}"#,
        )
        .cards();
        assert_eq!(cards.len(), 1, "a card the reader can see beats a silence");
        assert_eq!(
            cards.first().map(|card| card.url.clone()),
            Some(String::new()),
            "the screen states that the server named no canonical"
        );
    }

    #[test]
    fn a_repeated_canonical_merges_into_one_card() {
        let cards = inline(
            FhirVersion::R4B,
            r#"{"codeSystem":[
                {"uri":"https://terminology.example/twice","version":[{"code":"1"}]},
                {"uri":"https://terminology.example/twice","version":[{"code":"2"}]}]}"#,
        )
        .cards();
        assert_eq!(
            cards.len(),
            1,
            "one canonical is one card, so keys are unique"
        );
        assert_eq!(
            cards
                .first()
                .map(|card| card.versions.len())
                .unwrap_or_default(),
            2,
            "no declared version is dropped by the merge"
        );
    }

    #[test]
    fn every_version_read_from_an_artifact_names_it_on_every_fhir_version() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            let card = card_of(&cards, "http://loinc.org");
            let version = card.versions.first().expect("one version is served");
            let artifact = version
                .artifact
                .as_ref()
                .expect("the recorded server read this system from an artifact");
            assert_eq!(
                (artifact.name.as_deref(), artifact.release.as_deref()),
                (Some("loinc"), Some("2.99")),
                "{root} names the artifact and the release the build recorded"
            );
        }
    }

    #[test]
    fn a_system_that_came_from_no_artifact_declares_none() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            let card = card_of(&cards, "urn:iso:std:iso:3166");
            assert!(
                card.versions
                    .iter()
                    .all(|version| version.artifact.is_none()),
                "{root}: a registry the server carries is loaded from no artifact"
            );
        }
    }

    #[test]
    fn an_artifact_that_names_no_directory_still_names_its_release() {
        let cards = inline(
            FhirVersion::R4B,
            &format!(
                r#"{{"codeSystem":[{{"uri":"https://terminology.example/x","version":[{{"code":"1",
                "extension":[{{"url":"{ARTIFACT_EXTENSION}",
                  "extension":[{{"url":"{ARTIFACT_RELEASE}","valueString":"20260630"}}]}}]}}]}}]}}"#
            ),
        )
        .cards();
        let artifact = card_of(&cards, "https://terminology.example/x")
            .versions
            .first()
            .and_then(|version| version.artifact.clone())
            .expect("the version declares an artifact");
        assert_eq!(artifact.name, None, "the screen states the absence");
        assert_eq!(artifact.release.as_deref(), Some("20260630"));
    }

    #[test]
    fn the_lookup_properties_are_read_per_version_on_every_fhir_version() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            let card = card_of(&cards, "http://example.org/fhir/CodeSystem/animals");
            let version = card
                .versions
                .first()
                .expect("the fixture serves one version");
            assert!(
                version.properties.contains(&"legs".to_owned()),
                "{root} declares the system's own `$lookup` property"
            );
        }
    }

    #[test]
    fn a_version_that_declares_no_property_reads_as_an_empty_list() {
        let cards = inline(
            FhirVersion::R4B,
            r#"{"codeSystem":[{"uri":"https://terminology.example/bare",
            "version":[{"code":"1"}]}]}"#,
        )
        .cards();
        assert_eq!(
            card_of(&cards, "https://terminology.example/bare")
                .versions
                .first()
                .map(|version| version.properties.len()),
            Some(0),
            "the screen states the absence rather than drawing an empty list"
        );
    }

    #[test]
    fn a_canonical_the_document_names_is_found_as_one_card() {
        let document = parse(FhirVersion::R5, R5);
        let found = document
            .card("http://example.org/fhir/CodeSystem/animals")
            .expect("the fixture declares this system");
        assert_eq!(found.url, "http://example.org/fhir/CodeSystem/animals");
        assert!(!found.versions.is_empty(), "the card carries its versions");
    }

    #[test]
    fn a_repeated_canonical_is_one_card_whether_it_is_looked_up_or_listed() {
        let document = inline(
            FhirVersion::R5,
            r#"{"codeSystem":[
                {"uri":"https://terminology.example/twice","content":"complete",
                 "version":[{"code":"1"}]},
                {"uri":"https://terminology.example/twice","content":"complete",
                 "version":[{"code":"2"}]}]}"#,
        );
        assert_eq!(
            document.card("https://terminology.example/twice"),
            document
                .cards()
                .into_iter()
                .find(|card| card.url == "https://terminology.example/twice"),
            "one canonical reads the same whichever way the screen asks for it"
        );
    }

    #[test]
    fn a_canonical_the_document_does_not_name_is_absent_rather_than_an_error() {
        assert_eq!(
            parse(FhirVersion::R5, R5).card("https://terminology.example/never-served"),
            None,
            "the screen states the absence, and does not render an error page"
        );
    }

    #[test]
    fn a_document_with_no_code_system_reads_as_no_cards() {
        assert!(
            inline(FhirVersion::R4B, "{}").cards().is_empty(),
            "the screen then says the server declared no code system"
        );
    }

    fn form(pattern: &str, argument: ImplicitArgument) -> ImplicitForm {
        ImplicitForm {
            pattern: pattern.to_owned(),
            argument,
        }
    }

    #[test]
    fn every_declared_implicit_form_is_read_on_every_fhir_version() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            let forms = &card_of(&cards, "http://unitsofmeasure.org")
                .versions
                .first()
                .expect("one version is served")
                .implicit_forms;
            assert_eq!(
                forms,
                &[
                    form("http://unitsofmeasure.org/vs", ImplicitArgument::None),
                    form(
                        "http://unitsofmeasure.org/vs/[expression]",
                        ImplicitArgument::Expression
                    ),
                ],
                "{root} declares both forms in order"
            );
        }
    }

    #[test]
    fn a_system_that_resolves_no_implicit_value_set_declares_no_form() {
        for (root, document) in RECORDED {
            let cards = parse(root, document).cards();
            let card = card_of(&cards, "urn:iso:std:iso:3166");
            assert!(
                card.versions
                    .iter()
                    .all(|version| version.implicit_forms.is_empty()),
                "{root}: the registry declares no implicit value set"
            );
        }
    }

    #[test]
    fn a_form_with_an_unknown_argument_or_no_pattern_is_left_out() {
        let cards = inline(FhirVersion::R4B, &format!(
            r#"{{"codeSystem":[{{"uri":"https://terminology.example/x","version":[{{"code":"1",
                "extension":[
                  {{"url":"{IMPLICIT_EXTENSION}","extension":[
                    {{"url":"{IMPLICIT_PATTERN}","valueString":"https://terminology.example/x/vs"}},
                    {{"url":"{IMPLICIT_ARGUMENT}","valueCode":"sometimes"}}]}},
                  {{"url":"{IMPLICIT_EXTENSION}","extension":[
                    {{"url":"{IMPLICIT_ARGUMENT}","valueCode":"none"}}]}},
                  {{"url":"{IMPLICIT_EXTENSION}","extension":[
                    {{"url":"{IMPLICIT_PATTERN}","valueString":"https://terminology.example/x/vs/[c]"}},
                    {{"url":"{IMPLICIT_ARGUMENT}","valueCode":"code"}}]}}]}}]}}]}}"#
        ))
        .cards();
        let forms = &card_of(&cards, "https://terminology.example/x")
            .versions
            .first()
            .expect("one version")
            .implicit_forms;
        assert_eq!(
            forms,
            &[form(
                "https://terminology.example/x/vs/[c]",
                ImplicitArgument::Code
            )]
        );
    }
}
