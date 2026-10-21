// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Value set filters (`ValueSet.compose.include.filter`) and their generic
//! evaluation over a provider.
//!
//! The operator set is the `filter-operator` code system
//! (<https://hl7.org/fhir/R5/codesystem-filter-operator.html>); `child-of`
//! and `descendent-leaf` exist from R5 on, and the wire layer of each version
//! admits exactly its own set.

use regex::Regex;

use crate::provider::{CodeSystemProvider, Concept, ConceptSet, Hierarchy, ProviderError};

/// `ValueSet.compose.include.filter.op`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    /// `=`: the property equals the value.
    Equal,
    /// `is-a`: descendants and self.
    IsA,
    /// `descendent-of`: descendants only.
    DescendentOf,
    /// `is-not-a`: not a descendant and not self.
    IsNotA,
    /// `regex`: the property matches the regular expression.
    Regex,
    /// `in`: the property is one of the comma-separated values.
    In,
    /// `not-in`: the property is none of the comma-separated values.
    NotIn,
    /// `generalizes`: ancestors and self.
    Generalizes,
    /// `child-of`: direct children only (R5 and later).
    ChildOf,
    /// `descendent-leaf`: descendants without children (R5 and later).
    DescendentLeaf,
    /// `exists`: the property has a value (`true`) or none (`false`).
    Exists,
}

impl FilterOperator {
    /// Every operator, in code system order.
    pub const ALL: [Self; 11] = [
        Self::Equal,
        Self::IsA,
        Self::DescendentOf,
        Self::IsNotA,
        Self::Regex,
        Self::In,
        Self::NotIn,
        Self::Generalizes,
        Self::ChildOf,
        Self::DescendentLeaf,
        Self::Exists,
    ];

    /// The FHIR code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::IsA => "is-a",
            Self::DescendentOf => "descendent-of",
            Self::IsNotA => "is-not-a",
            Self::Regex => "regex",
            Self::In => "in",
            Self::NotIn => "not-in",
            Self::Generalizes => "generalizes",
            Self::ChildOf => "child-of",
            Self::DescendentLeaf => "descendent-leaf",
            Self::Exists => "exists",
        }
    }

    /// The operator for a FHIR code.
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.code() == code)
    }

    /// Whether the operator exists only from R5 on (`filter-operator|5.0.0`).
    #[must_use]
    pub const fn r5_only(self) -> bool {
        matches!(self, Self::ChildOf | Self::DescendentLeaf)
    }

    /// Whether the operator needs the hierarchy.
    #[must_use]
    pub const fn hierarchical(self) -> bool {
        matches!(
            self,
            Self::IsA
                | Self::DescendentOf
                | Self::IsNotA
                | Self::Generalizes
                | Self::ChildOf
                | Self::DescendentLeaf
        )
    }
}

/// One filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// The property or filter code the system defines.
    pub property: String,
    /// The operator.
    pub op: FilterOperator,
    /// The value.
    pub value: String,
}

/// The generic evaluation every provider gets by default.
///
/// `concept` and `code` name the code itself; `parent` and `child` with `=`
/// walk one hierarchy step; every other property is a declared property
/// compared by its text value over every concept.
///
/// # Errors
///
/// Returns [`ProviderError::UnsupportedFilter`] for a property the system does
/// not declare or a hierarchy operator on a system without a hierarchy,
/// [`ProviderError::UnknownCode`] for a code-valued filter naming an unknown
/// code (spec-silent; refusing beats a silently empty answer), and the
/// regular-expression and value errors.
pub fn evaluate<P: CodeSystemProvider + ?Sized>(
    provider: &P,
    filter: &Filter,
) -> Result<ConceptSet, ProviderError> {
    match filter.property.as_str() {
        "concept" | "code" => on_code(provider, filter),
        "parent" | "child" if filter.op == FilterOperator::Equal => {
            let hierarchy = hierarchy(provider, filter)?;
            let concept = locate(provider, &filter.value)?;
            Ok(if filter.property == "parent" {
                hierarchy.children(concept)
            } else {
                hierarchy.parents(concept)
            })
        }
        property => {
            if !provider
                .declaration()
                .properties
                .iter()
                .any(|definition| definition.code == property)
            {
                return Err(unsupported(filter));
            }
            on_property(provider, filter)
        }
    }
}

fn unsupported(filter: &Filter) -> ProviderError {
    ProviderError::UnsupportedFilter {
        property: filter.property.clone(),
        operator: filter.op.code().to_owned(),
    }
}

fn hierarchy<'a, P: CodeSystemProvider + ?Sized>(
    provider: &'a P,
    filter: &Filter,
) -> Result<&'a dyn Hierarchy, ProviderError> {
    provider.hierarchy().ok_or_else(|| unsupported(filter))
}

fn locate<P: CodeSystemProvider + ?Sized>(
    provider: &P,
    code: &str,
) -> Result<Concept, ProviderError> {
    provider
        .locate(code)?
        .map(|located| located.concept)
        .ok_or_else(|| ProviderError::UnknownCode(code.to_owned()))
}

fn values(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|v| !v.is_empty())
}

// NOTE: `regex` matches the whole property value ("matches the regex specified",
// <https://hl7.org/fhir/R4B/codesystem-filter-operator.html>), so the pattern is anchored.
fn anchored(value: &str) -> Result<Regex, regex::Error> {
    Regex::new(&format!("^(?:{value})$"))
}

pub(crate) fn boolean(filter: &Filter) -> Result<bool, ProviderError> {
    match filter.value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(ProviderError::invalid_filter_value(
            &filter.property,
            other,
            "expected `true` or `false`",
        )),
    }
}

fn on_code<P: CodeSystemProvider + ?Sized>(
    provider: &P,
    filter: &Filter,
) -> Result<ConceptSet, ProviderError> {
    let mut set = ConceptSet::new();
    match filter.op {
        FilterOperator::Equal => {
            set.insert(locate(provider, &filter.value)?.index());
        }
        FilterOperator::In => {
            for code in values(&filter.value) {
                set.insert(locate(provider, code)?.index());
            }
        }
        FilterOperator::NotIn => {
            set = provider.all()?;
            for code in values(&filter.value) {
                set.remove(locate(provider, code)?.index());
            }
        }
        FilterOperator::Regex => {
            let regex = anchored(&filter.value)?;
            for index in provider.all()? {
                if provider
                    .code(Concept::new(index))?
                    .is_some_and(|code| regex.is_match(&code))
                {
                    set.insert(index);
                }
            }
        }
        FilterOperator::Exists => {
            if boolean(filter)? {
                set = provider.all()?;
            }
        }
        FilterOperator::IsA
        | FilterOperator::DescendentOf
        | FilterOperator::IsNotA
        | FilterOperator::Generalizes
        | FilterOperator::ChildOf
        | FilterOperator::DescendentLeaf => {
            let hierarchy = hierarchy(provider, filter)?;
            let concept = locate(provider, &filter.value)?;
            set = match filter.op {
                FilterOperator::IsA => with_self(hierarchy.descendants(concept), concept),
                FilterOperator::DescendentOf => hierarchy.descendants(concept),
                FilterOperator::IsNotA => {
                    provider.all()? - with_self(hierarchy.descendants(concept), concept)
                }
                FilterOperator::Generalizes => with_self(hierarchy.ancestors(concept), concept),
                FilterOperator::ChildOf => hierarchy.children(concept),
                _ => hierarchy
                    .descendants(concept)
                    .into_iter()
                    .filter(|index| hierarchy.children(Concept::new(*index)).is_empty())
                    .collect(),
            };
        }
    }
    Ok(set)
}

fn with_self(mut set: ConceptSet, concept: Concept) -> ConceptSet {
    set.insert(concept.index());
    set
}

fn on_property<P: CodeSystemProvider + ?Sized>(
    provider: &P,
    filter: &Filter,
) -> Result<ConceptSet, ProviderError> {
    if filter.op.hierarchical() {
        return Err(unsupported(filter));
    }
    let regex = if filter.op == FilterOperator::Regex {
        Some(anchored(&filter.value)?)
    } else {
        None
    };
    let wanted: Vec<&str> = values(&filter.value).collect();
    let exists = if filter.op == FilterOperator::Exists {
        Some(boolean(filter)?)
    } else {
        None
    };
    let mut set = ConceptSet::new();
    for index in provider.all()? {
        let texts: Vec<String> = provider
            .properties(Concept::new(index))?
            .into_iter()
            .filter(|property| property.code == filter.property)
            .map(|property| property.value.as_text())
            .collect();
        let selected = match filter.op {
            FilterOperator::Equal => texts.iter().any(|text| text == &filter.value),
            FilterOperator::In => texts.iter().any(|text| wanted.contains(&text.as_str())),
            FilterOperator::NotIn => !texts.iter().any(|text| wanted.contains(&text.as_str())),
            FilterOperator::Regex => texts
                .iter()
                .any(|text| regex.as_ref().is_some_and(|regex| regex.is_match(text))),
            FilterOperator::Exists => exists.is_some_and(|wanted| wanted != texts.is_empty()),
            _ => false,
        };
        if selected {
            set.insert(index);
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::FilterOperator;

    #[test]
    fn operators_round_trip_their_codes_and_flag_the_r5_additions() {
        for op in FilterOperator::ALL {
            assert_eq!(FilterOperator::parse(op.code()), Some(op));
        }
        assert_eq!(FilterOperator::parse("equals"), None);
        let r5_only: Vec<&str> = FilterOperator::ALL
            .into_iter()
            .filter(|op| op.r5_only())
            .map(FilterOperator::code)
            .collect();
        assert_eq!(r5_only, vec!["child-of", "descendent-leaf"]);
        assert_eq!(FilterOperator::ALL.len(), 11);
    }
}
