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

mod body;

use body::designation_body;
use body::object;
use body::set;
use body::text_of;
use body::value_body;

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
mod tests;
