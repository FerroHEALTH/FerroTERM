// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The syntax tree, named after the rules of the normative ABNF
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-compositional-grammar-specification/design/5-syntax-specification>).
//!
//! The tree keeps what the source said and nothing more: the definition status
//! is `None` when the expression omitted it, a term is kept as written, and a
//! concrete number keeps its lexical form. Meaning is the reader's, and the
//! printer writes the tree back in the grammar.

/// `definitionStatus = equivalentTo / subtypeOf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefinitionStatus {
    /// `equivalentTo = "==="`: the conditions are necessary and sufficient.
    EquivalentTo,
    /// `subtypeOf = "<<<"`: the conditions are necessary, not sufficient.
    SubtypeOf,
}

impl DefinitionStatus {
    /// The prefix the grammar writes.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::EquivalentTo => "===",
            Self::SubtypeOf => "<<<",
        }
    }
}

/// `expression = ws [definitionStatus ws] subExpression ws`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    /// The definition status, absent when the expression omitted it.
    ///
    /// "'equivalent to' is the assumed definition status when it is not
    /// explicitly stated" (the Compositional Grammar specification, §6.7
    /// Expressions With a Definition Status), so [`Expression::status`]
    /// answers the assumption and this field answers what was written.
    pub definition_status: Option<DefinitionStatus>,
    /// The expression itself.
    pub sub_expression: SubExpression,
}

impl Expression {
    /// The definition status in force: the one written, else the assumed
    /// `equivalent to` (§6.7).
    #[must_use]
    pub const fn status(&self) -> DefinitionStatus {
        match self.definition_status {
            Some(status) => status,
            None => DefinitionStatus::EquivalentTo,
        }
    }
}

/// `subExpression = focusConcept [ws ":" ws refinement]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubExpression {
    /// `focusConcept`: one or more concept references.
    pub focus: Vec<ConceptReference>,
    /// The refinement, when the sub-expression carries one.
    pub refinement: Option<Refinement>,
}

/// `conceptReference = conceptId [ws "|" ws term ws "|"]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConceptReference {
    /// `conceptId = sctId`: 6 to 18 digits with a non-zero first digit.
    pub concept_id: String,
    /// `term`, as written between the pipes and with the surrounding `ws`
    /// removed; absent when the reference carried none.
    pub term: Option<String>,
}

/// `refinement = (attributeSet / attributeGroup) *( ws ["," ws] attributeGroup )`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refinement {
    /// The leading `attributeSet`, empty when the refinement opened with a
    /// group.
    pub ungrouped: Vec<Attribute>,
    /// The attribute groups, in the order they were written.
    pub groups: Vec<AttributeGroup>,
}

/// `attributeGroup = "{" ws attributeSet ws "}"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeGroup {
    /// `attributeSet = attribute *(ws "," ws attribute)`: one or more.
    pub attributes: Vec<Attribute>,
}

/// `attribute = attributeName ws "=" ws attributeValue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// `attributeName = conceptReference`.
    pub name: ConceptReference,
    /// `attributeValue`.
    pub value: AttributeValue,
}

/// `attributeValue = expressionValue / QM stringValue QM / "#" numericValue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// `expressionValue = conceptReference`.
    Concept(ConceptReference),
    /// `expressionValue = "(" ws subExpression ws ")"`.
    ///
    /// Boxed: a nested sub-expression is the rare and by far the widest
    /// variant, and an enum costs its widest variant for every value.
    Nested(Box<SubExpression>),
    /// `stringValue`, with the quotation marks removed and `\"` and `\\`
    /// unescaped.
    Text(String),
    /// `numericValue`, in its lexical form and without the `"#"`.
    Number(String),
}
