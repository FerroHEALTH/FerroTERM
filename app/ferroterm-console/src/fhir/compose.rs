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

pub(crate) mod stored;
mod wire;

use serde_json::Value;

use crate::fhir::release::{Release, Rewritable, rewritten};
use crate::fhir::version::FhirVersion;
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
    /// A key is minted once and never derived from a position, so a posted
    /// field names its clause whatever was added or removed before it.
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

    /// The `ValueSet` a save sends, written as the `ValueSet` of `version`.
    ///
    /// It starts from the resource this draft was read from, so an element the
    /// form does not draw travels back rather than being deleted by an update
    /// that replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>). The elements the form
    /// owns are written over it.
    ///
    /// # Errors
    ///
    /// Returns the decoder's error for a stored resource that is not a
    /// `ValueSet` at `version`.
    pub(crate) fn body(&self, version: FhirVersion) -> Result<String, serde_json::Error> {
        version.rewrite(self)
    }

    /// The `Parameters` body a preview of this draft sends, written as the
    /// `Parameters` of `version`.
    ///
    /// `$expand` invoked by `POST` carries every parameter in a `Parameters`
    /// body (<https://hl7.org/fhir/R4B/operations.html#request>), and the
    /// unsaved compose travels in `valueSet` because nothing has a canonical
    /// to name yet.
    ///
    /// # Errors
    ///
    /// Returns the decoder's error for a stored resource that is not a
    /// `ValueSet` at `version`.
    pub(crate) fn preview_body(
        &self,
        version: FhirVersion,
        page: &Preview,
    ) -> Result<String, serde_json::Error> {
        version.preview(&self.base, self, page)
    }
}

impl Rewritable for Draft {
    fn rewrite<R: Release>(&self) -> Result<String, serde_json::Error> {
        rewritten::<R::ValueSet, Self>(&self.base, self)
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
    pub(crate) count: i32,
    /// Where the page starts.
    pub(crate) offset: i32,
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
    pub(crate) fn rule(&self, locale: &ferrohealth_ui::i18n::Locale) -> String {
        let mut criteria: Vec<String> = Vec::new();
        match self.value_sets.len() {
            0 => {}
            1 => criteria.push(tr(locale, Msg::ValueSetNamedHere)),
            several => criteria.push(tr_with(
                locale,
                Msg::EveryOneValueSetsNamedHere,
                &[("several", (several).to_string().into())],
            )),
        }
        if !self.system.trim().is_empty() {
            criteria.push(tr(locale, Msg::ComesFromCodeSystemNamedHere));
        }
        if !self.concepts.is_empty() {
            criteria.push(tr_with(
                locale,
                Msg::OneCodesListed,
                &[("concepts", (self.concepts.len()).to_string().into())],
            ));
        }
        if !self.filters.is_empty() {
            criteria.push(tr_with(
                locale,
                Msg::MatchesEveryOneFilters,
                &[("filters", (self.filters.len()).to_string().into())],
            ));
        }
        let verb = if self.included {
            tr(locale, Msg::ClauseDrawsEveryCode)
        } else {
            tr(locale, Msg::ClauseTakesBackOutEveryCode)
        };
        if criteria.is_empty() {
            return tr_with(
                locale,
                Msg::NothingSelectsYet,
                &[("verb", (verb).clone().into())],
            );
        }
        format!("{verb} {}.", criteria.join(&tr(locale, Msg::And)))
    }
}

#[cfg(test)]
mod tests;
