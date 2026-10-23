// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ValueSet.compose` a terminologist authors, and what the server reads
//! it as.
//!
//! `compose` is a definition rather than a list. "Multiple include statements
//! are cumulative", "within an include, all the criterion apply", codes named
//! by several `valueSet` references in one include are "selected for inclusion
//! if they are in all the referenced value sets", and "codes in the exclude
//! statements are never in the value set"
//! (<https://hl7.org/fhir/R4B/valueset.html#compositions>, and the same text
//! plus the worked union and intersection rules on
//! <https://hl7.org/fhir/R5/valueset.html#union-intersection>). Three
//! invariants bound what one include may say, and the screen states the one it
//! breaks by its own number rather than refusing silently.
//!
//! The model lives here, outside every component, so the composition rules and
//! the JSON the server receives are values plain unit tests can pin.

use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Number;
use serde_json::Value;

use crate::fhir::concept::VALUE_SET_PARAMETER;
use crate::fhir::concept::resource_parameter;
use crate::fhir::concept::value_parameter;
use crate::fhir::expansion::COUNT_PARAMETER;
use crate::fhir::expansion::FILTER_PARAMETER;
use crate::fhir::expansion::OFFSET_PARAMETER;
use crate::locale::{Msg, tr, tr_with};

/// The `ValueSet.status` codes a draft can be in.
///
/// They are the `PublicationStatus` codes every definitional resource carries
/// (<https://hl7.org/fhir/R4B/valueset-publication-status.html>), so the list
/// is the specification's rather than this server's.
pub(crate) const STATUSES: [&str; 4] = ["draft", "active", "retired", "unknown"];

/// The status a new draft opens in.
const NEW_STATUS: &str = "draft";

/// One invariant `ValueSet.compose.include` has to pass.
///
/// The three are `vsd-1`, `vsd-2`, and `vsd-3` of `ValueSet.compose.include`
/// (<https://hl7.org/fhir/R4B/valueset.html>). A screen that let one through
/// would be offering a save the server refuses, so the composer states which
/// one is broken and where.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Defect {
    /// `vsd-1`: the clause names neither a code system nor a value set.
    NoSystemOrValueSet,
    /// `vsd-2`: the clause lists codes or filters without naming a system.
    CriteriaWithoutSystem,
    /// `vsd-3`: the clause lists codes and filters at once.
    ConceptsAndFilters,
}

impl Defect {
    /// The invariant's own number, as the specification writes it.
    pub(crate) const fn constraint(self) -> &'static str {
        match self {
            Self::NoSystemOrValueSet => "vsd-1",
            Self::CriteriaWithoutSystem => "vsd-2",
            Self::ConceptsAndFilters => "vsd-3",
        }
    }

    /// What the invariant says, in the terms of this form.
    pub(crate) const fn sentence(self) -> Msg {
        match self {
            Self::NoSystemOrValueSet => Msg::ClauseNamesNeitherValueSetNor,
            Self::CriteriaWithoutSystem => Msg::ClauseListsCodesFiltersWhichName,
            Self::ConceptsAndFilters => Msg::ClauseListsCodesFiltersOnceSplit,
        }
    }
}

/// Where a defect sits, so the screen can put the message on the clause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Broken {
    /// The clause's own key.
    pub(crate) key: u32,
    /// Which invariant it breaks.
    pub(crate) defect: Defect,
}

/// One value set a clause draws in whole.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ValueSetRef {
    /// The row's own key, for a list that is added to and removed from.
    pub(crate) key: u32,
    /// The canonical, whatever form its code system publishes it under.
    pub(crate) canonical: String,
}

/// One code a clause names, with the display its author gives it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PickedConcept {
    /// The row's own key.
    pub(crate) key: u32,
    /// The code, as the code system spells it.
    pub(crate) code: String,
    /// The display the code system answered, kept so the row reads.
    pub(crate) served_display: String,
    /// `compose.include.concept.display`, the author's own wording, which
    /// overrides the code system's (<https://hl7.org/fhir/R4B/valueset.html>).
    pub(crate) display: String,
}

/// One filter a clause selects with.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClauseFilter {
    /// The row's own key.
    pub(crate) key: u32,
    /// `filter.property`, a code the system declares.
    pub(crate) property: String,
    /// `filter.op`, an operator the system declares on that property.
    pub(crate) op: String,
    /// `filter.value`.
    pub(crate) value: String,
}

/// One `compose.include` or `compose.exclude`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Clause {
    /// The clause's own key, minted once and never derived from a position.
    pub(crate) key: u32,
    /// Whether the clause draws codes in or takes them back out.
    pub(crate) included: bool,
    /// `include.system`, empty when the clause names none.
    pub(crate) system: String,
    /// `include.version`, the code system version the clause is pinned to.
    pub(crate) system_version: String,
    /// `include.valueSet`, the value sets drawn in whole.
    pub(crate) value_sets: Vec<ValueSetRef>,
    /// `include.concept`, the codes named one by one.
    pub(crate) concepts: Vec<PickedConcept>,
    /// `include.filter`, the filters selected with.
    pub(crate) filters: Vec<ClauseFilter>,
}

/// The value set being composed, before it is saved.
///
/// It is `PartialEq` and not `Eq`, because the resource it was read from is
/// carried as the JSON document the server sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Draft {
    /// The logical id the server holds it under, absent until it is created.
    pub(crate) id: String,
    /// `meta.versionId`, the version an update states in `If-Match`.
    pub(crate) version_id: String,
    /// `ValueSet.url`, the canonical it is published under.
    pub(crate) url: String,
    /// `ValueSet.name`, the computer-friendly name.
    pub(crate) name: String,
    /// `ValueSet.title`, the name written for a person.
    pub(crate) title: String,
    /// `ValueSet.version`, the business version of the resource.
    pub(crate) version: String,
    /// `ValueSet.status`.
    pub(crate) status: String,
    /// `compose.inactive`, absent when the author states nothing.
    pub(crate) inactive: Option<bool>,
    /// The clauses, includes and excludes in one list in their own order.
    pub(crate) clauses: Vec<Clause>,
    /// The next key to mint. Keys are never reused, so a row a `<For>` drew
    /// cannot be confused with the row that takes its place.
    next_key: u32,
    /// The resource this draft was read from, as the server sent it.
    ///
    /// An update replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>), so a save is this
    /// document with the elements the form owns written over it. Everything
    /// else, from `description` to a concept's own designations, travels back
    /// untouched instead of being deleted by a form that never drew it.
    base: Value,
}

impl Draft {
    /// A draft of nothing, with one empty include to start from.
    pub(crate) fn new() -> Self {
        let mut draft = Self {
            status: String::from(NEW_STATUS),
            ..Self::default()
        };
        draft.add_clause(true);
        draft
    }

    /// Mints a key nothing in this draft has carried.
    fn mint(&mut self) -> u32 {
        let key = self.next_key;
        self.next_key = self.next_key.saturating_add(1);
        key
    }

    /// Adds an empty clause, included or excluded, and answers its key.
    pub(crate) fn add_clause(&mut self, included: bool) -> u32 {
        let key = self.mint();
        self.clauses.push(Clause {
            key,
            included,
            ..Clause::default()
        });
        key
    }

    /// The clause `key` names, to change.
    pub(crate) fn clause_mut(&mut self, key: u32) -> Option<&mut Clause> {
        self.clauses.iter_mut().find(|clause| clause.key == key)
    }

    /// The keys of the clauses one side of the compose is drawn from.
    ///
    /// A key is minted once and never derived from a position, so it is the
    /// data-derived key a `<For>` needs
    /// (<https://github.com/leptos-rs/book/blob/main/src/view/04_iteration.md>).
    pub(crate) fn clause_keys(&self, included: bool) -> Vec<u32> {
        self.clauses
            .iter()
            .filter(|clause| clause.included == included)
            .map(|clause| clause.key)
            .collect()
    }

    /// Adds a value set reference to the clause `key` names.
    pub(crate) fn add_value_set(&mut self, key: u32, canonical: &str) {
        let row = self.mint();
        if let Some(clause) = self.clause_mut(key) {
            clause.value_sets.push(ValueSetRef {
                key: row,
                canonical: canonical.to_owned(),
            });
        }
    }

    /// Adds a code to the clause `key` names, with the display it was found
    /// under.
    pub(crate) fn add_concept(&mut self, key: u32, code: &str, served_display: &str) {
        let row = self.mint();
        if let Some(clause) = self.clause_mut(key)
            && !clause.concepts.iter().any(|held| held.code == code)
        {
            clause.concepts.push(PickedConcept {
                key: row,
                code: code.to_owned(),
                served_display: served_display.to_owned(),
                display: String::new(),
            });
        }
    }

    /// Adds a filter to the clause `key` names.
    pub(crate) fn add_filter(&mut self, key: u32, property: &str, op: &str) {
        let row = self.mint();
        if let Some(clause) = self.clause_mut(key) {
            clause.filters.push(ClauseFilter {
                key: row,
                property: property.to_owned(),
                op: op.to_owned(),
                value: String::new(),
            });
        }
    }

    /// Every invariant any clause breaks, in the order the clauses are drawn.
    pub(crate) fn broken(&self) -> Vec<Broken> {
        self.clauses
            .iter()
            .filter_map(|clause| {
                clause.defect().map(|defect| Broken {
                    key: clause.key,
                    defect,
                })
            })
            .collect()
    }

    /// Whether the draft is one the server can be asked to take.
    ///
    /// `ValueSet.status` is 1..1, `url` is what every other resource refers to
    /// it by, and `compose.include` is 1..*
    /// (<https://hl7.org/fhir/R4B/valueset.html>), so a draft missing any of
    /// the three is not offered for saving.
    pub(crate) fn savable(&self) -> bool {
        !self.url.trim().is_empty()
            && !self.status.is_empty()
            && !self.clause_keys(true).is_empty()
            && self.broken().is_empty()
    }

    /// The `ValueSet` resource this draft is, as the server receives it.
    ///
    /// It starts from the resource this draft was read from, so an element the
    /// form does not draw travels back rather than being deleted by an update
    /// that replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>). The elements the form
    /// owns are written over it.
    pub(crate) fn resource(&self) -> Value {
        let mut resource = match &self.base {
            Value::Object(held) => held.clone(),
            _unread => Map::new(),
        };
        resource.insert(
            String::from("resourceType"),
            Value::String(String::from("ValueSet")),
        );
        if self.id.is_empty() {
            resource.remove("id");
        } else {
            resource.insert(String::from("id"), Value::String(self.id.clone()));
        }
        for (element, text) in [
            ("url", &self.url),
            ("version", &self.version),
            ("name", &self.name),
            ("title", &self.title),
        ] {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                resource.remove(element);
            } else {
                resource.insert(String::from(element), Value::String(trimmed.to_owned()));
            }
        }
        resource.insert(String::from("status"), Value::String(self.status.clone()));
        match self.compose() {
            // `ValueSet.compose` is 0..1, so a draft composing nothing sends
            // no element rather than an empty one.
            Some(compose) => resource.insert(String::from("compose"), compose),
            None => resource.remove("compose"),
        };
        Value::Object(resource)
    }

    /// `ValueSet.compose`, with the includes before the excludes.
    ///
    /// The elements the composer does not draw, `lockedDate` among them,
    /// travel back from the resource this draft was read from.
    fn compose(&self) -> Option<Value> {
        if self.clauses.is_empty() {
            return None;
        }
        let mut compose = match self.base.get("compose") {
            Some(Value::Object(held)) => held.clone(),
            _unread => Map::new(),
        };
        match self.inactive {
            Some(inactive) => compose.insert(String::from("inactive"), Value::Bool(inactive)),
            None => compose.remove("inactive"),
        };
        for (element, included) in [("include", true), ("exclude", false)] {
            let clauses: Vec<Value> = self
                .clauses
                .iter()
                .filter(|clause| clause.included == included)
                .map(|clause| clause.wire(self.held(element, clause)))
                .collect();
            if clauses.is_empty() {
                compose.remove(element);
            } else {
                compose.insert(String::from(element), Value::Array(clauses));
            }
        }
        Some(Value::Object(compose))
    }

    /// The `compose.include` or `compose.exclude` entry `clause` was read from.
    ///
    /// A clause is matched to the stored one by its position on its own side,
    /// which is the order `StoredValueSet::draft` read them in and the order
    /// this writes them back, so a clause keeps the elements the form does not
    /// draw as long as the reader has not reordered that side.
    fn held(&self, element: &str, clause: &Clause) -> Map<String, Value> {
        let position = self
            .clauses
            .iter()
            .filter(|other| other.included == clause.included)
            .position(|other| other.key == clause.key);
        position
            .and_then(|at| {
                self.base
                    .get("compose")?
                    .get(element)?
                    .as_array()?
                    .get(at)?
                    .as_object()
                    .cloned()
            })
            .unwrap_or_default()
    }

    /// The `Parameters` body a preview of this draft sends.
    ///
    /// `$expand` invoked by `POST` carries every parameter in a `Parameters`
    /// body (<https://hl7.org/fhir/R4B/operations.html#request>), and the
    /// unsaved compose travels in `valueSet` because nothing has a canonical
    /// to name yet.
    pub(crate) fn preview_body(&self, page: &Preview) -> String {
        let mut parameters = vec![resource_parameter(VALUE_SET_PARAMETER, self.resource())];
        if let Some(filter) = page.filter.as_ref().filter(|text| !text.is_empty()) {
            parameters.push(value_parameter(
                FILTER_PARAMETER,
                "valueString",
                Value::String(filter.clone()),
            ));
        }
        parameters.push(value_parameter(
            COUNT_PARAMETER,
            "valueInteger",
            Value::Number(Number::from(page.count)),
        ));
        if page.offset > 0 {
            parameters.push(value_parameter(
                OFFSET_PARAMETER,
                "valueInteger",
                Value::Number(Number::from(page.offset)),
            ));
        }
        let mut body = Map::new();
        body.insert(
            String::from("resourceType"),
            Value::String(tr(Msg::Parameters)),
        );
        body.insert(String::from("parameter"), Value::Array(parameters));
        Value::Object(body).to_string()
    }
}

/// What one run of the preview asks for.
///
/// Every field is a query parameter of the screen, so a preview is an address
/// and the back button walks it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Preview {
    /// The text filter, when the reader typed one.
    pub(crate) filter: Option<String>,
    /// How many concepts one page holds.
    pub(crate) count: u32,
    /// Where the page starts.
    pub(crate) offset: u32,
}

impl Clause {
    /// The invariant this clause breaks, when it breaks one.
    ///
    /// The order is the specification's own numbering, so a clause that breaks
    /// more than one is reported by the first.
    pub(crate) fn defect(&self) -> Option<Defect> {
        let system = !self.system.trim().is_empty();
        let criteria = !self.concepts.is_empty() || !self.filters.is_empty();
        if !system {
            if self.value_sets.iter().all(|row| row.canonical.is_empty()) {
                return Some(Defect::NoSystemOrValueSet);
            }
            if criteria {
                return Some(Defect::CriteriaWithoutSystem);
            }
        }
        if !self.concepts.is_empty() && !self.filters.is_empty() {
            return Some(Defect::ConceptsAndFilters);
        }
        None
    }

    /// How the server reads this clause, as the sentence the screen shows.
    ///
    /// The criteria within one include intersect, so this names each of them
    /// and says they are taken together
    /// (<https://hl7.org/fhir/R4B/valueset.html#compositions>).
    pub(crate) fn rule(&self) -> String {
        let mut criteria: Vec<String> = Vec::new();
        match self.value_sets.len() {
            0 => {}
            1 => criteria.push(tr(Msg::ValueSetNamedHere)),
            several => criteria.push(tr_with(
                Msg::EveryOneValueSetsNamedHere,
                &[("several", (several).to_string().into())],
            )),
        }
        if !self.system.trim().is_empty() {
            criteria.push(tr(Msg::ComesFromCodeSystemNamedHere));
        }
        if !self.concepts.is_empty() {
            criteria.push(tr_with(
                Msg::OneCodesListed,
                &[("concepts", (self.concepts.len()).to_string().into())],
            ));
        }
        if !self.filters.is_empty() {
            criteria.push(tr_with(
                Msg::MatchesEveryOneFilters,
                &[("filters", (self.filters.len()).to_string().into())],
            ));
        }
        let verb = if self.included {
            tr(Msg::ClauseDrawsEveryCode)
        } else {
            tr(Msg::ClauseTakesBackOutEveryCode)
        };
        if criteria.is_empty() {
            return tr_with(Msg::NothingSelectsYet, &[("verb", (verb).clone().into())]);
        }
        format!("{verb} {}.", criteria.join(&tr(Msg::And)))
    }

    /// This clause as `compose.include` or `compose.exclude`.
    ///
    /// `held` is the entry it was read from, so an element the form does not
    /// draw travels back rather than being deleted by the update.
    fn wire(&self, held: Map<String, Value>) -> Value {
        let mut set = held;
        let system = self.system.trim();
        if system.is_empty() {
            set.remove("system");
            set.remove("version");
        } else {
            set.insert(String::from("system"), Value::String(system.to_owned()));
            let version = self.system_version.trim();
            if version.is_empty() {
                set.remove("version");
            } else {
                set.insert(String::from("version"), Value::String(version.to_owned()));
            }
        }
        let value_sets: Vec<Value> = self
            .value_sets
            .iter()
            .filter(|row| !row.canonical.trim().is_empty())
            .map(|row| Value::String(row.canonical.trim().to_owned()))
            .collect();
        if value_sets.is_empty() {
            set.remove("valueSet");
        } else {
            set.insert(String::from("valueSet"), Value::Array(value_sets));
        }
        // NOTE: "Any display names specified for the codes are ignored" in an
        // exclude (<https://hl7.org/fhir/R4B/valueset.html>,
        // `ValueSet.compose.exclude`), so none is written into one.
        let concepts: Vec<Value> = self
            .concepts
            .iter()
            .filter(|picked| !picked.code.trim().is_empty())
            .map(|picked| {
                picked.wire(
                    self.included,
                    held_by(&set, "concept", "code", &picked.code),
                )
            })
            .collect();
        if concepts.is_empty() {
            set.remove("concept");
        } else {
            set.insert(String::from("concept"), Value::Array(concepts));
        }
        let filters: Vec<Value> = self
            .filters
            .iter()
            .filter(|filter| !filter.property.trim().is_empty())
            .map(|filter| filter.wire(held_by(&set, "filter", "property", &filter.property)))
            .collect();
        if filters.is_empty() {
            set.remove("filter");
        } else {
            set.insert(String::from("filter"), Value::Array(filters));
        }
        Value::Object(set)
    }
}

/// The entry of `list` in `held` whose `key` element is `value`.
///
/// A concept is identified by its code and a filter by the property it is
/// over (<https://hl7.org/fhir/R4B/valueset.html>), so that is what an
/// authored row is matched to the stored one by, and what carries the
/// elements the form does not draw back through a save.
fn held_by(held: &Map<String, Value>, list: &str, key: &str, value: &str) -> Map<String, Value> {
    held.get(list)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .find(|entry| entry.get(key).and_then(Value::as_str) == Some(value.trim()))
        .cloned()
        .unwrap_or_default()
}

impl PickedConcept {
    /// This code as `compose.include.concept`, or `compose.exclude.concept`.
    ///
    /// `with_display` is whether the clause is an include, which is the only
    /// place a display the author wrote is read.
    fn wire(&self, with_display: bool, held: Map<String, Value>) -> Value {
        let mut concept = held;
        concept.insert(
            String::from("code"),
            Value::String(self.code.trim().to_owned()),
        );
        let display = self.display.trim();
        if with_display && !display.is_empty() {
            concept.insert(String::from("display"), Value::String(display.to_owned()));
        } else {
            concept.remove("display");
        }
        Value::Object(concept)
    }
}

impl ClauseFilter {
    /// This filter as `compose.include.filter`.
    fn wire(&self, held: Map<String, Value>) -> Value {
        let mut filter = held;
        filter.insert(
            String::from("property"),
            Value::String(self.property.trim().to_owned()),
        );
        filter.insert(String::from("op"), Value::String(self.op.trim().to_owned()));
        filter.insert(
            String::from("value"),
            Value::String(self.value.trim().to_owned()),
        );
        Value::Object(filter)
    }
}

/// The `ValueSet` the composer reads back off the server.
///
/// It carries the elements the composer authors and the `meta` an update needs,
/// and nothing else: the viewer never mirrors a whole FHIR resource.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredValueSet {
    /// `ValueSet.id`.
    #[serde(default)]
    id: Option<String>,
    /// `ValueSet.meta`.
    #[serde(default)]
    meta: Option<StoredMeta>,
    /// `ValueSet.url`.
    #[serde(default)]
    url: Option<String>,
    /// `ValueSet.version`.
    #[serde(default)]
    version: Option<String>,
    /// `ValueSet.name`.
    #[serde(default)]
    name: Option<String>,
    /// `ValueSet.title`.
    #[serde(default)]
    title: Option<String>,
    /// `ValueSet.status`.
    #[serde(default)]
    status: Option<String>,
    /// `ValueSet.compose`.
    #[serde(default)]
    compose: Option<StoredCompose>,
}

/// The `meta` elements the composer reads.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
struct StoredMeta {
    /// `meta.versionId`.
    #[serde(default, rename = "versionId")]
    version_id: Option<String>,
}

/// `ValueSet.compose`, as it is read back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
struct StoredCompose {
    /// `compose.inactive`.
    inactive: Option<bool>,
    /// `compose.include`.
    include: Vec<StoredSet>,
    /// `compose.exclude`.
    exclude: Vec<StoredSet>,
}

/// One `compose.include` or `compose.exclude`, as it is read back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
struct StoredSet {
    /// `system`.
    system: Option<String>,
    /// `version`.
    version: Option<String>,
    /// `valueSet`.
    #[serde(rename = "valueSet")]
    value_set: Vec<String>,
    /// `concept`.
    concept: Vec<StoredConcept>,
    /// `filter`.
    filter: Vec<StoredFilter>,
}

/// One `compose.include.concept`, as it is read back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
struct StoredConcept {
    /// `concept.code`.
    code: Option<String>,
    /// `concept.display`.
    display: Option<String>,
}

/// One `compose.include.filter`, as it is read back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
struct StoredFilter {
    /// `filter.property`.
    property: Option<String>,
    /// `filter.op`.
    op: Option<String>,
    /// `filter.value`.
    value: Option<String>,
}

impl StoredValueSet {
    /// Whether this server holds a writable record of the resource.
    ///
    /// The server stamps `meta.versionId` on every resource written through
    /// its own API and raises it on each update, and a resource served out of
    /// the loaded content carries none, so a read that answers no version is
    /// content with no write path behind it and the screen opens read-only
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    pub(crate) fn writable(&self) -> bool {
        self.meta
            .as_ref()
            .and_then(|meta| meta.version_id.as_deref())
            .is_some_and(|version| !version.is_empty())
    }

    /// The elements the composer reads out of one `ValueSet` document.
    ///
    /// A document that does not parse into them reads as a resource stating
    /// none of them, which is the same answer as a server that sent none.
    pub(crate) fn of(resource: &Value) -> Self {
        serde_json::from_value(resource.clone()).unwrap_or_default()
    }

    /// This resource as a draft to edit.
    ///
    /// `resource` is the document the server sent, which the draft carries so
    /// a save writes over it rather than replacing it with the elements the
    /// form draws (<https://hl7.org/fhir/R4B/http.html#update>).
    pub(crate) fn draft(&self, resource: &Value) -> Draft {
        let mut draft = Draft {
            id: self.id.clone().unwrap_or_default(),
            version_id: self
                .meta
                .as_ref()
                .and_then(|meta| meta.version_id.clone())
                .unwrap_or_default(),
            url: self.url.clone().unwrap_or_default(),
            name: self.name.clone().unwrap_or_default(),
            title: self.title.clone().unwrap_or_default(),
            version: self.version.clone().unwrap_or_default(),
            status: self
                .status
                .clone()
                .unwrap_or_else(|| String::from(NEW_STATUS)),
            inactive: self.compose.as_ref().and_then(|compose| compose.inactive),
            clauses: Vec::new(),
            next_key: 0,
            base: resource.clone(),
        };
        let compose = self.compose.clone().unwrap_or_default();
        for (sets, included) in [(&compose.include, true), (&compose.exclude, false)] {
            for set in sets {
                let key = draft.add_clause(included);
                for canonical in &set.value_set {
                    draft.add_value_set(key, canonical);
                }
                for concept in &set.concept {
                    let code = concept.code.clone().unwrap_or_default();
                    draft.add_concept(key, &code, concept.display.as_deref().unwrap_or_default());
                    if let Some(display) = concept.display.clone()
                        && let Some(row) = draft
                            .clause_mut(key)
                            .and_then(|clause| clause.concepts.last_mut())
                    {
                        row.display = display;
                    }
                }
                for filter in &set.filter {
                    draft.add_filter(
                        key,
                        filter.property.as_deref().unwrap_or_default(),
                        filter.op.as_deref().unwrap_or_default(),
                    );
                    if let Some(row) = draft
                        .clause_mut(key)
                        .and_then(|clause| clause.filters.last_mut())
                    {
                        row.value = filter.value.clone().unwrap_or_default();
                    }
                }
                if let Some(clause) = draft.clause_mut(key) {
                    clause.system = set.system.clone().unwrap_or_default();
                    clause.system_version = set.version.clone().unwrap_or_default();
                }
            }
        }
        if draft.clauses.is_empty() {
            draft.add_clause(true);
        }
        draft
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A draft with one include over `system`, and the key of that include.
    fn over(system: &str) -> (Draft, u32) {
        let mut draft = Draft::new();
        draft.url = String::from("https://terminology.example/vs/local");
        let key = draft
            .clause_keys(true)
            .first()
            .copied()
            .expect("a new draft opens with one include");
        if let Some(clause) = draft.clause_mut(key) {
            clause.system = system.to_owned();
        }
        (draft, key)
    }

    #[test]
    fn a_clause_naming_nothing_breaks_the_first_invariant() {
        let draft = Draft::new();
        let broken = draft.broken();
        assert_eq!(
            broken.first().map(|row| row.defect),
            Some(Defect::NoSystemOrValueSet),
            "an empty clause selects nothing, which vsd-1 refuses"
        );
        assert_eq!(
            broken.first().map(|row| row.defect.constraint()),
            Some("vsd-1")
        );
        assert!(
            !draft.savable(),
            "a broken draft is never offered for saving"
        );
    }

    #[test]
    fn codes_without_a_system_break_the_second_invariant() {
        let mut draft = Draft::new();
        draft.url = String::from("https://terminology.example/vs/local");
        let key = draft
            .clause_keys(true)
            .first()
            .copied()
            .expect("one include");
        draft.add_value_set(key, "https://terminology.example/vs/national");
        draft.add_concept(key, "A", "Alpha");
        assert_eq!(
            draft.broken().first().map(|row| row.defect),
            Some(Defect::CriteriaWithoutSystem),
            "vsd-2: concepts name the system they are read from"
        );
    }

    #[test]
    fn codes_and_filters_in_one_clause_break_the_third_invariant() {
        let (mut draft, key) = over("https://terminology.example/x");
        draft.add_concept(key, "A", "Alpha");
        draft.add_filter(key, "concept", "is-a");
        let broken = draft.broken();
        assert_eq!(
            broken.first().map(|row| row.defect),
            Some(Defect::ConceptsAndFilters),
            "vsd-3: a filter beside enumerated codes is refused"
        );
        assert_eq!(
            broken.first().map(|row| row.defect.constraint()),
            Some("vsd-3")
        );
        assert_eq!(broken.first().map(|row| row.key), Some(key));
    }

    #[test]
    fn a_value_set_beside_enumerated_codes_is_allowed_and_intersects() {
        let (mut draft, key) = over("https://terminology.example/x");
        draft.add_value_set(key, "https://terminology.example/vs/national");
        draft.add_concept(key, "A", "Alpha");
        assert!(
            draft.broken().is_empty(),
            "vsd-3 forbids concepts beside filters, not beside a value set"
        );
        let rule = draft
            .clauses
            .iter()
            .find(|clause| clause.key == key)
            .map(Clause::rule)
            .unwrap_or_default();
        assert!(
            rule.contains("is in the value set named here")
                && rule.contains("comes from the code system named here")
                && rule.contains("is one of the 1 codes listed"),
            "the criteria inside one include intersect: `{rule}`"
        );
        assert!(rule.starts_with("This clause draws in"), "{rule}");
    }

    #[test]
    fn several_value_sets_in_one_clause_are_stated_as_intersecting() {
        let (mut draft, key) = over("");
        draft.add_value_set(key, "https://terminology.example/vs/a");
        draft.add_value_set(key, "https://terminology.example/vs/b");
        let rule = draft
            .clauses
            .iter()
            .find(|clause| clause.key == key)
            .map(Clause::rule)
            .unwrap_or_default();
        assert!(
            rule.contains("is in every one of the 2 value sets named here"),
            "several valueSet references in one include intersect: `{rule}`"
        );
    }

    #[test]
    fn an_exclude_says_it_takes_codes_back_out() {
        let mut draft = Draft::new();
        let key = draft.add_clause(false);
        draft.add_value_set(key, "https://terminology.example/vs/a");
        let rule = draft
            .clauses
            .iter()
            .find(|clause| clause.key == key)
            .map(Clause::rule)
            .unwrap_or_default();
        assert!(
            rule.starts_with("This clause takes back out"),
            "exclude subtracts from the union of the includes: `{rule}`"
        );
    }

    #[test]
    fn the_resource_is_the_compose_the_server_reads() {
        let (mut draft, key) = over("https://terminology.example/x");
        draft.name = String::from("LocalSet");
        draft.status = String::from("active");
        draft.inactive = Some(false);
        draft.add_concept(key, "A", "Alpha");
        if let Some(row) = draft
            .clause_mut(key)
            .and_then(|clause| clause.concepts.last_mut())
        {
            row.display = String::from("The author's own wording");
        }
        let national = draft.add_clause(true);
        draft.add_value_set(national, "https://terminology.example/vs/national");
        let excluded = draft.add_clause(false);
        if let Some(clause) = draft.clause_mut(excluded) {
            clause.system = String::from("https://terminology.example/x");
        }
        draft.add_filter(excluded, "concept", "is-a");
        if let Some(row) = draft
            .clause_mut(excluded)
            .and_then(|clause| clause.filters.last_mut())
        {
            row.value = String::from("root");
        }
        assert_eq!(
            draft.resource().to_string(),
            concat!(
                r#"{"compose":{"exclude":[{"filter":[{"op":"is-a","property":"concept","value":"root"}],"#,
                r#""system":"https://terminology.example/x"}],"inactive":false,"#,
                r#""include":[{"concept":[{"code":"A","display":"The author's own wording"}],"#,
                r#""system":"https://terminology.example/x"},"#,
                r#"{"valueSet":["https://terminology.example/vs/national"]}]},"#,
                r#""name":"LocalSet","resourceType":"ValueSet","status":"active",
"url":"https://terminology.example/vs/local"}"#
            )
            .replace('\n', ""),
            "the display the author wrote overrides the code system's own"
        );
        assert!(draft.savable());
    }

    #[test]
    fn an_exclude_carries_no_display_because_the_server_ignores_one() {
        let mut draft = Draft::new();
        draft.url = String::from("https://terminology.example/vs/local");
        let excluded = draft.add_clause(false);
        if let Some(clause) = draft.clause_mut(excluded) {
            clause.system = String::from("https://terminology.example/x");
        }
        draft.add_concept(excluded, "A", "Alpha");
        if let Some(row) = draft
            .clause_mut(excluded)
            .and_then(|clause| clause.concepts.last_mut())
        {
            row.display = String::from("Something else");
        }
        let written = draft.resource().to_string();
        assert!(
            written.contains(r#""exclude":[{"concept":[{"code":"A"}]"#),
            "`Any display names specified for the codes are ignored` in an exclude: {written}"
        );
    }

    #[test]
    fn an_update_carries_back_every_element_the_form_does_not_draw() {
        // An update replaces the whole resource
        // (<https://hl7.org/fhir/R4B/http.html#update>), so what the composer
        // does not model has to travel back out of the document it read.
        let document: Value = serde_json::from_str(
            r#"{"resourceType":"ValueSet","id":"local",
                "meta":{"versionId":"3","source":"https://terminology.example/source"},
                "language":"nl","text":{"status":"generated","div":"<p/>"},
                "url":"https://terminology.example/vs/local","status":"active",
                "description":"What this value set is for","publisher":"A publisher",
                "experimental":true,"jurisdiction":[{"text":"NL"}],
                "compose":{"lockedDate":"2026-01-01",
                  "include":[{"system":"https://terminology.example/x",
                    "concept":[{"code":"A","designation":[{"language":"nl","value":"Alfa"}]}]}]}}"#,
        )
        .expect("the server's own answer parses");
        let stored: StoredValueSet = serde_json::from_value(document.clone())
            .expect("the elements the composer reads parse");
        let written = stored.draft(&document).resource();
        for (element, kept) in [
            ("description", "What this value set is for"),
            ("publisher", "A publisher"),
            ("language", "nl"),
        ] {
            assert_eq!(
                written.get(element).and_then(Value::as_str),
                Some(kept),
                "`{element}` survives a save that never drew it"
            );
        }
        assert!(written.get("text").is_some(), "the narrative survives");
        assert!(
            written.get("jurisdiction").is_some(),
            "so does the jurisdiction"
        );
        assert_eq!(
            written
                .pointer("/compose/lockedDate")
                .and_then(Value::as_str),
            Some("2026-01-01"),
            "and a compose element the composer does not draw"
        );
        assert!(
            written
                .pointer("/compose/include/0/concept/0/designation")
                .is_some(),
            "and a designation on a code the composer only names: {written}"
        );
        assert_eq!(
            written.pointer("/meta/source").and_then(Value::as_str),
            Some("https://terminology.example/source"),
            "the meta the server sent is not replaced by the version alone"
        );
    }

    #[test]
    fn a_draft_composing_nothing_is_not_savable_and_sends_no_compose() {
        // `compose.include` is 1..* and `compose` itself is 0..1
        // (<https://hl7.org/fhir/R4B/valueset.html>).
        let mut draft = Draft::new();
        draft.url = String::from("https://terminology.example/vs/local");
        let key = draft.clause_keys(true)[0];
        draft.clauses.retain(|clause| clause.key != key);
        assert!(
            !draft.savable(),
            "a value set with no include is not offered for saving"
        );
        let written = draft.resource();
        assert!(
            written.get("compose").is_none(),
            "and sends no compose element at all: {written}"
        );
    }

    #[test]
    fn an_exclude_alone_is_not_savable() {
        let mut draft = Draft::new();
        draft.url = String::from("https://terminology.example/vs/local");
        let key = draft.clause_keys(true)[0];
        draft.clauses.retain(|clause| clause.key != key);
        let excluded = draft.add_clause(false);
        if let Some(clause) = draft.clause_mut(excluded) {
            clause.system = String::from("https://terminology.example/x");
        }
        assert!(
            !draft.savable(),
            "an exclude subtracts from a union that has to exist"
        );
    }

    #[test]
    fn a_preview_sends_the_unsaved_compose_inline_and_its_page() {
        let (mut draft, key) = over("https://terminology.example/x");
        draft.add_concept(key, "A", "Alpha");
        let body = draft.preview_body(&Preview {
            filter: Some(String::from("alp")),
            count: 20,
            offset: 40,
        });
        assert!(
            body.starts_with(r#"{"parameter":[{"name":"valueSet","resource":{"compose":"#),
            "the compose travels as a resource rather than as a canonical: {body}"
        );
        assert!(
            body.contains(r#"{"name":"filter","valueString":"alp"}"#),
            "{body}"
        );
        assert!(
            body.contains(r#"{"name":"count","valueInteger":20}"#),
            "{body}"
        );
        assert!(
            body.contains(r#"{"name":"offset","valueInteger":40}"#),
            "{body}"
        );
    }

    #[test]
    fn the_first_page_sends_no_offset() {
        let (draft, _key) = over("https://terminology.example/x");
        let body = draft.preview_body(&Preview {
            filter: None,
            count: 20,
            offset: 0,
        });
        assert!(
            !body.contains("offset"),
            "an unmoved page states no offset: {body}"
        );
        assert!(
            !body.contains("filter"),
            "an empty eye sends no filter: {body}"
        );
    }

    #[test]
    fn a_resource_read_back_round_trips_through_the_draft() {
        let document: Value = serde_json::from_str(
            r#"{"resourceType":"ValueSet","id":"local","meta":{"versionId":"3"},
                "url":"https://terminology.example/vs/local","status":"active","name":"LocalSet",
                "compose":{"inactive":true,
                  "include":[{"valueSet":["https://terminology.example/vs/national"]},
                             {"system":"https://terminology.example/x","version":"2.0",
                              "concept":[{"code":"A","display":"Alpha, as we say it"}]}],
                  "exclude":[{"system":"https://terminology.example/x",
                              "filter":[{"property":"concept","op":"is-a","value":"root"}]}]}}"#,
        )
        .expect("the server's own answer parses");
        let stored: StoredValueSet =
            serde_json::from_value(document.clone()).expect("the composer reads it");
        assert!(
            stored.writable(),
            "the server stamped a version, so it holds the record"
        );
        let draft = stored.draft(&document);
        assert_eq!(draft.id, "local");
        assert_eq!(draft.version_id, "3");
        assert_eq!(draft.inactive, Some(true));
        assert_eq!(draft.clauses.len(), 3);
        let keys: Vec<u32> = draft.clauses.iter().map(|clause| clause.key).collect();
        assert_eq!(
            keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
            keys.len(),
            "no two clauses share a key: {keys:?}"
        );
        assert_eq!(
            draft.clause_keys(false).len(),
            1,
            "one exclude was read back"
        );
        let second = draft.clause_keys(true)[1];
        let coded = draft
            .clauses
            .iter()
            .find(|clause| clause.key == second)
            .expect("the second include");
        assert_eq!(coded.system_version, "2.0");
        assert_eq!(
            coded.concepts.first().map(|row| row.display.clone()),
            Some(String::from("Alpha, as we say it"))
        );
        assert!(!draft.clauses[2].included, "the exclude stays an exclude");
        assert_eq!(
            draft.resource(),
            stored.draft(&document).resource(),
            "reading and writing back changes nothing"
        );
    }

    #[test]
    fn a_resource_the_server_stamped_no_version_on_is_read_only() {
        let document: Value = serde_json::from_str(
            r#"{"resourceType":"ValueSet","id":"national","url":"https://terminology.example/vs/n",
                "status":"active","compose":{"include":[{"system":"https://terminology.example/x"}]}}"#,
        )
        .expect("the server's own answer parses");
        let stored: StoredValueSet =
            serde_json::from_value(document.clone()).expect("the composer reads it");
        assert!(
            !stored.writable(),
            "content the server serves out of its loaded indexes has no write path"
        );
        assert!(
            !stored.draft(&document).id.is_empty(),
            "it still opens, to be read"
        );
    }

    #[test]
    fn a_resource_carrying_no_compose_opens_on_an_empty_clause() {
        let document: Value =
            serde_json::from_str(r#"{"resourceType":"ValueSet","id":"x","status":"draft"}"#)
                .expect("the server's own answer parses");
        let stored: StoredValueSet =
            serde_json::from_value(document.clone()).expect("the composer reads it");
        assert_eq!(
            stored.draft(&document).clauses.len(),
            1,
            "an expansion-only value set opens on something to compose into"
        );
    }

    #[test]
    fn a_code_is_never_listed_twice_in_one_clause() {
        let (mut draft, key) = over("https://terminology.example/x");
        draft.add_concept(key, "A", "Alpha");
        draft.add_concept(key, "A", "Alpha again");
        assert_eq!(
            draft
                .clauses
                .iter()
                .find(|clause| clause.key == key)
                .map(|clause| clause.concepts.len()),
            Some(1),
            "picking the same code twice adds one row"
        );
    }

    #[test]
    fn removing_a_clause_leaves_every_other_key_where_it_was() {
        let mut draft = Draft::new();
        let first = draft
            .clause_keys(true)
            .first()
            .copied()
            .expect("one include");
        let second = draft.add_clause(true);
        let third = draft.add_clause(false);
        draft.clauses.retain(|clause| clause.key != second);
        assert_eq!(draft.clause_keys(true), vec![first]);
        assert_eq!(draft.clause_keys(false), vec![third]);
        let fourth = draft.add_clause(true);
        assert!(
            ![first, second, third].contains(&fourth),
            "a key is never reused, so a row cannot inherit another row's state"
        );
    }

    #[test]
    fn the_statuses_offered_are_the_publication_status_codes() {
        assert_eq!(STATUSES, ["draft", "active", "retired", "unknown"]);
        assert!(STATUSES.contains(&NEW_STATUS));
    }
}
