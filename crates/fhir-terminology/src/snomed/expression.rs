// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Post-coordinated SNOMED CT expressions behind the provider seam.
//!
//! An expression in Compositional Grammar is a valid `code` for
//! `http://snomed.info/sct` and is "subject to the same rules as
//! precoordinated concepts" (<https://hl7.org/fhir/R4B/snomedct.html>,
//! "Code"), so it locates like any other code: this module parses it, checks
//! every concept it names against the edition, and hands the operations an
//! opaque concept handle they read the same way.
//!
//! Validation is the first and the third of the three checks §7.3 Validating
//! lists: "Expressions must conform to the syntax defined before" and "All
//! concept references included in the expression must be valid" (the
//! Compositional Grammar specification). The second, conformance to the
//! concept model, is not made here.
//!
//! Subsumption is decided over the edition's inferred view, and its boundary
//! is recorded on the `entails` method below.

use std::collections::BTreeMap;
use std::sync::Arc;

use concept_graph::attributes::ValueRef;
use concept_graph::ordinal::Ordinal;
use concept_graph::subsumption::Outcome;
use roaring::RoaringBitmap;
use sct_scg::ast;

use crate::normal_form;
use crate::provider::{Concept, Property, PropertyValue, ProviderError};
use crate::snomed::{SnomedProvider, storage};

/// The first concept handle an expression takes.
///
/// No FHIR/SNOMED spec governs this: our own design. A handle is an index into
/// one code system version, and an edition's concepts fill the low half of the
/// `u32`, so the high half is free for the expressions a session has seen.
const BASE: u32 = 1 << 31;

/// How many expressions one edition keeps resolved at a time.
///
/// No FHIR/SNOMED spec governs this: our own design. A client mints
/// expressions, so the table is bounded; a handle is minted once and never
/// reused, so an evicted one resolves to nothing rather than to another
/// expression.
const CAPACITY: usize = 4096;

/// Why an expression is not a code of this edition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExpressionError {
    /// The text is not an expression in Compositional Grammar.
    #[error("the expression is not valid compositional grammar: {source}")]
    Syntax {
        /// The parser's failure, which carries the byte offset.
        #[source]
        source: sct_scg::ParseError,
    },
    /// The expression names a concept the edition does not define.
    #[error("the expression names concept `{code}`, which this version does not define")]
    UnknownConcept {
        /// The concept identifier as the expression wrote it.
        code: String,
    },
}

/// A concept handle that no longer names a resolved expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expression handle {handle} is not resolved; an edition keeps {CAPACITY} expressions")]
pub struct StaleExpression {
    /// The handle the operation held.
    pub handle: u32,
}

/// The table's lock, poisoned by a panic in another request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the expression table is poisoned")]
pub struct PoisonedTable;

/// One attribute of a resolved expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Attribute {
    /// The attribute type, as a store ordinal.
    kind: u32,
    /// The value.
    value: Value,
}

/// What a resolved attribute is equal to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    /// A concept or a nested sub-expression.
    ///
    /// Boxed: an enum costs its widest variant for every value, and a nested
    /// sub-expression is the rare and by far the widest one.
    Concept(Box<Term>),
    /// A concrete number, in its lexical form.
    Number(String),
    /// A concrete string.
    Text(String),
}

/// A resolved `subExpression`: focus concepts and a refinement, over store
/// ordinals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Term {
    /// The focus concepts, as store ordinals.
    focus: Vec<u32>,
    /// The ungrouped attributes.
    ungrouped: Vec<Attribute>,
    /// The attribute groups, each non-empty.
    groups: Vec<Vec<Attribute>>,
}

/// One expression this edition has resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Registered {
    /// The parsed tree, kept so a display can be rendered per language.
    tree: ast::Expression,
    /// The resolved focus concepts and refinement.
    term: Term,
    /// The expression rendered in one canonical order, concept ids only.
    canonical: String,
    /// Whether the expression states conditions that are sufficient
    /// (`===`, the assumed status), rather than necessary only (`<<<`).
    defined: bool,
}

impl Registered {
    /// The expression in one canonical order, concept ids only.
    pub(super) fn canonical(&self) -> &str {
        &self.canonical
    }

    /// Whether the expression states conditions that are sufficient, the
    /// status `===` carries and the grammar assumes (§6.7).
    pub(super) const fn sufficiently_defined(&self) -> bool {
        self.defined
    }

    /// The focus concepts of the outermost sub-expression, as store ordinals.
    pub(super) fn focus_concepts(&self) -> &[u32] {
        &self.term.focus
    }
}

/// One side of a subsumption test.
#[derive(Debug, Clone)]
enum Subject {
    /// A concept the edition enumerates.
    Concept {
        /// Its store ordinal.
        ordinal: u32,
        /// Its code, which is its identity.
        code: String,
        /// Whether its conditions are sufficient (`definitionStatusId`).
        defined: bool,
    },
    /// An expression this session resolved.
    Expression(Arc<Registered>),
}

impl Subject {
    /// What identifies this side: the code, or the canonical expression.
    fn key(&self) -> &str {
        match self {
            Self::Concept { code, .. } => code,
            Self::Expression(record) => &record.canonical,
        }
    }

    /// The focus concepts and refinement this side states.
    ///
    /// A concept is a term with itself as its one focus concept; `expanded`
    /// reads its attributes from the inferred view.
    fn term(&self) -> Term {
        match self {
            Self::Concept { ordinal, .. } => Term {
                focus: vec![*ordinal],
                ..Term::default()
            },
            Self::Expression(record) => record.term.clone(),
        }
    }
}

/// Whether a rendering carries the edition's terms, and in which language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terms<'a> {
    /// Concept identifiers alone, the form `normalFormTerse` asks for.
    None,
    /// The edition's terms, in `language` or in the edition's default.
    In(Option<&'a str>),
}

/// What a subject necessarily is: its supertypes and its attributes.
#[derive(Debug, Default)]
struct Expanded {
    /// Every focus concept and every ancestor of one.
    supertypes: RoaringBitmap,
    /// The ungrouped attributes.
    ungrouped: Vec<Attribute>,
    /// The attribute groups.
    groups: Vec<Vec<Attribute>>,
}

/// The expressions one edition has resolved, by canonical form and by handle.
#[derive(Debug, Default)]
pub(super) struct Expressions {
    handles: BTreeMap<String, u32>,
    records: BTreeMap<u32, Arc<Registered>>,
    next: u32,
}

impl Expressions {
    /// The handle of `record`, minting one when the canonical form is new.
    fn intern(&mut self, record: Registered) -> u32 {
        if let Some(handle) = self.handles.get(&record.canonical) {
            return *handle;
        }
        if self.next >= BASE {
            self.handles.clear();
            self.records.clear();
            self.next = 0;
        }
        let handle = BASE.saturating_add(self.next);
        self.next = self.next.saturating_add(1);
        self.handles.insert(record.canonical.clone(), handle);
        self.records.insert(handle, Arc::new(record));
        while self.records.len() > CAPACITY {
            let Some((_, evicted)) = self.records.pop_first() else {
                break;
            };
            self.handles.remove(&evicted.canonical);
        }
        handle
    }
}

/// Every concept identifier a sub-expression names, focus concepts, attribute
/// types, attribute values, and everything a nested sub-expression names.
fn named_concepts(sub: &ast::SubExpression) -> Vec<&str> {
    let mut out: Vec<&str> = sub
        .focus
        .iter()
        .map(|concept| concept.concept_id.as_str())
        .collect();
    let Some(refinement) = &sub.refinement else {
        return out;
    };
    let groups = refinement.groups.iter().flat_map(|g| g.attributes.iter());
    for attribute in refinement.ungrouped.iter().chain(groups) {
        out.push(&attribute.name.concept_id);
        match &attribute.value {
            ast::AttributeValue::Concept(concept) => out.push(&concept.concept_id),
            ast::AttributeValue::Nested(nested) => out.extend(named_concepts(nested)),
            ast::AttributeValue::Text(_) | ast::AttributeValue::Number(_) => {}
        }
    }
    out
}

/// A reference the renderer writes: the code, and the term when one is asked
/// for.
fn reference(code: &str, term: Option<String>) -> normal_form::Reference {
    normal_form::Reference {
        // A term carrying the grammar's own delimiter cannot be written and no
        // escape exists, so the reference drops it.
        term: term.filter(|term| !term.contains('|')),
        code: code.to_owned(),
    }
}

impl SnomedProvider {
    /// Whether `concept` is a handle this module minted.
    pub(super) const fn is_expression_handle(concept: Concept) -> bool {
        concept.index() >= BASE
    }

    /// The expression behind `concept`, or `None` for a concept of the store.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when `concept` is an expression
    /// handle the table no longer holds, which is a defective handle and never
    /// a code that is absent, and when the table's lock is poisoned.
    pub(super) fn registered(
        &self,
        concept: Concept,
    ) -> Result<Option<Arc<Registered>>, ProviderError> {
        if !Self::is_expression_handle(concept) {
            return Ok(None);
        }
        let table = self
            .scg
            .lock()
            .map_err(|_poisoned| ProviderError::Storage(Box::new(PoisonedTable)))?;
        match table.records.get(&concept.index()) {
            Some(record) => Ok(Some(Arc::clone(record))),
            None => Err(ProviderError::Storage(Box::new(StaleExpression {
                handle: concept.index(),
            }))),
        }
    }

    /// Resolves `code` as an expression and returns its handle and canonical
    /// form.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError`] for text the grammar does not admit and for
    /// a concept the edition does not define, and [`ProviderError`] when the
    /// substrate fails.
    pub(super) fn resolve_expression(
        &self,
        code: &str,
    ) -> Result<Result<(Concept, String), ExpressionError>, ProviderError> {
        let tree = match sct_scg::parse(code) {
            Ok(tree) => tree,
            Err(source) => return Ok(Err(ExpressionError::Syntax { source })),
        };
        let term = match self.resolve_sub(&tree.sub_expression)? {
            Ok(term) => term,
            Err(error) => return Ok(Err(error)),
        };
        let defined = tree.status() == ast::DefinitionStatus::EquivalentTo;
        let canonical = self.render(&tree, defined, Terms::None)?;
        let record = Registered {
            tree,
            term,
            canonical: canonical.clone(),
            defined,
        };
        let mut table = self
            .scg
            .lock()
            .map_err(|_poisoned| ProviderError::Storage(Box::new(PoisonedTable)))?;
        Ok(Ok((Concept::new(table.intern(record)), canonical)))
    }

    /// The ordinal of every concept a sub-expression names, or the first one
    /// the edition does not define.
    fn resolve_sub(
        &self,
        sub: &ast::SubExpression,
    ) -> Result<Result<Term, ExpressionError>, ProviderError> {
        let mut focus = Vec::with_capacity(sub.focus.len());
        for concept in &sub.focus {
            match self.store.ordinal(&concept.concept_id).map_err(storage)? {
                Some(ordinal) => focus.push(ordinal.index()),
                None => {
                    return Ok(Err(ExpressionError::UnknownConcept {
                        code: concept.concept_id.clone(),
                    }));
                }
            }
        }
        let mut term = Term {
            focus,
            ..Term::default()
        };
        let Some(refinement) = &sub.refinement else {
            return Ok(Ok(term));
        };
        match self.resolve_attributes(&refinement.ungrouped)? {
            Ok(attributes) => term.ungrouped = attributes,
            Err(error) => return Ok(Err(error)),
        }
        for group in &refinement.groups {
            match self.resolve_attributes(&group.attributes)? {
                Ok(attributes) => term.groups.push(attributes),
                Err(error) => return Ok(Err(error)),
            }
        }
        Ok(Ok(term))
    }

    /// The resolved form of one attribute set.
    fn resolve_attributes(
        &self,
        attributes: &[ast::Attribute],
    ) -> Result<Result<Vec<Attribute>, ExpressionError>, ProviderError> {
        let mut out = Vec::with_capacity(attributes.len());
        for attribute in attributes {
            let Some(kind) = self
                .store
                .ordinal(&attribute.name.concept_id)
                .map_err(storage)?
            else {
                return Ok(Err(ExpressionError::UnknownConcept {
                    code: attribute.name.concept_id.clone(),
                }));
            };
            let value = match &attribute.value {
                ast::AttributeValue::Concept(concept) => {
                    match self.store.ordinal(&concept.concept_id).map_err(storage)? {
                        Some(target) => Value::Concept(Box::new(Term {
                            focus: vec![target.index()],
                            ..Term::default()
                        })),
                        None => {
                            return Ok(Err(ExpressionError::UnknownConcept {
                                code: concept.concept_id.clone(),
                            }));
                        }
                    }
                }
                ast::AttributeValue::Nested(nested) => match self.resolve_sub(nested)? {
                    Ok(term) => Value::Concept(Box::new(term)),
                    Err(error) => return Ok(Err(error)),
                },
                ast::AttributeValue::Number(number) => Value::Number(number.clone()),
                ast::AttributeValue::Text(text) => Value::Text(text.clone()),
            };
            out.push(Attribute {
                kind: kind.index(),
                value,
            });
        }
        Ok(Ok(out))
    }

    /// The expression rendered in one canonical order, with the edition's
    /// terms when `language` asks for them.
    ///
    /// The renderer is the one the precoordinated normal form uses, so an
    /// expression and a concept definition are written the same way
    /// (`crate::normal_form`).
    fn render(
        &self,
        tree: &ast::Expression,
        defined: bool,
        language: Terms<'_>,
    ) -> Result<String, ProviderError> {
        let expression = normal_form::Expression {
            defined,
            focus: self.render_focus(&tree.sub_expression, language)?,
            attributes: self.render_refinement(&tree.sub_expression, language)?,
        };
        Ok(expression
            .render()
            .unwrap_or_else(|| expression.body().clone()))
    }

    /// The focus concepts of a sub-expression as the renderer takes them.
    fn render_focus(
        &self,
        sub: &ast::SubExpression,
        language: Terms<'_>,
    ) -> Result<Vec<normal_form::Reference>, ProviderError> {
        let mut out = Vec::with_capacity(sub.focus.len());
        for concept in &sub.focus {
            out.push(reference(
                &concept.concept_id,
                self.term_of(&concept.concept_id, language)?,
            ));
        }
        Ok(out)
    }

    /// The refinement of a sub-expression as the renderer takes it, with the
    /// ungrouped attributes in group `0` and each group numbered from `1`.
    fn render_refinement(
        &self,
        sub: &ast::SubExpression,
        language: Terms<'_>,
    ) -> Result<Vec<normal_form::Attribute>, ProviderError> {
        let mut out = Vec::new();
        let Some(refinement) = &sub.refinement else {
            return Ok(out);
        };
        for attribute in &refinement.ungrouped {
            out.push(self.render_attribute(attribute, 0, language)?);
        }
        for (index, group) in refinement.groups.iter().enumerate() {
            let number = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            for attribute in &group.attributes {
                out.push(self.render_attribute(attribute, number, language)?);
            }
        }
        Ok(out)
    }

    /// One attribute as the renderer takes it.
    fn render_attribute(
        &self,
        attribute: &ast::Attribute,
        group: u32,
        language: Terms<'_>,
    ) -> Result<normal_form::Attribute, ProviderError> {
        let value = match &attribute.value {
            ast::AttributeValue::Concept(concept) => normal_form::Value::Concept(reference(
                &concept.concept_id,
                self.term_of(&concept.concept_id, language)?,
            )),
            ast::AttributeValue::Nested(nested) => {
                normal_form::Value::Nested(Box::new(normal_form::Expression {
                    defined: true,
                    focus: self.render_focus(nested, language)?,
                    attributes: self.render_refinement(nested, language)?,
                }))
            }
            ast::AttributeValue::Number(number) => normal_form::Value::Number(number.clone()),
            ast::AttributeValue::Text(text) => normal_form::Value::Text(text.clone()),
        };
        Ok(normal_form::Attribute {
            group,
            kind: reference(
                &attribute.name.concept_id,
                self.term_of(&attribute.name.concept_id, language)?,
            ),
            value,
        })
    }

    /// The edition's term for a concept the expression names, when the
    /// rendering carries terms.
    ///
    /// SNOMED International "does not define terms for expressions", and where
    /// no expression repository publishes one "the full expression with terms
    /// embedded may be used" (<https://hl7.org/fhir/R4B/snomedct.html>,
    /// "Display"), so the terms are this edition's own.
    fn term_of(&self, code: &str, language: Terms<'_>) -> Result<Option<String>, ProviderError> {
        let Terms::In(language) = language else {
            return Ok(None);
        };
        let Some(ordinal) = self.store.ordinal(code).map_err(storage)? else {
            return Ok(None);
        };
        self.choose_display(ordinal, language)
    }

    /// The display of an expression: the expression with this edition's terms
    /// written in.
    ///
    /// "SNOMED International does not define terms for expressions … If no
    /// term or description template has been published, the full expression
    /// with terms embedded may be used"
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Display").
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails.
    pub(super) fn expression_display(
        &self,
        record: &Registered,
        language: Option<&str>,
    ) -> Result<String, ProviderError> {
        self.render(&record.tree, record.defined, Terms::In(language))
    }

    /// Whether every concept an expression names is active in this version.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails.
    pub(super) fn expression_is_active(&self, record: &Registered) -> Result<bool, ProviderError> {
        for code in named_concepts(&record.tree.sub_expression) {
            let Some(ordinal) = self.store.ordinal(code).map_err(storage)? else {
                return Ok(false);
            };
            if !self
                .store
                .concept(ordinal)
                .map_err(storage)?
                .is_some_and(|concept| concept.active)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The attributes of an expression's own refinement, as the concept-model
    /// properties keyed by attribute concept id that the FHIR SNOMED CT page
    /// defines for "the given code or expression"
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties").
    ///
    /// A nested sub-expression has no `code` to carry as a property value, so
    /// its attribute is left out; the normal form renders it in full.
    pub(super) fn expression_attributes(record: &Registered) -> Vec<Property> {
        let Some(refinement) = &record.tree.sub_expression.refinement else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let groups = refinement.groups.iter().flat_map(|g| g.attributes.iter());
        for attribute in refinement.ungrouped.iter().chain(groups) {
            let value = match &attribute.value {
                ast::AttributeValue::Concept(concept) => {
                    PropertyValue::Code(concept.concept_id.clone())
                }
                ast::AttributeValue::Text(text) => PropertyValue::String(text.clone()),
                ast::AttributeValue::Number(number) => PropertyValue::Decimal(number.clone()),
                ast::AttributeValue::Nested(_) => continue,
            };
            out.push(Property {
                code: attribute.name.concept_id.clone(),
                value,
                ..Property::default()
            });
        }
        out
    }

    /// The Necessary Normal Form of an expression, with terms when `terms`
    /// asks.
    ///
    /// The focus concepts are the expression's own, with any that another
    /// focus concept is subsumed by dropped as redundant, and the attributes
    /// are the inferred attributes of each focus concept beside the
    /// expression's own refinement. No further transformation runs: replacing
    /// a defined focus concept by its definition "often results in redundancy
    /// or duplication of meaning", and "use of description logic classifier is
    /// more effective way to normalize and compare expressions"
    /// (<https://docs.snomed.org/snomed-international-documents/snomed-ct-glossary/n/normal-form>).
    pub(super) fn expression_normal_form(
        &self,
        record: &Registered,
        terms: bool,
    ) -> Result<Option<String>, ProviderError> {
        let language = if terms { Terms::In(None) } else { Terms::None };
        let minimal = self.minimal_focus(&record.term.focus);
        let mut focus = Vec::with_capacity(minimal.len());
        for ordinal in &minimal {
            let Some(code) = self
                .store
                .code(Ordinal::new(*ordinal))
                .map_err(storage)?
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let term = self.term_of(&code, language)?;
            focus.push(reference(&code, term));
        }
        if focus.is_empty() {
            return Ok(None);
        }
        let mut attributes = self.render_refinement(&record.tree.sub_expression, language)?;
        let mut next = attributes
            .iter()
            .map(|attribute| attribute.group)
            .max()
            .unwrap_or(0);
        for ordinal in &minimal {
            let inherited = self.inferred_attributes(Ordinal::new(*ordinal), next, language)?;
            next = inherited
                .iter()
                .map(|attribute| attribute.group)
                .max()
                .unwrap_or(next);
            attributes.extend(inherited);
        }
        Ok(normal_form::Expression {
            defined: record.defined,
            focus,
            attributes,
        }
        .render())
    }

    /// The inferred attributes of one concept, with its groups renumbered
    /// above `offset` so two concepts never share a group.
    fn inferred_attributes(
        &self,
        ordinal: Ordinal,
        offset: u32,
        language: Terms<'_>,
    ) -> Result<Vec<normal_form::Attribute>, ProviderError> {
        let mut out = Vec::new();
        for row in self.attributes.rows(ordinal) {
            let index = usize::try_from(row.kind).unwrap_or(usize::MAX);
            let Some(kind) = self.attributes.types().get(index).map(u64::to_string) else {
                continue;
            };
            let value = match row.value {
                ValueRef::Concept(target) => {
                    let Some(code) = self
                        .store
                        .code(target)
                        .map_err(storage)?
                        .map(ToOwned::to_owned)
                    else {
                        continue;
                    };
                    let term = self.term_of(&code, language)?;
                    normal_form::Value::Concept(reference(&code, term))
                }
                ValueRef::Number(number) => normal_form::Value::Number(number.to_owned()),
                ValueRef::String(text) => normal_form::Value::Text(text.to_owned()),
            };
            let group = if row.group == 0 {
                0
            } else {
                offset.saturating_add(row.group)
            };
            let term = self.term_of(&kind, language)?;
            out.push(normal_form::Attribute {
                group,
                kind: reference(&kind, term),
                value,
            });
        }
        Ok(out)
    }

    /// The subsumption between two codes when at least one is an expression.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails.
    pub(super) fn expression_subsumes(
        &self,
        a: Concept,
        b: Concept,
    ) -> Result<Outcome, ProviderError> {
        let a = self.subject(a)?;
        let b = self.subject(b)?;
        let a_under_b = self.entails(&a, &b)?;
        let b_under_a = self.entails(&b, &a)?;
        Ok(match (a_under_b, b_under_a) {
            (true, true) => Outcome::Equivalent,
            (true, false) => Outcome::SubsumedBy,
            (false, true) => Outcome::Subsumes,
            (false, false) => Outcome::NotSubsumed,
        })
    }

    /// What one side of a subsumption test is.
    fn subject(&self, concept: Concept) -> Result<Subject, ProviderError> {
        if let Some(record) = self.registered(concept)? {
            return Ok(Subject::Expression(record));
        }
        let ordinal = Ordinal::new(concept.index());
        // A concept with no code is no concept of this version, and two of
        // them would compare equal by an empty identity, which is a wrong
        // subsumption rather than a missing one.
        let Some(code) = self.store.code(ordinal).map_err(storage)? else {
            return Err(ProviderError::CannotDetermine(format!(
                "concept {} is not in this version",
                concept.index()
            )));
        };
        Ok(Subject::Concept {
            ordinal: concept.index(),
            code: code.to_owned(),
            defined: self.is_defined(ordinal)?,
        })
    }

    /// Whether `sup` subsumes `sub`.
    ///
    /// The test is the classic one over the edition's inferred view: every
    /// focus concept of the subsumer subsumes a focus concept of the subsumee,
    /// and every attribute and attribute group of the subsumer is matched by
    /// one of the subsumee whose type and value are subsumed.
    ///
    /// It is sound and it is not complete. SNOMED International withdrew
    /// normal-form comparison as a subsumption test in the July 2019
    /// International Edition and directs implementers to "a description logic
    /// classifier"
    /// (<https://docs.snomed.org/snomed-international-documents/snomed-ct-glossary/n/normal-form>),
    /// and no reasoner runs here. What this answers `subsumes` or
    /// `subsumed-by` holds in the inferred view; what it answers
    /// `not-subsumed` is what that view does not state.
    fn entails(&self, sub: &Subject, sup: &Subject) -> Result<bool, ProviderError> {
        if sub.key() == sup.key() {
            return Ok(true);
        }
        let requirements = self.requirements(sup)?;
        if requirements.is_empty() {
            return Ok(false);
        }
        let expanded = self.expanded(&sub.term())?;
        for requirement in requirements {
            if self.satisfies(&expanded, &requirement, 0)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The conditions a code states that are sufficient for subsumption.
    ///
    /// A concept of the edition is one: being subsumed by it is being under it
    /// in the inferred hierarchy, and a sufficiently defined concept is also
    /// satisfied by its own definition. An expression with the `===` status
    /// states its focus concepts and refinement; one with `<<<` states
    /// conditions that are "necessary but not necessarily sufficient" (the
    /// Compositional Grammar specification, §6.7), so nothing but the same
    /// expression is under it.
    fn requirements(&self, sup: &Subject) -> Result<Vec<Term>, ProviderError> {
        match sup {
            Subject::Concept {
                ordinal, defined, ..
            } => {
                let mut out = vec![Term {
                    focus: vec![*ordinal],
                    ..Term::default()
                }];
                if *defined {
                    out.push(self.definition_term(Ordinal::new(*ordinal))?);
                }
                Ok(out)
            }
            Subject::Expression(record) if record.defined => Ok(vec![record.term.clone()]),
            Subject::Expression(_) => Ok(Vec::new()),
        }
    }

    /// The inferred definition of a concept as a term: its `is a` parents as
    /// focus concepts and its other inferred rows as the refinement.
    fn definition_term(&self, ordinal: Ordinal) -> Result<Term, ProviderError> {
        let focus = self.hierarchy.graph.is_a.neighbours(ordinal).to_vec();
        let (ungrouped, groups) = self.inferred_term(ordinal)?;
        Ok(Term {
            focus,
            ungrouped,
            groups,
        })
    }

    /// What a term necessarily is: every supertype of its focus concepts, and
    /// every attribute it carries.
    ///
    /// A concept's inferred rows are its whole necessary attribute set,
    /// because "the inferred view is the necessary normal form of the concept
    /// definitions, following classification" (the SNOMED CT Release File
    /// Specification, Appendix D), so no supertype walk gathers more.
    fn expanded(&self, term: &Term) -> Result<Expanded, ProviderError> {
        let mut supertypes = RoaringBitmap::new();
        let mut ungrouped = term.ungrouped.clone();
        let mut groups = term.groups.clone();
        for focus in &term.focus {
            let ordinal = Ordinal::new(*focus);
            supertypes |= self.hierarchy.graph.closure.ancestors_or_self(ordinal);
            let (inherited, inherited_groups) = self.inferred_term(ordinal)?;
            ungrouped.extend(inherited);
            groups.extend(inherited_groups);
        }
        Ok(Expanded {
            supertypes,
            ungrouped,
            groups,
        })
    }

    /// The inferred attributes of one concept, split into the ungrouped ones
    /// and one list per role group.
    fn inferred_term(
        &self,
        ordinal: Ordinal,
    ) -> Result<(Vec<Attribute>, Vec<Vec<Attribute>>), ProviderError> {
        let mut ungrouped = Vec::new();
        let mut groups: BTreeMap<u32, Vec<Attribute>> = BTreeMap::new();
        for row in self.attributes.rows(ordinal) {
            let Some(kind) = self.attribute_ordinal(row.kind)? else {
                continue;
            };
            let value = match row.value {
                ValueRef::Concept(target) => Value::Concept(Box::new(Term {
                    focus: vec![target.index()],
                    ..Term::default()
                })),
                ValueRef::Number(number) => Value::Number(number.to_owned()),
                ValueRef::String(text) => Value::Text(text.to_owned()),
            };
            let attribute = Attribute { kind, value };
            if row.group == 0 {
                ungrouped.push(attribute);
            } else {
                groups.entry(row.group).or_default().push(attribute);
            }
        }
        Ok((ungrouped, groups.into_values().collect()))
    }

    /// The store ordinal of an attribute type, by its index in the attribute
    /// table, resolved once for the edition.
    fn attribute_ordinal(&self, kind: u32) -> Result<Option<u32>, ProviderError> {
        let table = if let Some(table) = self.attribute_ordinals.get() {
            table
        } else {
            let mut built = Vec::with_capacity(self.attributes.types().len());
            for sctid in self.attributes.types() {
                built.push(
                    self.store
                        .ordinal(&sctid.to_string())
                        .map_err(storage)?
                        .map(Ordinal::index),
                );
            }
            self.attribute_ordinals.get_or_init(|| built)
        };
        let index = usize::try_from(kind).unwrap_or(usize::MAX);
        Ok(table.get(index).copied().flatten())
    }

    /// Whether what a subject necessarily is satisfies a requirement.
    fn satisfies(
        &self,
        sub: &Expanded,
        requirement: &Term,
        depth: usize,
    ) -> Result<bool, ProviderError> {
        if depth > sct_scg::NESTING_LIMIT {
            return Ok(false);
        }
        if !requirement
            .focus
            .iter()
            .all(|focus| sub.supertypes.contains(*focus))
        {
            return Ok(false);
        }
        // NOTE: "a grouped set of attributes is more specific than the same
        // attributes that are not grouped" (the SNOMED CT Editorial Guide,
        // Relationship Group), so a grouped attribute answers an ungrouped ask.
        for required in &requirement.ungrouped {
            let mut candidates = sub.ungrouped.iter().chain(sub.groups.iter().flatten());
            if !self.any_matches(&mut candidates, required, depth)? {
                return Ok(false);
            }
        }
        for required in &requirement.groups {
            if !self.group_matches(sub, required, depth)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether one group of the subject answers every attribute of a required
    /// group.
    fn group_matches(
        &self,
        sub: &Expanded,
        required: &[Attribute],
        depth: usize,
    ) -> Result<bool, ProviderError> {
        // NOTE: "Grouping all ungrouped attributes with a relationship type
        // that is allowed to be grouped" (§7.7 Classifying) needs the concept
        // model, so an ungrouped attribute answers no required group.
        for group in &sub.groups {
            let mut all = true;
            for attribute in required {
                if !self.any_matches(&mut group.iter(), attribute, depth)? {
                    all = false;
                    break;
                }
            }
            if all {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether any candidate answers the required attribute.
    fn any_matches<'a>(
        &self,
        candidates: &mut impl Iterator<Item = &'a Attribute>,
        required: &Attribute,
        depth: usize,
    ) -> Result<bool, ProviderError> {
        for candidate in candidates {
            if self.attribute_matches(candidate, required, depth)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether one attribute answers a required one: its type is the required
    /// type or a subtype, and its value is subsumed by the required value.
    fn attribute_matches(
        &self,
        candidate: &Attribute,
        required: &Attribute,
        depth: usize,
    ) -> Result<bool, ProviderError> {
        if !self
            .hierarchy
            .graph
            .closure
            .ancestors_or_self(Ordinal::new(candidate.kind))
            .contains(required.kind)
        {
            return Ok(false);
        }
        match (&candidate.value, &required.value) {
            (Value::Concept(candidate), Value::Concept(required)) => {
                let expanded = self.expanded(candidate)?;
                self.satisfies(&expanded, required, depth.saturating_add(1))
            }
            // NOTE: `attribute = attributeName ws "=" ws attributeValue` (§5)
            // is the grammar's one operator on a value, so a concrete value
            // compares by the release's own lexical form.
            (Value::Number(candidate), Value::Number(required))
            | (Value::Text(candidate), Value::Text(required)) => Ok(candidate == required),
            _ => Ok(false),
        }
    }

    /// The focus concepts with every concept another focus concept is subsumed
    /// by removed, since a conjunction states only its most specific members.
    fn minimal_focus(&self, focus: &[u32]) -> Vec<u32> {
        let mut minimal: Vec<u32> = Vec::with_capacity(focus.len());
        for candidate in focus {
            let strictly_narrower = focus.iter().any(|other| {
                other != candidate
                    && self
                        .hierarchy
                        .graph
                        .closure
                        .ancestors(Ordinal::new(*other))
                        .contains(*candidate)
            });
            if !strictly_narrower && !minimal.contains(candidate) {
                minimal.push(*candidate);
            }
        }
        if minimal.is_empty() {
            minimal.extend_from_slice(focus);
            minimal.dedup();
        }
        minimal
    }
}
