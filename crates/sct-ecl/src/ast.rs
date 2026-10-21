// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The syntax tree of an expression constraint, one type per grammar rule
//! that carries meaning (`ECL.g4`; the rules that only spell a keyword or a
//! character class are folded into their parents).

/// A SNOMED CT identifier as written in an expression (`sctid`: 6 to 18
/// digits, the first not zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sctid(pub u64);

/// `eclconceptreference`: an identifier with its optional term.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConceptReference {
    /// The identifier.
    pub id: Sctid,
    /// The term between pipes, trimmed.
    pub term: Option<String>,
}

/// `altidentifier`: a code from another scheme (`LOINC#54486-6`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AltIdentifier {
    /// The scheme alias before the `#`.
    pub scheme: String,
    /// The code after the `#`.
    pub code: String,
    /// The term between pipes, trimmed.
    pub term: Option<String>,
}

/// `eclfocusconcept`, or a parenthesized nested constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FocusConcept {
    /// A concept reference.
    Reference(ConceptReference),
    /// `*`, any concept.
    Wildcard,
    /// An alternate identifier.
    AltIdentifier(AltIdentifier),
    /// `( expressionconstraint )`.
    Nested(Box<ExpressionConstraint>),
}

/// `constraintoperator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstraintOperator {
    /// `<`
    DescendantOf,
    /// `<<`
    DescendantOrSelfOf,
    /// `<!`
    ChildOf,
    /// `<<!`
    ChildOrSelfOf,
    /// `>`
    AncestorOf,
    /// `>>`
    AncestorOrSelfOf,
    /// `>!`
    ParentOf,
    /// `>>!`
    ParentOrSelfOf,
    /// `!!>`
    Top,
    /// `!!<`
    Bottom,
}

/// The reference set fields a `memberof` names (`^ [field, field]`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RefsetFields {
    /// `[*]`.
    Any,
    /// `[name, name]`.
    Names(Vec<String>),
}

/// `memberof`: `^`, with its optional field selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberOf {
    /// The fields between brackets, when given.
    pub fields: Option<RefsetFields>,
}

/// `subexpressionconstraint`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubExpressionConstraint {
    /// The constraint operator before the focus.
    pub operator: Option<ConstraintOperator>,
    /// The `^` before the focus.
    pub member_of: Option<MemberOf>,
    /// The focus concept or nested constraint.
    pub focus: FocusConcept,
    /// `memberfilterconstraint`s, one list per `{{ M ... }}`.
    pub member_filters: Vec<Vec<MemberFilter>>,
    /// The description and concept filter constraints, in order.
    pub filters: Vec<FilterConstraint>,
    /// `historysupplement`.
    pub history: Option<HistorySupplement>,
}

/// `descriptionfilterconstraint` or `conceptfilterconstraint`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FilterConstraint {
    /// `{{ D ... }}`.
    Description(Vec<DescriptionFilter>),
    /// `{{ C ... }}`.
    Concept(Vec<ConceptFilter>),
}

/// `expressionconstraint`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExpressionConstraint {
    /// `refinedexpressionconstraint`: a focus with `:` and a refinement.
    Refined {
        /// The constraint refined.
        focus: SubExpressionConstraint,
        /// The refinement.
        refinement: Box<Refinement>,
    },
    /// `conjunctionexpressionconstraint`, two or more operands.
    Conjunction(Vec<SubExpressionConstraint>),
    /// `disjunctionexpressionconstraint`, two or more operands.
    Disjunction(Vec<SubExpressionConstraint>),
    /// `exclusionexpressionconstraint`.
    Exclusion {
        /// The set kept.
        left: SubExpressionConstraint,
        /// The set removed.
        right: SubExpressionConstraint,
    },
    /// `dottedexpressionconstraint`: a focus followed by `. attribute` steps.
    Dotted {
        /// The constraint the walk starts from.
        focus: SubExpressionConstraint,
        /// The attribute names walked, in order.
        attributes: Vec<SubExpressionConstraint>,
    },
    /// A bare `subexpressionconstraint`.
    Sub(SubExpressionConstraint),
}

/// `eclrefinement`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Refinement {
    /// One sub-refinement.
    Single(Box<SubRefinement>),
    /// Sub-refinements joined by `AND` or `,`, two or more.
    Conjunction(Vec<SubRefinement>),
    /// Sub-refinements joined by `OR`, two or more.
    Disjunction(Vec<SubRefinement>),
}

/// `subrefinement`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubRefinement {
    /// An attribute set.
    AttributeSet(AttributeSet),
    /// `eclattributegroup`: `[c] { attributes }`.
    Group {
        /// The group cardinality.
        cardinality: Option<Cardinality>,
        /// The attributes in the group.
        attributes: AttributeSet,
    },
    /// `( eclrefinement )`.
    Nested(Box<Refinement>),
}

/// `eclattributeset`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeSet {
    /// One sub-attribute set.
    Single(Box<SubAttributeSet>),
    /// Joined by `AND` or `,`, two or more.
    Conjunction(Vec<SubAttributeSet>),
    /// Joined by `OR`, two or more.
    Disjunction(Vec<SubAttributeSet>),
}

/// `subattributeset`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubAttributeSet {
    /// An attribute.
    Attribute(Box<Attribute>),
    /// `( eclattributeset )`.
    Nested(Box<AttributeSet>),
}

/// `eclattribute`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attribute {
    /// `[min..max]` before the name.
    pub cardinality: Option<Cardinality>,
    /// The reverse flag `R`.
    pub reverse: bool,
    /// `eclattributename`, a sub-expression constraint.
    pub name: SubExpressionConstraint,
    /// The comparison and value.
    pub value: AttributeValue,
}

/// The comparison and value of an attribute or a member field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeValue {
    /// `= sub` or `!= sub`.
    Expression {
        /// The operator.
        operator: Equality,
        /// The value constraint.
        value: SubExpressionConstraint,
    },
    /// `op #number`.
    Numeric {
        /// The operator.
        operator: Comparison,
        /// The number.
        value: NumericValue,
    },
    /// `= "string"`, `= wild:"..."`, or a set of them.
    String {
        /// The operator.
        operator: Equality,
        /// The search terms; one prints bare, more print as a set.
        terms: Vec<TypedSearchTerm>,
    },
    /// `= true` or `= false`.
    Boolean {
        /// The operator.
        operator: Equality,
        /// The value.
        value: bool,
    },
}

/// `cardinality`: `min..max`, `max` absent for `*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cardinality {
    /// The minimum.
    pub min: u32,
    /// The maximum, `None` for `*`.
    pub max: Option<u32>,
}

/// `expressioncomparisonoperator`, `stringcomparisonoperator`,
/// `booleancomparisonoperator`, and `idcomparisonoperator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Equality {
    /// `=`
    Equal,
    /// `!=`
    NotEqual,
}

/// `numericcomparisonoperator` and `timecomparisonoperator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Comparison {
    /// `=`
    Equal,
    /// `!=`
    NotEqual,
    /// `<`
    Less,
    /// `<=`
    LessOrEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterOrEqual,
}

/// `numericvalue`: the literal after `#`, as written (an optional sign,
/// digits, an optional fraction).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NumericValue(pub String);

/// `typedsearchterm`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypedSearchTerm {
    /// `"word word"` or `match:"..."`: every word must match as a prefix.
    Match(Vec<String>),
    /// `wild:"..."`: a pattern with `*` wildcards.
    Wild(String),
}

/// `descriptionfilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DescriptionFilter {
    /// `term op terms`.
    Term {
        /// The operator.
        operator: Equality,
        /// The search terms.
        terms: Vec<TypedSearchTerm>,
    },
    /// `language op codes`.
    Language {
        /// The operator.
        operator: Equality,
        /// Two-letter language codes.
        codes: Vec<String>,
    },
    /// `typeId op concepts`.
    TypeId {
        /// The operator.
        operator: Equality,
        /// The description type concepts.
        value: ConceptSet,
    },
    /// `type op tokens`.
    Type {
        /// The operator.
        operator: Equality,
        /// The description type tokens.
        tokens: Vec<TypeToken>,
    },
    /// `dialectId op value [acceptability]`.
    DialectId {
        /// The operator.
        operator: Equality,
        /// The language reference sets.
        value: DialectIdValue,
        /// The acceptability that applies to every dialect named.
        acceptability: Option<AcceptabilitySet>,
    },
    /// `dialect op aliases [acceptability]`.
    Dialect {
        /// The operator.
        operator: Equality,
        /// The dialect aliases with their own acceptability.
        aliases: Vec<DialectAlias>,
        /// The acceptability that applies to every dialect named.
        acceptability: Option<AcceptabilitySet>,
    },
    /// `moduleId op concepts`.
    Module {
        /// The operator.
        operator: Equality,
        /// The modules.
        value: ConceptSet,
    },
    /// `effectiveTime op times`.
    EffectiveTime {
        /// The operator.
        operator: Comparison,
        /// The times.
        values: Vec<TimeValue>,
    },
    /// `active op value`.
    Active {
        /// The operator.
        operator: Equality,
        /// The value.
        value: bool,
    },
    /// `id op ids`.
    Id {
        /// The operator.
        operator: Equality,
        /// The description identifiers.
        ids: Vec<Sctid>,
    },
}

/// A sub-expression constraint or a set of two or more concept references
/// (`subexpressionconstraint | eclconceptreferenceset`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConceptSet {
    /// A constraint.
    Expression(SubExpressionConstraint),
    /// `( ref ref ... )`, two or more.
    Set(Vec<ConceptReference>),
}

/// `typetoken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeToken {
    /// `syn`
    Synonym,
    /// `fsn`
    FullySpecifiedName,
    /// `def`
    Definition,
}

/// The value of a `dialectidfilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DialectIdValue {
    /// A constraint.
    Expression(SubExpressionConstraint),
    /// `( ref [acceptability] ... )`, one or more.
    Set(Vec<(ConceptReference, Option<AcceptabilitySet>)>),
}

/// One alias of a `dialectaliasfilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DialectAlias {
    /// The alias (`en-gb`).
    pub alias: String,
    /// The acceptability for this alias alone.
    pub acceptability: Option<AcceptabilitySet>,
}

/// `acceptabilityset`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AcceptabilitySet {
    /// Acceptability concepts, one or more.
    Concepts(Vec<ConceptReference>),
    /// `accept` and `prefer` tokens, one or more.
    Tokens(Vec<Acceptability>),
}

/// `acceptabilitytoken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Acceptability {
    /// `accept`
    Acceptable,
    /// `prefer`
    Preferred,
}

/// `timevalue`: `YYYYMMDD` or empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeValue(pub String);

/// `conceptfilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConceptFilter {
    /// `definitionStatusId op concepts`.
    DefinitionStatusId {
        /// The operator.
        operator: Equality,
        /// The definition status concepts.
        value: ConceptSet,
    },
    /// `definitionStatus op tokens`.
    DefinitionStatus {
        /// The operator.
        operator: Equality,
        /// The tokens.
        tokens: Vec<DefinitionStatus>,
    },
    /// `moduleId op concepts`.
    Module {
        /// The operator.
        operator: Equality,
        /// The modules.
        value: ConceptSet,
    },
    /// `effectiveTime op times`.
    EffectiveTime {
        /// The operator.
        operator: Comparison,
        /// The times.
        values: Vec<TimeValue>,
    },
    /// `active op value`.
    Active {
        /// The operator.
        operator: Equality,
        /// The value.
        value: bool,
    },
}

/// `definitionstatustoken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefinitionStatus {
    /// `primitive`
    Primitive,
    /// `defined`
    Defined,
}

/// `memberfilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemberFilter {
    /// `moduleId op concepts`.
    Module {
        /// The operator.
        operator: Equality,
        /// The modules.
        value: ConceptSet,
    },
    /// `effectiveTime op times`.
    EffectiveTime {
        /// The operator.
        operator: Comparison,
        /// The times.
        values: Vec<TimeValue>,
    },
    /// `active op value`.
    Active {
        /// The operator.
        operator: Equality,
        /// The value.
        value: bool,
    },
    /// `memberfieldfilter`: a reference set field compared to a value.
    Field {
        /// The field name.
        name: String,
        /// The comparison.
        value: FieldValue,
    },
}

/// The comparison of a `memberfieldfilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldValue {
    /// `= sub` or `!= sub`.
    Expression {
        /// The operator.
        operator: Equality,
        /// The value constraint.
        value: SubExpressionConstraint,
    },
    /// `op #number`.
    Numeric {
        /// The operator.
        operator: Comparison,
        /// The number.
        value: NumericValue,
    },
    /// `= "string"` or a set.
    String {
        /// The operator.
        operator: Equality,
        /// The search terms.
        terms: Vec<TypedSearchTerm>,
    },
    /// `= true` or `= false`.
    Boolean {
        /// The operator.
        operator: Equality,
        /// The value.
        value: bool,
    },
    /// `< "time"` and the other ordering operators (`=` and `!=` with a
    /// quoted value are string comparisons, the grammar's first match).
    Time {
        /// The operator: `Less`, `LessOrEqual`, `Greater`, or `GreaterOrEqual`.
        operator: Comparison,
        /// The times.
        values: Vec<TimeValue>,
    },
}

/// `historysupplement`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HistorySupplement {
    /// `{{ + HISTORY }}`, the default profile.
    Default,
    /// `{{ + HISTORY-MIN }}`.
    Minimum,
    /// `{{ + HISTORY-MOD }}`.
    Moderate,
    /// `{{ + HISTORY-MAX }}`.
    Maximum,
    /// `{{ + HISTORY ( constraint ) }}`: the association reference sets.
    Subset(Box<ExpressionConstraint>),
}
