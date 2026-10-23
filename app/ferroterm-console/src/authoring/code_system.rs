// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The form model behind the code system editor.
//!
//! Everything here is plain values and plain functions: what a local
//! `CodeSystem` is while it is being authored, what a lifecycle transition
//! writes, and the FHIR JSON a save sends
//! (<https://hl7.org/fhir/R4B/codesystem.html>). No component reaches into it,
//! which is what lets the rules it encodes be pinned by ordinary unit tests.
//!
//! The lifecycle is the standard concept properties of
//! <https://hl7.org/fhir/R5/codesystem-concept-properties.html>, and nothing
//! else: a status, an inactive flag, and the two dates. The editor never
//! deletes a concept, because a code that has been published has to keep
//! meaning what it meant; retiring it is the change that says so.

use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use crate::fhir::authoring::CODING_SEPARATOR;
use crate::fhir::authoring::StoredCodeSystem;

/// The code system that defines the standard concept properties.
///
/// A property declaration states the formal identifier of the property in
/// `property.uri`, and the standard ones are identified by this system with
/// the property code as its fragment
/// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
const CONCEPT_PROPERTIES: &str = "http://hl7.org/fhir/concept-properties";

/// The `status` property, whose code says where a concept is in its life.
const STATUS: &str = "status";

/// The `inactive` property, which says a concept is no longer to be used.
const INACTIVE: &str = "inactive";

/// The `deprecationDate` property.
const DEPRECATION_DATE: &str = "deprecationDate";

/// The `retirementDate` property.
const RETIREMENT_DATE: &str = "retirementDate";

/// The `code` property type, of the concept property type value set.
const TYPE_CODE: &str = "code";

/// The `boolean` property type.
const TYPE_BOOLEAN: &str = "boolean";

/// The `dateTime` property type.
const TYPE_DATE_TIME: &str = "dateTime";

/// The `Coding` property type.
const TYPE_CODING: &str = "Coding";

/// The `integer` property type.
const TYPE_INTEGER: &str = "integer";

/// The `decimal` property type.
const TYPE_DECIMAL: &str = "decimal";

/// The `string` property type, and the type an undeclared property falls to.
const TYPE_STRING: &str = "string";

/// A stable key for one row of the editor, minted when the row is made.
///
/// A row's key never comes from its position: a `<For>` keyed by position
/// moves the state of the row that shifted into it. It cannot come from the
/// concept code either, because a row exists before its code is typed and two
/// half-typed rows would then share a key. WebAssembly is 32-bit, so the
/// counter is a fixed-size integer.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Key(pub(crate) u32);

/// Mints keys for the rows of one draft.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Keys(u32);

impl Keys {
    /// The next key, which no row of this draft has had.
    pub(crate) fn next(&mut self) -> Key {
        let key = Key(self.0);
        self.0 = self.0.saturating_add(1);
        key
    }
}

/// Where one concept is in its life.
///
/// The four states are the ones the standard `status` property names, and each
/// writes exactly the properties that state implies. A deprecated concept is
/// not inactive: "deprecated but not inactive can still be used, but their use
/// is discouraged" (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Lifecycle {
    /// In use.
    #[default]
    Active,
    /// In use, and not yet settled.
    Experimental,
    /// Still usable, and discouraged, from the date given.
    Deprecated {
        /// `deprecationDate`, a FHIR `dateTime`.
        on: String,
    },
    /// Withdrawn from use, from the date given.
    Retired {
        /// `retirementDate`, a FHIR `dateTime`.
        on: String,
    },
}

impl Lifecycle {
    /// The `status` code this state is written as.
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Experimental => "experimental",
            Self::Deprecated { .. } => "deprecated",
            Self::Retired { .. } => "retired",
        }
    }

    /// The state `status` names, with `on` as its date where it takes one.
    pub(crate) fn of(status: &str, on: &str) -> Self {
        match status {
            "experimental" => Self::Experimental,
            "deprecated" => Self::Deprecated { on: on.to_owned() },
            "retired" => Self::Retired { on: on.to_owned() },
            _active => Self::Active,
        }
    }

    /// The date this state carries, empty where it carries none.
    pub(crate) fn on(&self) -> &str {
        match self {
            Self::Active | Self::Experimental => "",
            Self::Deprecated { on } | Self::Retired { on } => on,
        }
    }

    /// Whether a concept in this state is inactive.
    pub(crate) fn inactive(&self) -> bool {
        matches!(self, Self::Retired { .. })
    }

    /// The standard properties this state writes, in a fixed order.
    ///
    /// Exactly these and no others: the `status` code always, `inactive` only
    /// where the state means it, and the one date the state carries.
    pub(crate) fn properties(&self) -> Vec<Value> {
        let mut written = vec![json!({"code": STATUS, "valueCode": self.status()})];
        if self.inactive() {
            written.push(json!({"code": INACTIVE, "valueBoolean": true}));
        }
        // A state whose date the reader has not typed writes no date: an
        // empty `valueDateTime` is not a FHIR `dateTime`
        // (<https://hl7.org/fhir/R4B/datatypes.html#dateTime>), and the state
        // itself is already in the `status` code.
        let dated = match self {
            Self::Active | Self::Experimental => None,
            Self::Deprecated { on } => Some((DEPRECATION_DATE, on.trim())),
            Self::Retired { on } => Some((RETIREMENT_DATE, on.trim())),
        };
        if let Some((code, on)) = dated.filter(|(_, on)| !on.is_empty()) {
            written.push(json!({"code": code, "valueDateTime": on}));
        }
        written
    }

    /// The property codes the lifecycle owns, whichever state a concept is in.
    ///
    /// A property value the reader typed under one of these names would fight
    /// the lifecycle control for the same element, so the editor keeps them
    /// apart: the control writes them, the property table never shows them.
    pub(crate) fn owned() -> [&'static str; 4] {
        [STATUS, INACTIVE, DEPRECATION_DATE, RETIREMENT_DATE]
    }

    /// The declarations a code system needs for the lifecycle to be read back.
    ///
    /// A property value means nothing until the system declares the property
    /// it names (<https://hl7.org/fhir/R4B/codesystem.html>), so every save
    /// carries these four whatever state the concepts are in.
    pub(crate) fn declarations() -> Vec<Value> {
        [
            (STATUS, TYPE_CODE),
            (INACTIVE, TYPE_BOOLEAN),
            (DEPRECATION_DATE, TYPE_DATE_TIME),
            (RETIREMENT_DATE, TYPE_DATE_TIME),
        ]
        .into_iter()
        .map(|(code, kind)| {
            json!({
                "code": code,
                "uri": format!("{CONCEPT_PROPERTIES}#{code}"),
                "type": kind,
            })
        })
        .collect()
    }
}

/// One property a code system declares.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Declared {
    /// The row's key.
    pub(crate) key: Key,
    /// `property.code`.
    pub(crate) code: String,
    /// `property.uri`.
    pub(crate) uri: String,
    /// `property.type`, a code of the served version's concept property type
    /// value set.
    pub(crate) kind: String,
}

/// One designation of a concept.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Designation {
    /// The row's key.
    pub(crate) key: Key,
    /// `designation.language`, a BCP 47 tag.
    pub(crate) language: String,
    /// `designation.use`, as `system|code`.
    pub(crate) usage: String,
    /// `designation.value`.
    pub(crate) value: String,
}

/// One property value a concept carries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Valued {
    /// The row's key.
    pub(crate) key: Key,
    /// The declared property this value is for.
    pub(crate) code: String,
    /// The value, as the text its control holds.
    pub(crate) text: String,
}

/// One concept of a local code system.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Concept {
    /// The row's key.
    pub(crate) key: Key,
    /// `concept.code`.
    pub(crate) code: String,
    /// `concept.display`.
    pub(crate) display: String,
    /// `concept.definition`.
    pub(crate) definition: String,
    /// Where the concept is in its life.
    pub(crate) lifecycle: Lifecycle,
    /// The designations it carries.
    pub(crate) designations: Vec<Designation>,
    /// The property values it carries, the lifecycle's own apart.
    pub(crate) values: Vec<Valued>,
}

/// A local code system as the editor holds it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Draft {
    /// The logical id the server assigned, empty for one never saved.
    pub(crate) id: String,
    /// `meta.versionId`, the version an update states in `If-Match`.
    pub(crate) version_id: String,
    /// `CodeSystem.url`.
    pub(crate) url: String,
    /// `CodeSystem.version`.
    pub(crate) version: String,
    /// `CodeSystem.title`, the name written for a person.
    pub(crate) title: String,
    /// `CodeSystem.name`, the computer-friendly name.
    pub(crate) name: String,
    /// `CodeSystem.status`.
    pub(crate) status: String,
    /// `CodeSystem.content`.
    pub(crate) content: String,
    /// `CodeSystem.caseSensitive`.
    pub(crate) case_sensitive: bool,
    /// The properties the system declares, the lifecycle's own apart.
    pub(crate) properties: Vec<Declared>,
    /// The concepts it defines.
    pub(crate) concepts: Vec<Concept>,
    /// The keys minted for this draft's rows.
    pub(crate) keys: Keys,
    /// The resource this draft was read from, as the server sent it.
    ///
    /// An update replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>), so a save is this
    /// document with the elements the form owns written over it. Everything
    /// else, from `name` and `text` to a concept's own child concepts, travels
    /// back untouched instead of being deleted by a form that never drew it.
    base: Value,
}

/// The status a new draft opens in.
///
/// A code of the publication status value set, which every FHIR version binds
/// `CodeSystem.status` to as `required`
/// (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.status>).
/// The screen replaces the whole list with the codes the served root expands,
/// so this is only where an unsaved draft starts.
const NEW_STATUS: &str = "draft";

/// The content mode a new draft opens in.
///
/// A code system authored here carries its concepts, which is what `complete`
/// states (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.content>).
const NEW_CONTENT: &str = "complete";

impl Draft {
    /// A draft for a code system that does not exist yet.
    pub(crate) fn new() -> Self {
        Self {
            status: NEW_STATUS.to_owned(),
            content: NEW_CONTENT.to_owned(),
            case_sensitive: true,
            ..Self::default()
        }
    }

    /// The draft for a code system the server already holds.
    pub(crate) fn of(resource: &Value) -> Self {
        let stored = &StoredCodeSystem::of(resource);
        let mut keys = Keys::default();
        let properties = stored
            .property
            .iter()
            .filter(|property| {
                let code = property.code.as_deref().unwrap_or_default();
                !Lifecycle::owned().contains(&code)
            })
            .map(|property| Declared {
                key: keys.next(),
                code: property.code.clone().unwrap_or_default(),
                uri: property.uri.clone().unwrap_or_default(),
                kind: property.kind.clone().unwrap_or_default(),
            })
            .collect();
        let concepts = stored
            .concept
            .iter()
            .map(|concept| {
                let lifecycle = lifecycle_of(concept);
                Concept {
                    key: keys.next(),
                    code: concept.code.clone().unwrap_or_default(),
                    display: concept.display.clone().unwrap_or_default(),
                    definition: concept.definition.clone().unwrap_or_default(),
                    lifecycle,
                    designations: concept
                        .designation
                        .iter()
                        .map(|designation| Designation {
                            key: keys.next(),
                            language: designation.language.clone().unwrap_or_default(),
                            usage: designation
                                .usage
                                .as_ref()
                                .map(|coding| {
                                    let system = coding.system.clone().unwrap_or_default();
                                    let code = coding.code.clone().unwrap_or_default();
                                    format!("{system}{CODING_SEPARATOR}{code}")
                                })
                                .unwrap_or_default(),
                            value: designation.value.clone().unwrap_or_default(),
                        })
                        .collect(),
                    values: concept
                        .property
                        .iter()
                        .filter(|value| {
                            let code = value.code.as_deref().unwrap_or_default();
                            !Lifecycle::owned().contains(&code)
                        })
                        .map(|value| Valued {
                            key: keys.next(),
                            code: value.code.clone().unwrap_or_default(),
                            text: value.text(),
                        })
                        .collect(),
                }
            })
            .collect();
        Self {
            id: stored.id.clone().unwrap_or_default(),
            version_id: stored
                .meta
                .as_ref()
                .and_then(|meta| meta.version_id.clone())
                .unwrap_or_default(),
            url: stored.url.clone().unwrap_or_default(),
            version: stored.version.clone().unwrap_or_default(),
            title: text_of(resource, "title"),
            name: text_of(resource, "name"),
            status: stored.status.clone().unwrap_or_default(),
            content: stored.content.clone().unwrap_or_default(),
            case_sensitive: stored.case_sensitive.unwrap_or(true),
            properties,
            concepts,
            keys,
            base: resource.clone(),
        }
    }

    /// Whether the server states a version for this resource.
    ///
    /// `meta.versionId` is what an update states in `If-Match`
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>), so a resource the
    /// server states none for is one the REST API is not managing, and the
    /// editor shows it rather than offering to replace it.
    pub(crate) fn managed(&self) -> bool {
        !self.version_id.is_empty()
    }

    /// Whether the draft names enough for a save to be worth sending.
    ///
    /// `status` and `content` are the two elements `CodeSystem` makes
    /// mandatory (<https://hl7.org/fhir/R4B/codesystem.html>), and a canonical
    /// is what every later request names it by.
    pub(crate) fn savable(&self) -> bool {
        !self.url.trim().is_empty()
            && !self.status.trim().is_empty()
            && !self.content.trim().is_empty()
    }

    /// The declared type of the property `code` names.
    ///
    /// A value whose property the system does not declare is written as a
    /// string, which is the shape a reader typed it in.
    fn kind_of(&self, code: &str) -> &str {
        self.properties
            .iter()
            .find(|property| property.code == code)
            .map_or(TYPE_STRING, |property| property.kind.as_str())
    }

    /// The FHIR JSON a save sends.
    ///
    /// It starts from the resource this draft was read from, so an element the
    /// form does not draw is carried back rather than deleted by an update
    /// that replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>). The elements the form
    /// owns are written over it: the lifecycle declarations whatever the
    /// concepts carry, so the server can read a status back, and a property
    /// row the reader left unnamed left out rather than sent empty.
    pub(crate) fn body(&self) -> String {
        let mut resource = object(&self.base);
        resource.insert("resourceType".to_owned(), json!("CodeSystem"));
        if self.id.is_empty() {
            resource.remove("id");
        } else {
            resource.insert("id".to_owned(), json!(self.id));
        }
        resource.insert("url".to_owned(), json!(self.url.trim()));
        if self.version.trim().is_empty() {
            resource.remove("version");
        } else {
            resource.insert("version".to_owned(), json!(self.version.trim()));
        }
        set(&mut resource, "title", self.title.trim());
        set(&mut resource, "name", self.name.trim());
        resource.insert("status".to_owned(), json!(self.status));
        resource.insert("content".to_owned(), json!(self.content));
        resource.insert("caseSensitive".to_owned(), json!(self.case_sensitive));

        let mut properties = Lifecycle::declarations();
        for declared in &self.properties {
            let code = declared.code.trim();
            if code.is_empty() {
                continue;
            }
            let mut written = self.held("property", code);
            written.insert("code".to_owned(), json!(code));
            set(&mut written, "uri", declared.uri.trim());
            set(&mut written, "type", declared.kind.trim());
            properties.push(Value::Object(written));
        }
        resource.insert("property".to_owned(), Value::Array(properties));

        let concepts: Vec<Value> = self
            .concepts
            .iter()
            .filter(|concept| !concept.code.trim().is_empty())
            .map(|concept| self.concept_body(concept))
            .collect();
        if concepts.is_empty() {
            resource.remove("concept");
        } else {
            resource.insert("concept".to_owned(), Value::Array(concepts));
        }
        Value::Object(resource).to_string()
    }

    /// The element of `list` the code `code` names, as the server sent it.
    ///
    /// A concept and a property declaration are both identified by their code
    /// (<https://hl7.org/fhir/R4B/codesystem.html>), so that is what an
    /// authored row is matched to the stored one by.
    fn held(&self, list: &str, code: &str) -> Map<String, Value> {
        self.base
            .get(list)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .find(|held| held.get("code").and_then(Value::as_str) == Some(code))
            .cloned()
            .unwrap_or_default()
    }

    /// One concept, as the resource carries it.
    ///
    /// The stored concept of the same code is what this is written over, so a
    /// concept's own child concepts and anything else the form does not draw
    /// survive the save.
    fn concept_body(&self, concept: &Concept) -> Value {
        let code = concept.code.trim();
        let mut written = self.held("concept", code);
        written.insert("code".to_owned(), json!(code));
        set(&mut written, "display", concept.display.trim());
        set(&mut written, "definition", concept.definition.trim());
        let designations: Vec<Value> = concept
            .designations
            .iter()
            .filter(|designation| !designation.value.trim().is_empty())
            .map(designation_body)
            .collect();
        if designations.is_empty() {
            written.remove("designation");
        } else {
            written.insert("designation".to_owned(), Value::Array(designations));
        }
        let mut values = concept.lifecycle.properties();
        for valued in &concept.values {
            if valued.code.trim().is_empty() {
                continue;
            }
            values.push(value_body(
                valued.code.trim(),
                self.kind_of(valued.code.trim()),
                valued.text.trim(),
            ));
        }
        written.insert("property".to_owned(), Value::Array(values));
        Value::Object(written)
    }
}

/// The string element `name` of `resource`, empty when it states none.
fn text_of(resource: &Value, name: &str) -> String {
    resource
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The object `resource` carries, or an empty one when it is not an object.
fn object(resource: &Value) -> Map<String, Value> {
    resource.as_object().cloned().unwrap_or_default()
}

/// Writes `value` under `name`, or removes the element when nothing was typed.
///
/// An element the reader emptied is removed rather than sent as an empty
/// string, which is not a value any of these elements admits.
fn set(written: &mut Map<String, Value>, name: &str, value: &str) {
    if value.is_empty() {
        written.remove(name);
    } else {
        written.insert(name.to_owned(), json!(value));
    }
}

/// One designation, as the resource carries it.
fn designation_body(designation: &Designation) -> Value {
    let mut written = Map::new();
    if !designation.language.trim().is_empty() {
        written.insert("language".to_owned(), json!(designation.language.trim()));
    }
    if let Some(usage) = coding_body(designation.usage.trim()) {
        written.insert("use".to_owned(), usage);
    }
    written.insert("value".to_owned(), json!(designation.value.trim()));
    Value::Object(written)
}

/// A `Coding` written `system|code`, or `None` when nothing was typed.
///
/// FHIR search spells a coding that way
/// (<https://hl7.org/fhir/R4B/search.html#token>), so one field carries both
/// halves. Text with no separator is the code alone, which is how a reader
/// types a coding whose system the resource states elsewhere.
fn coding_body(typed: &str) -> Option<Value> {
    if typed.is_empty() {
        return None;
    }
    let (system, code) = typed
        .split_once(CODING_SEPARATOR)
        .map_or(("", typed), |(system, code)| (system.trim(), code.trim()));
    let mut coding = Map::new();
    if !system.is_empty() {
        coding.insert("system".to_owned(), json!(system));
    }
    if !code.is_empty() {
        coding.insert("code".to_owned(), json!(code));
    }
    if coding.is_empty() {
        None
    } else {
        Some(Value::Object(coding))
    }
}

/// One property value, under the element its declared type names.
///
/// `property.value[x]` is a choice of seven types
/// (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.concept.property.value_x_>),
/// and the declared type decides which one a value is written as. Text that is
/// not of the declared type is written as a string, so the server refuses it
/// in its own words rather than the viewer silently dropping the value.
fn value_body(code: &str, kind: &str, text: &str) -> Value {
    let typed = match kind {
        TYPE_CODE => Some(json!({"code": code, "valueCode": text})),
        TYPE_CODING => coding_body(text).map(|coding| json!({"code": code, "valueCoding": coding})),
        TYPE_BOOLEAN => text
            .parse::<bool>()
            .ok()
            .map(|flag| json!({"code": code, "valueBoolean": flag})),
        TYPE_INTEGER => text
            .parse::<i32>()
            .ok()
            .map(|number| json!({"code": code, "valueInteger": number})),
        TYPE_DECIMAL => text
            .parse::<serde_json::Number>()
            .ok()
            .map(|number| json!({"code": code, "valueDecimal": number})),
        TYPE_DATE_TIME => Some(json!({"code": code, "valueDateTime": text})),
        _string => None,
    };
    typed.unwrap_or_else(|| json!({"code": code, "valueString": text}))
}

/// Where one stored concept is in its life, from the properties it carries.
///
/// The status code leads, because it is the property that names the state;
/// the dates fill in the one the state carries, and a concept with no status
/// but a date reads as the state that date implies.
fn lifecycle_of(concept: &crate::fhir::authoring::StoredConcept) -> Lifecycle {
    let text_of = |wanted: &str| {
        concept
            .property
            .iter()
            .find(|value| value.code.as_deref() == Some(wanted))
            .map(crate::fhir::authoring::StoredValue::text)
            .unwrap_or_default()
    };
    let status = text_of(STATUS);
    let retirement = text_of(RETIREMENT_DATE);
    let deprecation = text_of(DEPRECATION_DATE);
    if status.is_empty() {
        if !retirement.is_empty() {
            return Lifecycle::Retired { on: retirement };
        }
        if !deprecation.is_empty() {
            return Lifecycle::Deprecated { on: deprecation };
        }
        if text_of(INACTIVE) == "true" {
            return Lifecycle::Retired { on: String::new() };
        }
        return Lifecycle::Active;
    }
    let on = match status.as_str() {
        "retired" => retirement,
        "deprecated" => deprecation,
        _other => String::new(),
    };
    Lifecycle::of(&status, &on)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property codes and values one lifecycle writes, in order.
    fn written(lifecycle: &Lifecycle) -> Vec<String> {
        lifecycle
            .properties()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn an_active_concept_carries_its_status_and_nothing_else() {
        assert_eq!(
            written(&Lifecycle::Active),
            vec![r#"{"code":"status","valueCode":"active"}"#.to_owned()],
            "an active concept is neither inactive nor dated"
        );
    }

    #[test]
    fn an_experimental_concept_carries_its_status_and_nothing_else() {
        assert_eq!(
            written(&Lifecycle::Experimental),
            vec![r#"{"code":"status","valueCode":"experimental"}"#.to_owned()]
        );
    }

    /// A deprecated concept "can still be used, but their use is discouraged"
    /// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>), so it is
    /// dated and never flagged inactive.
    #[test]
    fn deprecating_a_concept_dates_it_and_leaves_it_active() {
        let deprecated = Lifecycle::Deprecated {
            on: "2026-09-24".to_owned(),
        };
        assert!(!deprecated.inactive());
        assert_eq!(
            written(&deprecated),
            vec![
                r#"{"code":"status","valueCode":"deprecated"}"#.to_owned(),
                r#"{"code":"deprecationDate","valueDateTime":"2026-09-24"}"#.to_owned(),
            ]
        );
    }

    /// A `dateTime` is a date at minimum, so an empty one is not a value the
    /// element admits (<https://hl7.org/fhir/R4B/datatypes.html#dateTime>).
    /// The state itself is already in the `status` code.
    #[test]
    fn a_state_whose_date_nobody_typed_writes_no_date() {
        assert_eq!(
            written(&Lifecycle::Retired { on: String::new() }),
            vec![
                r#"{"code":"status","valueCode":"retired"}"#.to_owned(),
                r#"{"code":"inactive","valueBoolean":true}"#.to_owned(),
            ]
        );
        assert_eq!(
            written(&Lifecycle::Deprecated {
                on: "  ".to_owned()
            }),
            vec![r#"{"code":"status","valueCode":"deprecated"}"#.to_owned()]
        );
    }

    #[test]
    fn retiring_a_concept_writes_exactly_the_three_standard_properties() {
        let retired = Lifecycle::Retired {
            on: "2026-09-24".to_owned(),
        };
        assert!(retired.inactive());
        assert_eq!(
            written(&retired),
            vec![
                r#"{"code":"status","valueCode":"retired"}"#.to_owned(),
                r#"{"code":"inactive","valueBoolean":true}"#.to_owned(),
                r#"{"code":"retirementDate","valueDateTime":"2026-09-24"}"#.to_owned(),
            ],
            "a retirement is a status, an inactive flag, and the date it took effect"
        );
    }

    #[test]
    fn a_transition_writes_only_the_properties_the_lifecycle_owns() {
        for lifecycle in [
            Lifecycle::Active,
            Lifecycle::Experimental,
            Lifecycle::Deprecated {
                on: "2026-01-01".to_owned(),
            },
            Lifecycle::Retired {
                on: "2026-01-01".to_owned(),
            },
        ] {
            for property in lifecycle.properties() {
                let code = property
                    .get("code")
                    .and_then(Value::as_str)
                    .expect("every property names the code it sets");
                assert!(
                    Lifecycle::owned().contains(&code),
                    "{lifecycle:?} writes `{code}`, which is not a standard concept property"
                );
            }
        }
    }

    #[test]
    fn the_four_standard_properties_are_declared_with_their_own_identifiers() {
        let declared = Lifecycle::declarations();
        assert_eq!(declared.len(), Lifecycle::owned().len());
        for property in &declared {
            let code = property
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert_eq!(
                property.get("uri").and_then(Value::as_str),
                Some(format!("{CONCEPT_PROPERTIES}#{code}").as_str()),
                "a standard property is identified by the system that defines it"
            );
        }
    }

    /// A draft carrying one retired concept, for the body cases below.
    fn drafted() -> Draft {
        let mut draft = Draft::new();
        draft.url = "https://terminology.example/colours".to_owned();
        draft.version = "1.0.0".to_owned();
        let red = draft.keys.next();
        let designation = draft.keys.next();
        let vermilion = draft.keys.next();
        draft.concepts = vec![
            Concept {
                key: red,
                code: "red".to_owned(),
                display: "Red".to_owned(),
                definition: "The colour red".to_owned(),
                lifecycle: Lifecycle::Active,
                designations: vec![Designation {
                    key: designation,
                    language: "nl-NL".to_owned(),
                    usage: "https://terms.example/use|syn".to_owned(),
                    value: "Rood".to_owned(),
                }],
                values: Vec::new(),
            },
            Concept {
                key: vermilion,
                code: "vermilion".to_owned(),
                display: "Vermilion".to_owned(),
                lifecycle: Lifecycle::Retired {
                    on: "2026-09-24".to_owned(),
                },
                ..Concept::default()
            },
        ];
        draft
    }

    /// The body of a draft, as a `Value` to read fields out of.
    fn body_of(draft: &Draft) -> Value {
        serde_json::from_str(&draft.body()).expect("the editor writes JSON")
    }

    #[test]
    fn a_save_writes_the_metadata_the_resource_requires() {
        let body = body_of(&drafted());
        assert_eq!(body["resourceType"], json!("CodeSystem"));
        assert_eq!(body["url"], json!("https://terminology.example/colours"));
        assert_eq!(body["version"], json!("1.0.0"));
        assert_eq!(body["status"], json!("draft"));
        assert_eq!(body["content"], json!("complete"));
        assert_eq!(body["caseSensitive"], json!(true));
        assert_eq!(
            body.get("id"),
            None,
            "a draft the server has never seen names no logical id"
        );
    }

    #[test]
    fn a_save_declares_the_lifecycle_properties_whatever_the_concepts_carry() {
        let mut draft = drafted();
        draft.concepts.clear();
        let body = body_of(&draft);
        let declared: Vec<&str> = body["property"]
            .as_array()
            .expect("the declarations are an array")
            .iter()
            .filter_map(|property| property.get("code").and_then(Value::as_str))
            .collect();
        assert_eq!(
            declared,
            Lifecycle::owned().to_vec(),
            "a status the system does not declare cannot be read back"
        );
    }

    #[test]
    fn a_retired_concept_reaches_the_wire_as_the_standard_properties() {
        let body = body_of(&drafted());
        let concepts = body["concept"]
            .as_array()
            .expect("the concepts are an array");
        let retired = concepts
            .iter()
            .find(|concept| concept["code"] == json!("vermilion"))
            .expect("the retired concept is written");
        assert_eq!(
            retired["property"],
            json!([
                {"code": "status", "valueCode": "retired"},
                {"code": "inactive", "valueBoolean": true},
                {"code": "retirementDate", "valueDateTime": "2026-09-24"},
            ])
        );
    }

    #[test]
    fn a_designation_carries_its_language_its_use_and_its_value() {
        let body = body_of(&drafted());
        let concepts = body["concept"]
            .as_array()
            .expect("the concepts are an array");
        assert_eq!(
            concepts[0]["designation"],
            json!([{
                "language": "nl-NL",
                "use": {"system": "https://terms.example/use", "code": "syn"},
                "value": "Rood",
            }])
        );
    }

    #[test]
    fn a_half_typed_row_is_left_out_rather_than_sent_empty() {
        let mut draft = drafted();
        let key = draft.keys.next();
        draft.concepts.push(Concept {
            key,
            ..Concept::default()
        });
        let property = draft.keys.next();
        draft.properties.push(Declared {
            key: property,
            ..Declared::default()
        });
        let body = body_of(&draft);
        assert_eq!(
            body["concept"].as_array().map(Vec::len),
            Some(2),
            "a concept with no code is not a concept yet"
        );
        assert_eq!(
            body["property"].as_array().map(Vec::len),
            Some(Lifecycle::owned().len()),
            "a declaration with no code is not a declaration yet"
        );
    }

    #[test]
    fn a_property_value_is_written_under_the_element_its_declared_type_names() {
        for (kind, text, expected) in [
            ("code", "abc", json!({"code": "p", "valueCode": "abc"})),
            (
                "Coding",
                "https://x.example/s|c",
                json!({"code": "p", "valueCoding": {"system": "https://x.example/s", "code": "c"}}),
            ),
            (
                "string",
                "a phrase",
                json!({"code": "p", "valueString": "a phrase"}),
            ),
            ("integer", "-3", json!({"code": "p", "valueInteger": -3})),
            (
                "boolean",
                "true",
                json!({"code": "p", "valueBoolean": true}),
            ),
            (
                "dateTime",
                "2026-09-24",
                json!({"code": "p", "valueDateTime": "2026-09-24"}),
            ),
            ("decimal", "1.5", json!({"code": "p", "valueDecimal": 1.5})),
        ] {
            assert_eq!(value_body("p", kind, text), expected, "{kind}");
        }
    }

    #[test]
    fn a_value_that_is_not_of_its_declared_type_reaches_the_server_as_typed() {
        assert_eq!(
            value_body("p", "integer", "seven"),
            json!({"code": "p", "valueString": "seven"}),
            "the server refuses it in its own words rather than the value being dropped"
        );
    }

    #[test]
    fn a_stored_code_system_round_trips_into_the_form_and_back_out() {
        let stored: Value = serde_json::from_str(
            r#"{"resourceType":"CodeSystem","id":"colours","meta":{"versionId":"4"},
                "url":"https://terminology.example/colours","version":"1.0.0",
                "status":"active","content":"complete","caseSensitive":false,
                "property":[
                  {"code":"status","uri":"http://hl7.org/fhir/concept-properties#status","type":"code"},
                  {"code":"hue","uri":"https://terminology.example/hue","type":"integer"}],
                "concept":[
                  {"code":"red","display":"Red","property":[
                    {"code":"hue","valueInteger":0},
                    {"code":"status","valueCode":"retired"},
                    {"code":"inactive","valueBoolean":true},
                    {"code":"retirementDate","valueDateTime":"2026-09-24"}]}]}"#,
        )
        .expect("the server's own answer parses");
        let draft = Draft::of(&stored);
        assert_eq!(draft.id, "colours");
        assert_eq!(draft.version_id, "4");
        assert!(draft.managed());
        assert!(!draft.case_sensitive);
        assert_eq!(
            draft.properties.len(),
            1,
            "the lifecycle's own declarations belong to the lifecycle control"
        );
        assert_eq!(draft.properties[0].code, "hue");
        assert_eq!(draft.concepts[0].values.len(), 1, "and so do its values");
        assert_eq!(draft.concepts[0].values[0].text, "0");
        assert_eq!(
            draft.concepts[0].lifecycle,
            Lifecycle::Retired {
                on: "2026-09-24".to_owned()
            }
        );

        let body = body_of(&draft);
        assert_eq!(body["id"], json!("colours"));
        assert_eq!(
            body["concept"][0]["property"],
            json!([
                {"code": "status", "valueCode": "retired"},
                {"code": "inactive", "valueBoolean": true},
                {"code": "retirementDate", "valueDateTime": "2026-09-24"},
                {"code": "hue", "valueInteger": 0},
            ]),
            "the value keeps the type its declaration names"
        );
    }

    /// A FHIR update replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>), so every element the form
    /// does not draw has to reach the server again. A form that rebuilt the
    /// resource from what it drew would delete the rest of it silently, which
    /// is the failure class this case exists to catch.
    #[test]
    fn a_save_carries_back_every_element_the_form_does_not_draw() {
        let stored: Value = serde_json::from_str(
            r#"{"resourceType":"CodeSystem","id":"colours","meta":{"versionId":"4"},
                "url":"https://terminology.example/colours","version":"1.0.0",
                "name":"Colours","title":"The colours","publisher":"Someone",
                "hierarchyMeaning":"is-a","experimental":false,
                "text":{"status":"generated","div":"<div>a narrative</div>"},
                "status":"active","content":"complete","caseSensitive":true,
                "property":[{"code":"hue","uri":"https://terminology.example/hue",
                             "type":"integer","description":"the hue angle"}],
                "concept":[{"code":"red","display":"Red",
                  "property":[{"code":"hue","valueInteger":0}],
                  "concept":[{"code":"crimson","display":"Crimson"}]}]}"#,
        )
        .expect("the server's own answer parses");
        let draft = Draft::of(&stored);
        let body = body_of(&draft);

        for element in [
            "name",
            "title",
            "publisher",
            "hierarchyMeaning",
            "experimental",
            "text",
            "meta",
        ] {
            assert_eq!(
                body.get(element),
                stored.get(element),
                "`{element}` is not an element the form draws, so a save keeps it"
            );
        }
        assert_eq!(
            body["property"][4]["description"],
            json!("the hue angle"),
            "a declaration keeps what the form does not draw of it"
        );
        assert_eq!(
            body["concept"][0]["concept"],
            json!([{"code": "crimson", "display": "Crimson"}]),
            "and a concept keeps its own child concepts"
        );
        assert_eq!(
            body["concept"][0]["display"],
            json!("Red"),
            "while the elements the form does draw are the form's"
        );
    }

    #[test]
    fn a_form_that_empties_an_element_removes_it_rather_than_sending_it_empty() {
        let stored: Value = serde_json::from_str(
            r#"{"resourceType":"CodeSystem","url":"https://terminology.example/colours",
                "version":"1.0.0","status":"active","content":"complete",
                "concept":[{"code":"red","display":"Red","definition":"The colour red",
                  "designation":[{"language":"nl-NL","value":"Rood"}]}]}"#,
        )
        .expect("the server's own answer parses");
        let mut draft = Draft::of(&stored);
        draft.version = String::new();
        if let Some(concept) = draft.concepts.first_mut() {
            concept.display = String::new();
            concept.definition = String::from("  ");
            concept.designations.clear();
        }
        let body = body_of(&draft);
        assert_eq!(body.get("version"), None);
        assert_eq!(body["concept"][0].get("display"), None);
        assert_eq!(body["concept"][0].get("definition"), None);
        assert_eq!(
            body["concept"][0].get("designation"),
            None,
            "a designation the reader removed is gone rather than sent empty"
        );
    }

    #[test]
    fn a_concept_with_no_status_reads_the_state_its_dates_imply() {
        let stored: Value = serde_json::from_str(
            r#"{"resourceType":"CodeSystem","status":"active","content":"complete","concept":[
                 {"code":"a"},
                 {"code":"b","property":[{"code":"retirementDate","valueDateTime":"2001-06-15"}]},
                 {"code":"c","property":[{"code":"deprecationDate","valueDateTime":"2001-06-15"}]},
                 {"code":"d","property":[{"code":"inactive","valueBoolean":true}]}]}"#,
        )
        .expect("the server's own answer parses");
        let draft = Draft::of(&stored);
        let states: Vec<Lifecycle> = draft
            .concepts
            .iter()
            .map(|concept| concept.lifecycle.clone())
            .collect();
        assert_eq!(
            states,
            vec![
                Lifecycle::Active,
                Lifecycle::Retired {
                    on: "2001-06-15".to_owned()
                },
                Lifecycle::Deprecated {
                    on: "2001-06-15".to_owned()
                },
                Lifecycle::Retired { on: String::new() },
            ]
        );
    }

    #[test]
    fn every_row_of_one_draft_gets_a_key_of_its_own() {
        let draft = drafted();
        let mut keys: Vec<Key> = draft.properties.iter().map(|row| row.key).collect();
        for concept in &draft.concepts {
            keys.push(concept.key);
            keys.extend(concept.designations.iter().map(|row| row.key));
            keys.extend(concept.values.iter().map(|row| row.key));
        }
        let mut seen = keys.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), keys.len(), "two rows share a key: {keys:?}");
    }

    #[test]
    fn a_draft_names_what_a_save_needs_before_it_is_savable() {
        let mut draft = Draft::new();
        assert!(!draft.savable(), "a code system is named by its canonical");
        draft.url = "https://terminology.example/colours".to_owned();
        assert!(draft.savable());
        draft.status = String::new();
        assert!(!draft.savable(), "status is mandatory on a CodeSystem");
    }
}
