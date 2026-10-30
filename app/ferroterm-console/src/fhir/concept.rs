// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reads the concept browser makes, and the answers it draws.
//!
//! One concept is read with `CodeSystem/$lookup`
//! (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>). Concepts are
//! selected with `ValueSet/$expand` over a value set the browser sends inline,
//! which the operation declares as the `valueSet` input: "the value set is
//! provided directly as part of the request"
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). Sending the
//! selection inline is what lets the screen browse any served system, because
//! an implicit value set canonical is a form each code system defines for
//! itself and naming one here would make the screen know a system.
//!
//! Reading lives here, outside every component, so the screen renders values
//! plain unit tests can pin.

use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Number;
use serde_json::Value;

use crate::fhir::expansion::COUNT_PARAMETER;
use crate::fhir::expansion::DISPLAY_LANGUAGE_PARAMETER;
use crate::fhir::expansion::DesignationRow;
use crate::fhir::expansion::FILTER_PARAMETER;
use crate::fhir::terminology::SystemCard;
use crate::fhir::terminology::VersionRow;
use crate::locale::{Msg, tr};
use crate::url::RequestUrl;

/// The `$lookup` parameter naming the code system.
pub(crate) const SYSTEM_PARAMETER: &str = "system";

/// The `$lookup` parameter naming the code.
pub(crate) const CODE_PARAMETER: &str = "code";

/// The `$lookup` parameter naming the code system version.
pub(crate) const VERSION_PARAMETER: &str = "version";

/// The `$lookup` output carrying one designation of the concept.
const DESIGNATION_OUTPUT: &str = "designation";

/// The `$lookup` output carrying one property of the concept.
const PROPERTY_OUTPUT: &str = "property";

/// The concept property naming a direct parent of a concept.
///
/// `parent` and `child` are the standard concept properties any code system
/// may declare (<http://hl7.org/fhir/concept-properties>), so reading them is
/// reading the specification rather than knowing a system.
const PARENT_PROPERTY: &str = "parent";

/// The concept property naming a direct child of a concept.
const CHILD_PROPERTY: &str = "child";

/// The filter operator that selects the direct children of one code.
///
/// "child-of: Includes all concept ids that have a direct parent that matches"
/// (<https://hl7.org/fhir/R5/codesystem-filter-operator.html>). It is the one
/// operator that answers a single level of a hierarchy, which is what a tree
/// walks.
pub(crate) const CHILD_OF_OPERATOR: &str = "child-of";

/// The filter operators whose meaning is the code system's own hierarchy.
///
/// The other operators in `filter-operator` select on the value of a property.
/// These six are defined against a parent-and-child relationship, so a version
/// declaring one of them declares that it has a hierarchy.
const HIERARCHY_OPERATORS: [&str; 6] = [
    CHILD_OF_OPERATOR,
    "descendent-leaf",
    "descendent-of",
    "generalizes",
    "is-a",
    "is-not-a",
];

/// The `$expand` parameter carrying the value set to expand.
pub(crate) const VALUE_SET_PARAMETER: &str = "valueSet";

/// The `$expand` parameter that asks for a flat answer.
///
/// `excludeNested` controls whether the expansion nests codes under
/// `expansion.contains.contains`
/// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). The browser
/// asks for one level at a time and reads the answer as a list, so a nested
/// answer would hide a concept behind a parent the reader never opened.
const EXCLUDE_NESTED_PARAMETER: &str = "excludeNested";

/// What one served version declares about walking its hierarchy.
///
/// The browser draws a tree for a version that declares an operator selecting
/// direct children, and states what it found for one that does not. Nothing
/// here asks which code system it is looking at.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Hierarchy {
    /// The version declares no hierarchy filter operator, so its concepts are
    /// a flat list.
    #[default]
    Absent,
    /// It declares hierarchy operators, but none that selects direct children.
    WithoutChildren {
        /// The hierarchy operators it does declare, in order.
        operators: Vec<String>,
    },
    /// It declares an operator that selects the direct children of a code.
    Walkable {
        /// The filter property the operator is declared on.
        property: String,
    },
}

impl Hierarchy {
    /// What `row` declares about its hierarchy.
    ///
    /// The first filter declaring the direct-child operator carries the walk,
    /// because the property it is declared on is the property the request has
    /// to name.
    pub(crate) fn of(row: &VersionRow) -> Self {
        let mut operators: Vec<String> = Vec::new();
        let mut walk: Option<&str> = None;
        for filter in &row.filters {
            for operator in &filter.operators {
                if !HIERARCHY_OPERATORS.contains(&operator.as_str()) {
                    continue;
                }
                if !operators.contains(operator) {
                    operators.push(operator.clone());
                }
                if operator == CHILD_OF_OPERATOR && walk.is_none() && !filter.code.is_empty() {
                    walk = Some(filter.code.as_str());
                }
            }
        }
        match walk {
            Some(property) => Self::Walkable {
                property: property.to_owned(),
            },
            None if operators.is_empty() => Self::Absent,
            None => {
                operators.sort();
                Self::WithoutChildren { operators }
            }
        }
    }

    /// The filter property a child walk names, when the version declares one.
    pub(crate) fn walk(&self) -> Option<&str> {
        match self {
            Self::Walkable { property } => Some(property.as_str()),
            Self::Absent | Self::WithoutChildren { .. } => None,
        }
    }
}

/// The version of `card` the browser reads.
///
/// An address naming a version reads that version and no other, so a link a
/// reader shared shows what they were looking at. An address naming none takes
/// the version an unversioned request resolves to, and falls back to the first
/// declared version where the server marks no default.
pub(crate) fn chosen_version(card: &SystemCard, wanted: Option<&str>) -> Option<VersionRow> {
    match wanted.map(str::trim).filter(|wanted| !wanted.is_empty()) {
        Some(wanted) => card
            .versions
            .iter()
            .find(|row| row.code.as_deref() == Some(wanted))
            .cloned(),
        None => card
            .versions
            .iter()
            .find(|row| row.is_default)
            .or_else(|| card.versions.first())
            .cloned(),
    }
}

/// The parameters one `CodeSystem/$lookup` sends.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct LookupRequest {
    /// The canonical of the code system the code belongs to.
    pub(crate) system: String,
    /// The code system version, when the address names one.
    pub(crate) system_version: Option<String>,
    /// The code to read.
    pub(crate) code: String,
    /// The BCP 47 tag displays are wanted in.
    pub(crate) display_language: Option<String>,
}

impl LookupRequest {
    /// Appends every parameter that was set to the operation's address.
    ///
    /// No `property` is sent, so the server answers the properties it holds
    /// rather than the ones this screen thought to ask for.
    pub(crate) fn append(&self, url: RequestUrl) -> RequestUrl {
        let mut url = url
            .query(SYSTEM_PARAMETER, &self.system)
            .query(CODE_PARAMETER, &self.code);
        if let Some(version) = &self.system_version {
            url = url.query(VERSION_PARAMETER, version);
        }
        if let Some(language) = &self.display_language {
            url = url.query(DISPLAY_LANGUAGE_PARAMETER, language);
        }
        url
    }
}

/// The selection one inline `ValueSet/$expand` asks for.
///
/// The same shape carries both selections the browser makes: a text search
/// over the whole system, and one level of children below a code.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConceptQuery {
    /// The canonical of the code system to select from.
    pub(crate) system: String,
    /// The code system version, when the address names one.
    pub(crate) system_version: Option<String>,
    /// The text filter, when the reader typed one.
    pub(crate) filter: Option<String>,
    /// The direct-child filter, when the selection is one level of a tree.
    pub(crate) child_of: Option<ChildOf>,
    /// The BCP 47 tag displays are wanted in.
    pub(crate) display_language: Option<String>,
    /// How many concepts one answer may hold.
    pub(crate) count: u32,
}

/// The direct children of one code, as a `compose.include.filter`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ChildOf {
    /// The filter property the version declares the operator on.
    pub(crate) property: String,
    /// The code whose children are wanted.
    pub(crate) code: String,
}

impl ConceptQuery {
    /// The `Parameters` body this selection sends.
    ///
    /// `Parameters` is the body of an operation invoked by `POST`
    /// (<https://hl7.org/fhir/R4B/operations.html#request>), and the value set
    /// travels in it as a resource rather than as a canonical, so no code
    /// system's own implicit form is named.
    pub(crate) fn body(&self) -> String {
        let mut parameters = vec![resource_parameter(VALUE_SET_PARAMETER, self.value_set())];
        if let Some(filter) = &self.filter {
            parameters.push(value_parameter(
                FILTER_PARAMETER,
                "valueString",
                Value::String(filter.clone()),
            ));
        }
        parameters.push(value_parameter(
            COUNT_PARAMETER,
            "valueInteger",
            Value::Number(Number::from(self.count)),
        ));
        parameters.push(value_parameter(
            EXCLUDE_NESTED_PARAMETER,
            "valueBoolean",
            Value::Bool(true),
        ));
        if let Some(language) = &self.display_language {
            parameters.push(value_parameter(
                DISPLAY_LANGUAGE_PARAMETER,
                "valueCode",
                Value::String(language.clone()),
            ));
        }
        let mut body = Map::new();
        body.insert(
            String::from("resourceType"),
            Value::String(tr(
                &ferrohealth_ui::i18n::Locale::english(),
                Msg::Parameters,
            )),
        );
        body.insert(String::from("parameter"), Value::Array(parameters));
        Value::Object(body).to_string()
    }

    /// The value set this selection sends, as one include over one system.
    fn value_set(&self) -> Value {
        let mut include = Map::new();
        include.insert(String::from("system"), Value::String(self.system.clone()));
        if let Some(version) = &self.system_version {
            include.insert(String::from("version"), Value::String(version.clone()));
        }
        if let Some(child) = &self.child_of {
            include.insert(String::from("filter"), Value::Array(vec![child.clause()]));
        }
        let mut compose = Map::new();
        compose.insert(
            String::from("include"),
            Value::Array(vec![Value::Object(include)]),
        );
        let mut value_set = Map::new();
        value_set.insert(
            String::from("resourceType"),
            Value::String(String::from("ValueSet")),
        );
        // NOTE: `ValueSet.status` is 1..1, so the inline resource carries the
        // one status a request-scoped value set can have
        // (<https://hl7.org/fhir/R4B/valueset.html>).
        value_set.insert(
            String::from("status"),
            Value::String(String::from("active")),
        );
        value_set.insert(String::from("compose"), Value::Object(compose));
        Value::Object(value_set)
    }
}

impl ChildOf {
    /// This walk as one `compose.include.filter`.
    fn clause(&self) -> Value {
        let mut filter = Map::new();
        filter.insert(
            String::from("property"),
            Value::String(self.property.clone()),
        );
        filter.insert(
            String::from("op"),
            Value::String(String::from(CHILD_OF_OPERATOR)),
        );
        filter.insert(String::from("value"), Value::String(self.code.clone()));
        Value::Object(filter)
    }
}

/// One `Parameters.parameter` carrying a resource.
pub(crate) fn resource_parameter(name: &str, resource: Value) -> Value {
    let mut parameter = Map::new();
    parameter.insert(String::from("name"), Value::String(name.to_owned()));
    parameter.insert(String::from("resource"), resource);
    Value::Object(parameter)
}

/// One `Parameters.parameter` carrying a primitive value.
pub(crate) fn value_parameter(name: &str, value_name: &str, value: Value) -> Value {
    let mut parameter = Map::new();
    parameter.insert(String::from("name"), Value::String(name.to_owned()));
    parameter.insert(value_name.to_owned(), value);
    Value::Object(parameter)
}

/// The `Parameters` a `$lookup` answers, as the viewer reads it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct LookupAnswer {
    /// `Parameters.parameter`, one per answered output.
    #[serde(default)]
    parameter: Vec<WireParameter>,
}

/// One `Parameters.parameter`, with every `value[x]` the outputs use.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireParameter {
    /// `parameter.name`.
    name: Option<String>,
    /// The parts of a multi-part parameter.
    #[serde(default)]
    part: Vec<WireParameter>,
    /// `valueString`.
    #[serde(rename = "valueString")]
    value_string: Option<String>,
    /// `valueCode`.
    #[serde(rename = "valueCode")]
    value_code: Option<String>,
    /// `valueBoolean`.
    #[serde(rename = "valueBoolean")]
    value_boolean: Option<bool>,
    /// `valueInteger`.
    #[serde(rename = "valueInteger")]
    value_integer: Option<i32>,
    /// `valueDecimal`.
    #[serde(rename = "valueDecimal")]
    value_decimal: Option<Number>,
    /// `valueDateTime`.
    #[serde(rename = "valueDateTime")]
    value_date_time: Option<String>,
    /// `valueUri`.
    #[serde(rename = "valueUri")]
    value_uri: Option<String>,
    /// `valueCoding`.
    #[serde(rename = "valueCoding")]
    value_coding: Option<WireCoding>,
}

/// The parts of a `Coding` a designation's `use` is drawn from.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
struct WireCoding {
    /// The code.
    code: Option<String>,
    /// The display the server sent for it.
    display: Option<String>,
}

/// One concept, as the browser draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Concept {
    /// The code system's name, as the server answered it.
    pub(crate) name: Option<String>,
    /// The code system version the answer came from.
    pub(crate) version: Option<String>,
    /// The display the server chose, in the language that was asked for.
    pub(crate) display: Option<String>,
    /// Every designation the server holds for the concept.
    pub(crate) designations: Vec<DesignationRow>,
    /// Every property the server answers for the concept, in its own order.
    pub(crate) properties: Vec<PropertyRow>,
}

/// One answered property of a concept.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PropertyRow {
    /// The property code, empty when the server named none.
    pub(crate) code: String,
    /// The value, whichever `value[x]` carried it.
    pub(crate) value: Option<String>,
    /// What the property means, when the server said.
    pub(crate) description: Option<String>,
}

impl LookupAnswer {
    /// The concept the answer describes.
    pub(crate) fn concept(&self) -> Concept {
        Concept {
            name: self.first("name"),
            version: self.first(VERSION_PARAMETER),
            display: self.first("display"),
            designations: self
                .named(DESIGNATION_OUTPUT)
                .map(WireParameter::designation)
                .collect(),
            properties: self
                .named(PROPERTY_OUTPUT)
                .map(WireParameter::property)
                .collect(),
        }
    }

    /// Every parameter the answer carries under `name`.
    fn named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a WireParameter> {
        self.parameter
            .iter()
            .filter(move |parameter| parameter.name.as_deref() == Some(name))
    }

    /// The value of the first parameter under `name` that carries one.
    fn first(&self, name: &str) -> Option<String> {
        self.named(name).find_map(WireParameter::value)
    }
}

impl Concept {
    /// The codes this concept declares as its direct parents.
    pub(crate) fn parents(&self) -> Vec<String> {
        self.property_values(PARENT_PROPERTY)
    }

    /// The codes this concept declares as its direct children.
    pub(crate) fn children(&self) -> Vec<String> {
        self.property_values(CHILD_PROPERTY)
    }

    /// Every value the server answered for the property `code`.
    fn property_values(&self, code: &str) -> Vec<String> {
        self.properties
            .iter()
            .filter(|property| property.code == code)
            .filter_map(|property| property.value.clone())
            .collect()
    }
}

impl WireParameter {
    /// The value, from whichever `value[x]` the server used.
    fn value(&self) -> Option<String> {
        self.value_string
            .clone()
            .or_else(|| self.value_code.clone())
            .or_else(|| self.value_uri.clone())
            .or_else(|| self.value_date_time.clone())
            .or_else(|| self.value_boolean.map(|flag| flag.to_string()))
            .or_else(|| self.value_integer.map(|number| number.to_string()))
            .or_else(|| self.value_decimal.as_ref().map(ToString::to_string))
            .or_else(|| self.value_coding.as_ref().and_then(WireCoding::label))
    }

    /// The value of the part named `name`, when the parameter carried one.
    fn part(&self, name: &str) -> Option<String> {
        self.part
            .iter()
            .filter(|part| part.name.as_deref() == Some(name))
            .find_map(WireParameter::value)
    }

    /// This parameter as a designation line.
    fn designation(&self) -> DesignationRow {
        DesignationRow {
            language: self.part("language"),
            usage: self.part("use"),
            value: self.part("value").unwrap_or_default(),
        }
    }

    /// This parameter as a property line.
    fn property(&self) -> PropertyRow {
        PropertyRow {
            code: self.part("code").unwrap_or_default(),
            value: self.part("value"),
            description: self.part("description"),
        }
    }
}

impl WireCoding {
    /// The display the server sent, or the bare code when it sent none.
    fn label(&self) -> Option<String> {
        self.display.clone().or_else(|| self.code.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document `GET /r4b/metadata?mode=terminology` answered.
    const R4B: &str = include_str!("../../fixtures/terminology-capabilities-r4b.json");
    /// The document `GET /r5/metadata?mode=terminology` answered.
    const R5: &str = include_str!("../../fixtures/terminology-capabilities-r5.json");
    /// Every recorded document, named by the root that answered it.
    const RECORDED: [(&str, &str); 4] = [
        (
            "r4",
            include_str!("../../fixtures/terminology-capabilities-r4.json"),
        ),
        ("r4b", R4B),
        ("r5", R5),
        (
            "r6",
            include_str!("../../fixtures/terminology-capabilities-r6.json"),
        ),
    ];

    fn capabilities(json: &str) -> crate::fhir::terminology::TerminologyCapabilities {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    fn hierarchy_of(document: &str, system: &str) -> Hierarchy {
        let card = capabilities(document)
            .card(system)
            .expect("the fixture declares this system");
        let row = chosen_version(&card, None).expect("the fixture serves a version");
        Hierarchy::of(&row)
    }

    fn answer(json: &str) -> LookupAnswer {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    /// The roots whose `filter-operator` value set defines `child-of`.
    ///
    /// R5 added `child-of` and `descendent-leaf` to the value set; the R4
    /// family defines neither
    /// (<https://hl7.org/fhir/R4B/codesystem-filter-operator.html>,
    /// <https://hl7.org/fhir/R5/codesystem-filter-operator.html>). So a root of
    /// the R4 family states a hierarchy it has no way to select a direct child
    /// of, whatever the code system behind it can do.
    const ROOTS_DEFINING_CHILD_OF: [&str; 2] = ["r5", "r6"];

    #[test]
    fn the_tree_is_walkable_exactly_where_the_root_can_express_a_direct_child_filter() {
        for (root, document) in RECORDED {
            let hierarchy = hierarchy_of(document, "http://example.org/fhir/CodeSystem/animals");
            if ROOTS_DEFINING_CHILD_OF.contains(&root) {
                assert_eq!(
                    hierarchy,
                    Hierarchy::Walkable {
                        property: String::from("concept")
                    },
                    "{root} declares the direct-child operator, so the tree walks the property it is declared on"
                );
            } else {
                assert_eq!(
                    hierarchy,
                    Hierarchy::WithoutChildren {
                        operators: vec![
                            String::from("descendent-of"),
                            String::from("generalizes"),
                            String::from("is-a"),
                            String::from("is-not-a"),
                        ]
                    },
                    "{root} states the hierarchy operators it does define, and none of them selects a direct child, so the screen lists rather than walks"
                );
            }
        }
    }

    #[test]
    fn a_version_declaring_no_hierarchy_operator_has_no_tree_on_every_root() {
        for (root, document) in RECORDED {
            assert_eq!(
                hierarchy_of(document, "http://unitsofmeasure.org"),
                Hierarchy::Absent,
                "{root}: a grammar-defined system declares no hierarchy to walk"
            );
        }
    }

    #[test]
    fn a_declared_parent_property_is_not_a_declared_hierarchy_filter() {
        // This system declares the `parent` and `child` concept properties and
        // no hierarchy filter operator, so the tree cannot select a level of it
        // and the screen says so rather than drawing an empty tree.
        let card = capabilities(R5)
            .card("urn:iso:std:iso:3166")
            .expect("the fixture declares this system");
        let row = chosen_version(&card, None).expect("the fixture serves a version");
        assert!(
            row.properties.iter().any(|property| property == "parent"),
            "the version declares the property the tree would read"
        );
        assert_eq!(
            Hierarchy::of(&row),
            Hierarchy::Absent,
            "the pane is gated on the declared filter, which is what a walk needs"
        );
    }

    #[test]
    fn two_systems_with_different_shapes_both_draw_from_the_same_reading() {
        let document = capabilities(R5);
        let walked: Vec<Hierarchy> = ["http://loinc.org", "http://snomed.info/sct"]
            .into_iter()
            .map(|system| {
                let card = document.card(system).expect("the fixture declares it");
                let row = chosen_version(&card, None).expect("a version is served");
                Hierarchy::of(&row)
            })
            .collect();
        assert!(
            walked
                .iter()
                .all(|hierarchy| hierarchy.walk() == Some("concept")),
            "one reading covers both hierarchies: {walked:?}"
        );
    }

    #[test]
    fn a_hierarchy_without_a_child_operator_says_what_it_does_declare() {
        let row = VersionRow {
            filters: vec![crate::fhir::terminology::FilterRow {
                code: String::from("concept"),
                operators: vec![String::from("is-a"), String::from("descendent-of")],
            }],
            ..VersionRow::default()
        };
        assert_eq!(
            Hierarchy::of(&row),
            Hierarchy::WithoutChildren {
                operators: vec![String::from("descendent-of"), String::from("is-a")]
            },
            "the screen names the operators the version declared"
        );
        assert_eq!(
            Hierarchy::of(&row).walk(),
            None,
            "no level of the tree can be selected"
        );
    }

    #[test]
    fn a_child_operator_declared_on_no_property_is_not_a_walk() {
        let row = VersionRow {
            filters: vec![crate::fhir::terminology::FilterRow {
                code: String::new(),
                operators: vec![String::from(CHILD_OF_OPERATOR)],
            }],
            ..VersionRow::default()
        };
        assert_eq!(
            Hierarchy::of(&row).walk(),
            None,
            "a request cannot name a property the server did not"
        );
    }

    #[test]
    fn the_address_names_the_version_it_reads() {
        let card = capabilities(R5)
            .card("http://example.org/fhir/CodeSystem/animals")
            .expect("the fixture declares this system");
        assert_eq!(
            chosen_version(&card, Some("2.0")).and_then(|row| row.code),
            Some(String::from("2.0"))
        );
        assert_eq!(
            chosen_version(&card, Some("1.0")),
            None,
            "a version this root does not serve is stated, not silently replaced"
        );
        assert!(
            chosen_version(&card, Some("  ")).is_some(),
            "an empty version names none, so the default applies"
        );
    }

    #[test]
    fn a_system_that_marks_no_default_reads_its_first_version() {
        let card = SystemCard {
            versions: vec![
                VersionRow {
                    code: Some(String::from("a")),
                    ..VersionRow::default()
                },
                VersionRow {
                    code: Some(String::from("b")),
                    ..VersionRow::default()
                },
            ],
            ..SystemCard::default()
        };
        assert_eq!(
            chosen_version(&card, None).and_then(|row| row.code),
            Some(String::from("a"))
        );
    }

    #[test]
    fn a_lookup_address_carries_the_code_and_the_system_encoded() {
        let request = LookupRequest {
            system: String::from("https://terminology.example/x?edition=2031"),
            system_version: Some(String::from("2.0")),
            code: String::from("m/s2"),
            display_language: Some(String::from("nl-NL")),
        };
        assert_eq!(
            request.append(RequestUrl::new()).render(""),
            "?system=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031&code=m%2Fs2\
             &version=2.0&displayLanguage=nl-NL",
            "a code carrying a separator stays inside the parameter it belongs to"
        );
    }

    #[test]
    fn a_lookup_address_sends_only_what_the_address_names() {
        let request = LookupRequest {
            system: String::from("https://terminology.example/x"),
            code: String::from("a"),
            ..LookupRequest::default()
        };
        assert_eq!(
            request.append(RequestUrl::new()).render(""),
            "?system=https%3A%2F%2Fterminology.example%2Fx&code=a",
            "an unset parameter leaves the server's own default in force"
        );
    }

    #[test]
    fn a_search_body_sends_the_system_as_an_inline_value_set() {
        let query = ConceptQuery {
            system: String::from("https://terminology.example/x"),
            system_version: Some(String::from("2.0")),
            filter: Some(String::from("fever")),
            child_of: None,
            display_language: Some(String::from("nl")),
            count: 20,
        };
        assert_eq!(
            query.body(),
            r#"{"parameter":[{"name":"valueSet","resource":{"compose":{"include":[{"system":"https://terminology.example/x","version":"2.0"}]},"resourceType":"ValueSet","status":"active"}},{"name":"filter","valueString":"fever"},{"name":"count","valueInteger":20},{"name":"excludeNested","valueBoolean":true},{"name":"displayLanguage","valueCode":"nl"}],"resourceType":"Parameters"}"#,
            "the selection names the system and no value set canonical"
        );
    }

    #[test]
    fn a_child_body_sends_the_declared_property_and_the_child_operator() {
        let query = ConceptQuery {
            system: String::from("https://terminology.example/x"),
            child_of: Some(ChildOf {
                property: String::from("concept"),
                code: String::from("root"),
            }),
            count: 100,
            ..ConceptQuery::default()
        };
        assert!(
            query
                .body()
                .contains(r#""filter":[{"op":"child-of","property":"concept","value":"root"}]"#),
            "the walk sends the property the version declared: {}",
            query.body()
        );
    }

    #[test]
    fn a_lookup_answer_reads_the_display_and_the_designations() {
        let concept = answer(
            r#"{"resourceType":"Parameters","parameter":[
                {"name":"name","valueString":"Animals"},
                {"name":"version","valueString":"2.0"},
                {"name":"display","valueString":"Kat"},
                {"name":"designation","part":[
                    {"name":"language","valueCode":"nl"},
                    {"name":"use","valueCoding":{"code":"900000000000013009","display":"Synonym"}},
                    {"name":"value","valueString":"Kat"}]},
                {"name":"designation","part":[{"name":"value","valueString":"Cat"}]}]}"#,
        )
        .concept();
        assert_eq!(concept.name.as_deref(), Some("Animals"));
        assert_eq!(concept.version.as_deref(), Some("2.0"));
        assert_eq!(concept.display.as_deref(), Some("Kat"));
        assert_eq!(
            concept.designations.first().map(|row| (
                row.language.clone(),
                row.usage.clone(),
                row.value.clone()
            )),
            Some((
                Some(String::from("nl")),
                Some(String::from("Synonym")),
                String::from("Kat")
            ))
        );
        assert_eq!(
            concept
                .designations
                .get(1)
                .map(|row| (row.language.clone(), row.usage.clone())),
            Some((None, None)),
            "a designation with no language or use is drawn with the absence stated"
        );
    }

    #[test]
    fn a_lookup_answer_reads_every_value_type_a_property_carries() {
        let concept = answer(
            r#"{"parameter":[
                {"name":"property","part":[
                    {"name":"code","valueCode":"inactive"},
                    {"name":"value","valueBoolean":false},
                    {"name":"description","valueString":"Whether the concept is active"}]},
                {"name":"property","part":[
                    {"name":"code","valueCode":"legs"},
                    {"name":"value","valueInteger":4}]},
                {"name":"property","part":[
                    {"name":"code","valueCode":"parent"},
                    {"name":"value","valueCode":"mammal"}]}]}"#,
        )
        .concept();
        assert_eq!(
            concept
                .properties
                .iter()
                .map(|property| (property.code.clone(), property.value.clone()))
                .collect::<Vec<_>>(),
            vec![
                (String::from("inactive"), Some(String::from("false"))),
                (String::from("legs"), Some(String::from("4"))),
                (String::from("parent"), Some(String::from("mammal"))),
            ]
        );
        assert_eq!(
            concept
                .properties
                .first()
                .and_then(|property| property.description.clone()),
            Some(String::from("Whether the concept is active"))
        );
    }

    #[test]
    fn the_hierarchy_links_are_read_from_the_standard_concept_properties() {
        let concept = answer(
            r#"{"parameter":[
                {"name":"property","part":[{"name":"code","valueCode":"parent"},
                    {"name":"value","valueCode":"a"}]},
                {"name":"property","part":[{"name":"code","valueCode":"parent"},
                    {"name":"value","valueCode":"b"}]},
                {"name":"property","part":[{"name":"code","valueCode":"child"},
                    {"name":"value","valueCode":"c"}]}]}"#,
        )
        .concept();
        assert_eq!(
            concept.parents(),
            vec![String::from("a"), String::from("b")],
            "a concept with two parents keeps both, because a hierarchy may be a graph"
        );
        assert_eq!(concept.children(), vec![String::from("c")]);
    }

    #[test]
    fn an_answer_that_carries_nothing_reads_as_a_concept_that_declares_nothing() {
        let concept = answer(r#"{"resourceType":"Parameters"}"#).concept();
        assert_eq!(concept, Concept::default());
        assert!(
            concept.parents().is_empty(),
            "the screen states the absence"
        );
    }
}
