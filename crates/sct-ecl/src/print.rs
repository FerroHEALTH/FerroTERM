// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The canonical text of a syntax tree: `Display` for every node, in the
//! grammar's own spelling, so `parse(print(tree)) == tree`.

use std::fmt::{self, Display, Formatter};

use crate::ast::{
    Acceptability, AcceptabilitySet, AltIdentifier, Attribute, AttributeSet, AttributeValue,
    Cardinality, Comparison, ConceptFilter, ConceptReference, ConceptSet, ConstraintOperator,
    DefinitionStatus, DescriptionFilter, DialectAlias, DialectIdValue, Equality,
    ExpressionConstraint, FieldValue, FilterConstraint, FocusConcept, HistorySupplement,
    MemberFilter, MemberOf, NumericValue, Refinement, RefsetFields, Sctid, SubAttributeSet,
    SubExpressionConstraint, SubRefinement, TimeValue, TypeToken, TypedSearchTerm,
};

/// Writes `items` separated by `separator`.
fn join<T: Display>(f: &mut Formatter<'_>, items: &[T], separator: &str) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(separator)?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

/// Writes one item bare and several as `( a b )`.
fn bare_or_set<T: Display>(f: &mut Formatter<'_>, items: &[T]) -> fmt::Result {
    match items {
        [one] => write!(f, "{one}"),
        many => {
            f.write_str("(")?;
            join(f, many, " ")?;
            f.write_str(")")
        }
    }
}

/// Writes a quoted string.
fn quoted(f: &mut Formatter<'_>, text: &str) -> fmt::Result {
    write!(f, "\"{text}\"")
}

impl Display for Sctid {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for ConceptReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)?;
        if let Some(term) = &self.term {
            write!(f, " |{term}|")?;
        }
        Ok(())
    }
}

impl Display for AltIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let bare = self
            .code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'));
        if bare {
            write!(f, "{}#{}", self.scheme, self.code)?;
        } else {
            write!(f, "\"{}#{}\"", self.scheme, self.code)?;
        }
        if let Some(term) = &self.term {
            write!(f, " |{term}|")?;
        }
        Ok(())
    }
}

impl Display for FocusConcept {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reference(reference) => write!(f, "{reference}"),
            Self::Wildcard => f.write_str("*"),
            Self::AltIdentifier(alt) => write!(f, "{alt}"),
            Self::Nested(inner) => write!(f, "( {inner} )"),
        }
    }
}

impl Display for ConstraintOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DescendantOf => "<",
            Self::DescendantOrSelfOf => "<<",
            Self::ChildOf => "<!",
            Self::ChildOrSelfOf => "<<!",
            Self::AncestorOf => ">",
            Self::AncestorOrSelfOf => ">>",
            Self::ParentOf => ">!",
            Self::ParentOrSelfOf => ">>!",
            Self::Top => "!!>",
            Self::Bottom => "!!<",
        })
    }
}

impl Display for MemberOf {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("^")?;
        match &self.fields {
            None => Ok(()),
            Some(RefsetFields::Any) => f.write_str(" [*]"),
            Some(RefsetFields::Names(names)) => {
                f.write_str(" [")?;
                join(f, names, ", ")?;
                f.write_str("]")
            }
        }
    }
}

impl Display for SubExpressionConstraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(operator) = self.operator {
            write!(f, "{operator} ")?;
        }
        if let Some(member_of) = &self.member_of {
            write!(f, "{member_of} ")?;
        }
        write!(f, "{}", self.focus)?;
        for filters in &self.member_filters {
            f.write_str(" {{ M ")?;
            join(f, filters, ", ")?;
            f.write_str(" }}")?;
        }
        for filter in &self.filters {
            write!(f, " {filter}")?;
        }
        if let Some(history) = &self.history {
            write!(f, " {history}")?;
        }
        Ok(())
    }
}

impl Display for FilterConstraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Description(filters) => {
                f.write_str("{{ D ")?;
                join(f, filters, ", ")?;
                f.write_str(" }}")
            }
            Self::Concept(filters) => {
                f.write_str("{{ C ")?;
                join(f, filters, ", ")?;
                f.write_str(" }}")
            }
        }
    }
}

impl Display for ExpressionConstraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refined { focus, refinement } => write!(f, "{focus} : {refinement}"),
            Self::Conjunction(operands) => join(f, operands, " AND "),
            Self::Disjunction(operands) => join(f, operands, " OR "),
            Self::Exclusion { left, right } => write!(f, "{left} MINUS {right}"),
            Self::Dotted { focus, attributes } => {
                write!(f, "{focus}")?;
                for attribute in attributes {
                    write!(f, " . {attribute}")?;
                }
                Ok(())
            }
            Self::Sub(sub) => write!(f, "{sub}"),
        }
    }
}

impl Display for Refinement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single(one) => write!(f, "{one}"),
            Self::Conjunction(items) => join(f, items, ", "),
            Self::Disjunction(items) => join(f, items, " OR "),
        }
    }
}

impl Display for SubRefinement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AttributeSet(set) => write!(f, "{set}"),
            Self::Group {
                cardinality,
                attributes,
            } => {
                if let Some(cardinality) = cardinality {
                    write!(f, "{cardinality} ")?;
                }
                write!(f, "{{ {attributes} }}")
            }
            Self::Nested(inner) => write!(f, "( {inner} )"),
        }
    }
}

impl Display for AttributeSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single(one) => write!(f, "{one}"),
            Self::Conjunction(items) => join(f, items, ", "),
            Self::Disjunction(items) => join(f, items, " OR "),
        }
    }
}

impl Display for SubAttributeSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attribute(attribute) => write!(f, "{attribute}"),
            Self::Nested(inner) => write!(f, "( {inner} )"),
        }
    }
}

impl Display for Attribute {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(cardinality) = self.cardinality {
            write!(f, "{cardinality} ")?;
        }
        if self.reverse {
            f.write_str("R ")?;
        }
        write!(f, "{} {}", self.name, self.value)
    }
}

impl Display for AttributeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression { operator, value } => write!(f, "{operator} {value}"),
            Self::Numeric { operator, value } => write!(f, "{operator} {value}"),
            Self::String { operator, terms } => {
                write!(f, "{operator} ")?;
                bare_or_set(f, terms)
            }
            Self::Boolean { operator, value } => write!(f, "{operator} {value}"),
        }
    }
}

impl Display for Cardinality {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.max {
            Some(max) => write!(f, "[{}..{max}]", self.min),
            None => write!(f, "[{}..*]", self.min),
        }
    }
}

impl Display for Equality {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
        })
    }
}

impl Display for Comparison {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        })
    }
}

impl Display for NumericValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl Display for TypedSearchTerm {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Match(words) => {
                f.write_str("\"")?;
                join(f, words, " ")?;
                f.write_str("\"")
            }
            Self::Wild(pattern) => {
                f.write_str("wild:")?;
                quoted(f, pattern)
            }
        }
    }
}

impl Display for DescriptionFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Term { operator, terms } => {
                write!(f, "term {operator} ")?;
                bare_or_set(f, terms)
            }
            Self::Language { operator, codes } => {
                write!(f, "language {operator} ")?;
                bare_or_set(f, codes)
            }
            Self::TypeId { operator, value } => write!(f, "typeId {operator} {value}"),
            Self::Type { operator, tokens } => {
                write!(f, "type {operator} ")?;
                bare_or_set(f, tokens)
            }
            Self::DialectId {
                operator,
                value,
                acceptability,
            } => {
                write!(f, "dialectId {operator} {value}")?;
                if let Some(acceptability) = acceptability {
                    write!(f, " {acceptability}")?;
                }
                Ok(())
            }
            Self::Dialect {
                operator,
                aliases,
                acceptability,
            } => {
                write!(f, "dialect {operator} ")?;
                match aliases.as_slice() {
                    [
                        DialectAlias {
                            alias,
                            acceptability: None,
                        },
                    ] => f.write_str(alias)?,
                    many => {
                        f.write_str("(")?;
                        join(f, many, " ")?;
                        f.write_str(")")?;
                    }
                }
                if let Some(acceptability) = acceptability {
                    write!(f, " {acceptability}")?;
                }
                Ok(())
            }
            Self::Module { operator, value } => write!(f, "moduleId {operator} {value}"),
            Self::EffectiveTime { operator, values } => {
                write!(f, "effectiveTime {operator} ")?;
                bare_or_set(f, values)
            }
            Self::Active { operator, value } => write!(f, "active {operator} {value}"),
            Self::Id { operator, ids } => {
                write!(f, "id {operator} ")?;
                bare_or_set(f, ids)
            }
        }
    }
}

impl Display for ConceptSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression(sub) => write!(f, "{sub}"),
            Self::Set(references) => {
                f.write_str("(")?;
                join(f, references, " ")?;
                f.write_str(")")
            }
        }
    }
}

impl Display for TypeToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Synonym => "syn",
            Self::FullySpecifiedName => "fsn",
            Self::Definition => "def",
        })
    }
}

impl Display for DialectIdValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression(sub) => write!(f, "{sub}"),
            Self::Set(items) => {
                f.write_str("(")?;
                for (i, (reference, acceptability)) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{reference}")?;
                    if let Some(acceptability) = acceptability {
                        write!(f, " {acceptability}")?;
                    }
                }
                f.write_str(")")
            }
        }
    }
}

impl Display for DialectAlias {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.alias)?;
        if let Some(acceptability) = &self.acceptability {
            write!(f, " {acceptability}")?;
        }
        Ok(())
    }
}

impl Display for AcceptabilitySet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        match self {
            Self::Concepts(references) => join(f, references, " ")?,
            Self::Tokens(tokens) => join(f, tokens, " ")?,
        }
        f.write_str(")")
    }
}

impl Display for Acceptability {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Acceptable => "accept",
            Self::Preferred => "prefer",
        })
    }
}

impl Display for TimeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        quoted(f, &self.0)
    }
}

impl Display for ConceptFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DefinitionStatusId { operator, value } => {
                write!(f, "definitionStatusId {operator} {value}")
            }
            Self::DefinitionStatus { operator, tokens } => {
                write!(f, "definitionStatus {operator} ")?;
                bare_or_set(f, tokens)
            }
            Self::Module { operator, value } => write!(f, "moduleId {operator} {value}"),
            Self::EffectiveTime { operator, values } => {
                write!(f, "effectiveTime {operator} ")?;
                bare_or_set(f, values)
            }
            Self::Active { operator, value } => write!(f, "active {operator} {value}"),
        }
    }
}

impl Display for DefinitionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Primitive => "primitive",
            Self::Defined => "defined",
        })
    }
}

impl Display for MemberFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Module { operator, value } => write!(f, "moduleId {operator} {value}"),
            Self::EffectiveTime { operator, values } => {
                write!(f, "effectiveTime {operator} ")?;
                bare_or_set(f, values)
            }
            Self::Active { operator, value } => write!(f, "active {operator} {value}"),
            Self::Field { name, value } => write!(f, "{name} {value}"),
        }
    }
}

impl Display for FieldValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression { operator, value } => write!(f, "{operator} {value}"),
            Self::Numeric { operator, value } => write!(f, "{operator} {value}"),
            Self::String { operator, terms } => {
                write!(f, "{operator} ")?;
                bare_or_set(f, terms)
            }
            Self::Boolean { operator, value } => write!(f, "{operator} {value}"),
            Self::Time { operator, values } => {
                write!(f, "{operator} ")?;
                bare_or_set(f, values)
            }
        }
    }
}

impl Display for HistorySupplement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("{{ + HISTORY }}"),
            Self::Minimum => f.write_str("{{ + HISTORY-MIN }}"),
            Self::Moderate => f.write_str("{{ + HISTORY-MOD }}"),
            Self::Maximum => f.write_str("{{ + HISTORY-MAX }}"),
            Self::Subset(inner) => write!(f, "{{{{ + HISTORY ( {inner} ) }}}}"),
        }
    }
}
