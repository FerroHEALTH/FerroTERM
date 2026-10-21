// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The UCUM provider over the seam.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use concept_graph::subsumption::Outcome;

use super::canonical::{Canonical, Reducer};
use super::essence::{ESSENCE_DATA, Essence};
use super::grammar::{Atom, Expression, parse};
use crate::compose::{Compose, Include, SystemRef};
use crate::filter::{Filter, FilterOperator};
use crate::provider::{
    CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration, Designation,
    DesignationUse, FilterDefinition, Hierarchy, Identity, ImplicitArgument, ImplicitForm, Located,
    Property, PropertyDefinition, PropertyKind, PropertyValue, ProviderError, Status,
};
use crate::registries::interned::Interned;

/// The system URI (<https://terminology.hl7.org/UCUM.html>).
pub const URL: &str = "http://unitsofmeasure.org";

/// The language of the unit names this provider renders.
///
/// `ucum-essence.xml` gives each prefix and unit one `<name>`, in English, and
/// no translation. No FHIR/SNOMED spec governs this: our own design.
const BASE_LANGUAGE: &str = "en";

/// The UCUM provider: every valid expression is a code.
#[derive(Debug)]
pub struct UcumProvider {
    identity: Identity,
    declaration: Declaration,
    interned: Interned,
    essence: &'static Essence,
    reducer: Mutex<Reducer<'static>>,
}

impl Default for UcumProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl UcumProvider {
    /// The provider over the vendored essence.
    #[must_use]
    pub fn new() -> Self {
        let essence: &'static Essence = &ESSENCE_DATA;
        Self {
            identity: Identity {
                url: URL.to_owned(),
                version: essence.version.clone(),
                title: Some(String::from("Unified Code for Units of Measure (UCUM)")),
                name: None,
                version_needed: false,
            },
            declaration: Declaration {
                content: ContentMode::NotPresent,
                case_sensitive: true,
                hierarchy_meaning: None,
                // NOTE: UCUM §2 defines the unit expression grammar, and this
                // provider parses and canonicalizes it, so the grammar is both
                // defined and supported.
                compositional: Compositional::Supported,
                languages: vec![String::from(BASE_LANGUAGE)],
                properties: vec![
                    PropertyDefinition {
                        code: String::from("canonical"),
                        uri: None,
                        description: Some(String::from(
                            "The canonical form over the base units, the magnitude dropped",
                        )),
                        kind: PropertyKind::Code,
                    },
                    PropertyDefinition {
                        code: String::from("property"),
                        uri: None,
                        description: Some(String::from("The kind of quantity the unit measures")),
                        kind: PropertyKind::Code,
                    },
                ],
                filters: vec![
                    FilterDefinition {
                        code: String::from("canonical"),
                        description: Some(String::from("Units commensurable with the expression")),
                        operators: vec![FilterOperator::Equal, FilterOperator::In],
                        value: String::from("a UCUM expression"),
                    },
                    FilterDefinition {
                        code: String::from("property"),
                        description: Some(String::from("Units of this kind of quantity")),
                        operators: vec![FilterOperator::Equal],
                        value: String::from("a property such as `mass`"),
                    },
                ],
                capabilities: BTreeSet::new(),
                // NOTE: the `/vs` and `/vs/[expression]` forms of
                // <https://terminology.hl7.org/UCUM.html>.
                implicit_forms: vec![
                    ImplicitForm::new("http://unitsofmeasure.org/vs", ImplicitArgument::None),
                    ImplicitForm::new(
                        "http://unitsofmeasure.org/vs/[expression]",
                        ImplicitArgument::Expression,
                    ),
                ],
            },
            interned: Interned::new(),
            essence,
            reducer: Mutex::new(Reducer::new(essence)),
        }
    }

    /// The parsed form of `code`, when it is an expression.
    #[must_use]
    pub fn expression(&self, code: &str) -> Option<Expression> {
        parse(code, self.essence).ok()
    }

    /// The canonical form of an expression.
    fn canonical(&self, expression: &Expression) -> Option<Canonical> {
        self.reducer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reduce(expression)
            .ok()
    }

    /// The canonical form of `code`, when it is an expression.
    #[must_use]
    pub fn canonical_of(&self, code: &str) -> Option<Canonical> {
        let expression = self.expression(code)?;
        self.canonical(&expression)
    }

    fn located(&self, concept: Concept) -> Option<(String, Expression, Canonical)> {
        let code = self.interned.code(concept)?;
        let expression = self.expression(&code)?;
        let canonical = self.canonical(&expression)?;
        Some((code, expression, canonical))
    }

    /// The kind of quantity: the atom's property for a single atom, else the
    /// base unit's property when the form is one base unit.
    fn property(&self, expression: &Expression, canonical: &Canonical) -> Option<String> {
        if let [component] = expression.components.as_slice()
            && component.exponent == 1
            && let Atom::Unit { code, .. } = &component.atom
        {
            if let Some(unit) = self.essence.units.get(code) {
                return Some(unit.property.clone());
            }
            if let Some(base) = self.essence.base_units.get(code) {
                return Some(base.property.clone());
            }
        }
        canonical.base_property(self.essence).map(str::to_owned)
    }

    /// The English name of an expression, composed from the essence names
    /// (`milligram per deciliter`).
    fn describe(&self, expression: &Expression) -> String {
        let mut numerator = Vec::new();
        let mut denominator = Vec::new();
        for component in &expression.components {
            let name = match &component.atom {
                Atom::Unit { prefix, code } => {
                    let prefix = prefix
                        .as_ref()
                        .and_then(|p| self.essence.prefixes.get(p))
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    let unit = self
                        .essence
                        .units
                        .get(code)
                        .map(|u| u.name.clone())
                        .or_else(|| self.essence.base_units.get(code).map(|b| b.name.clone()))
                        .unwrap_or_else(|| code.clone());
                    format!("{prefix}{unit}")
                }
                Atom::Factor(n) => n.to_string(),
                Atom::Annotation => continue,
                Atom::Group(inner) => format!("({})", self.describe(inner)),
            };
            let power = match component.exponent.abs() {
                1 => name,
                2 => format!("square {name}"),
                3 => format!("cubic {name}"),
                n => format!("{name} to the power {n}"),
            };
            if component.exponent < 0 {
                denominator.push(power);
            } else {
                numerator.push(power);
            }
        }
        let mut text = if numerator.is_empty() {
            String::from("1")
        } else {
            numerator.join(" ")
        };
        for part in denominator {
            text.push_str(" per ");
            text.push_str(&part);
        }
        text
    }

    fn matches(
        &self,
        expression: &Expression,
        canonical: &Canonical,
        filter: &Filter,
    ) -> Result<bool, ProviderError> {
        let unsupported = || ProviderError::UnsupportedFilter {
            property: filter.property.clone(),
            operator: filter.op.code().to_owned(),
        };
        match (filter.property.as_str(), filter.op) {
            ("property", FilterOperator::Equal) => Ok(self
                .property(expression, canonical)
                .is_some_and(|p| p == filter.value.trim())),
            ("canonical", FilterOperator::Equal | FilterOperator::In) => {
                for wanted in filter.value.split(',') {
                    let wanted = wanted.trim();
                    let Some(other) = self.canonical_of(wanted) else {
                        return Err(ProviderError::invalid_filter_value(
                            &filter.property,
                            wanted,
                            "not a UCUM expression",
                        ));
                    };
                    if canonical.commensurable(&other) {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Err(unsupported()),
        }
    }
}

impl CodeSystemProvider for UcumProvider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The language of the unit names this provider returns, so a
    /// `displayLanguage` it has no name in is answered as a language the
    /// system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(&self) -> Option<&str> {
        Some(BASE_LANGUAGE)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        let Some(expression) = self.expression(code) else {
            return Ok(None);
        };
        if self.canonical(&expression).is_none() {
            return Ok(None);
        }
        let concept = self.interned.intern(code)?;
        Ok(Some(Located {
            concept,
            code: code.to_owned(),
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.interned.code(concept))
    }

    fn display(
        &self,
        concept: Concept,
        _language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        Ok(self.interned.code(concept))
    }

    fn status(&self, _concept: Concept) -> Result<Status, ProviderError> {
        Ok(Status {
            standards_status: None,
            active: true,
            inactive_reason: None,
            abstract_concept: false,
            codeless: false,
        })
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        if language.is_some_and(|l| !l.eq_ignore_ascii_case(BASE_LANGUAGE)) {
            return Ok(Vec::new());
        }
        let Some((_, expression, _)) = self.located(concept) else {
            return Ok(Vec::new());
        };
        Ok(vec![Designation {
            standards_status: None,
            language: Some(String::from(BASE_LANGUAGE)),
            use_: Some(DesignationUse {
                system: String::from("http://snomed.info/sct"),
                code: String::from("900000000000013009"),
                display: Some(String::from("Synonym")),
            }),
            value: self.describe(&expression),
        }])
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let Some((_, expression, canonical)) = self.located(concept) else {
            return Ok(Vec::new());
        };
        let mut out = vec![Property {
            code: String::from("canonical"),
            value: PropertyValue::Code(canonical.text(self.essence)),
            ..Property::default()
        }];
        if let Some(property) = self.property(&expression, &canonical) {
            out.push(Property {
                code: String::from("property"),
                value: PropertyValue::Code(property),
                ..Property::default()
            });
        }
        Ok(out)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        None
    }

    /// `http://unitsofmeasure.org/vs` (every unit) and
    /// `http://unitsofmeasure.org/vs/[expression]` (the units commensurable
    /// with the expression) (<https://terminology.hl7.org/UCUM.html>).
    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        let rest = url.strip_prefix(URL)?.strip_prefix("/vs")?;
        let filters = match rest {
            "" => Vec::new(),
            other => {
                let expression = other.strip_prefix('/')?;
                if self.canonical_of(expression).is_none() {
                    return Some(Err(ProviderError::malformed_implicit_value_set(
                        url,
                        format!("`{expression}` is not a UCUM expression"),
                    )));
                }
                vec![Filter {
                    property: String::from("canonical"),
                    op: FilterOperator::Equal,
                    value: expression.to_owned(),
                }]
            }
        };
        Some(Ok(Compose {
            include: vec![Include {
                system: Some(SystemRef {
                    url: URL.to_owned(),
                    version: None,
                }),
                filters,
                ..Include::default()
            }],
            ..Compose::default()
        }))
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Err(ProviderError::NotEnumerable)
    }

    fn search(&self, _text: &str, _language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        Err(ProviderError::NotEnumerable)
    }

    /// A UCUM selection is incomplete: the grammar multiplies and exponentiates
    /// its atoms into unboundedly many units, and `$expand` names UCUM as a
    /// case `valueset-unclosed` marks
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes).
    fn unclosed(&self, _filters: &[Filter]) -> bool {
        true
    }

    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        if self
            .declaration
            .filters
            .iter()
            .any(|f| f.code == filter.property)
        {
            return Err(ProviderError::NotEnumerable);
        }
        Err(ProviderError::UnsupportedFilter {
            property: filter.property.clone(),
            operator: filter.op.code().to_owned(),
        })
    }

    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
        let Some((_, expression, canonical)) = self.located(concept) else {
            return Ok(false);
        };
        self.matches(&expression, &canonical, filter)
    }

    /// Two expressions are the same unit or unrelated: UCUM has no
    /// hierarchy, so `equivalent` and `not-subsumed` are the only answers.
    fn subsumes(&self, a: Concept, b: Concept) -> Result<Option<Outcome>, ProviderError> {
        let (Some((_, _, a)), Some((_, _, b))) = (self.located(a), self.located(b)) else {
            return Ok(None);
        };
        Ok(Some(if a.same_unit(&b) {
            Outcome::Equivalent
        } else {
            Outcome::NotSubsumed
        }))
    }
}
