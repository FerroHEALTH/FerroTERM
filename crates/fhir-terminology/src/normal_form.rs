// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Necessary Normal Form of a concept, rendered in SNOMED CT
//! Compositional Grammar.
//!
//! The FHIR SNOMED CT page defines `normalForm` and `normalFormTerse` as the
//! "Generated Necessary Normal Form expression for the provided code or
//! expression", with terms and with concept ids only
//! (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties"). It
//! defines the property and not the generation.
//!
//! The generation is a rendering and not a computation. The necessary normal
//! form of a precoordinated concept is what the release's inferred view
//! already holds: "The inferred view is the necessary normal form of the
//! concept definitions, following classification", and it is the relationship
//! file that carries it (the SNOMED CT Release File Specification, Appendix D,
//! <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/appendices/appendix-d-concept-definition-illustrations.md>).
//! So the focus concepts are the concept's inferred `is a` parents and the
//! attributes are its other inferred rows, both of which the built index holds
//! as point reads. No reasoner and no supertype walk runs here.
//!
//! The syntax is the SNOMED CT Compositional Grammar specification, §5 Syntax
//! Specification
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-compositional-grammar-specification/design/5-syntax-specification>).
//!
//! Three shapes are ours to choose, because no specification fixes them, and
//! each is marked where it is decided: the order of what is rendered, which
//! term a concept reference carries, and the layout.

/// The definition status prefix of a concept that is sufficiently defined.
///
/// "When the combination of focus concepts and attribute refinements are both
/// necessary and sufficient to define the clinical meaning being represented,
/// the 'equivalent to' definition status is used" (the Compositional Grammar
/// specification, §6.7 Expressions With a Definition Status).
const EQUIVALENT_TO: &str = "===";

/// The definition status prefix of a primitive concept.
///
/// The same section: a definition that is "necessary but not necessarily
/// sufficient" carries the 'subtype of' status. The prefix is optional in the
/// grammar and `===` is assumed when it is absent, so a primitive concept
/// rendered without one would assert an equivalence the terminology denies.
/// Both are therefore always written.
const SUBTYPE_OF: &str = "<<<";

/// One concept reference: the identifier, and the term when one is rendered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference {
    /// The concept identifier.
    pub code: String,
    /// The term, absent in the terse form and for a term that cannot be
    /// written.
    pub term: Option<String>,
}

/// What an attribute is equal to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Another concept.
    Concept(Reference),
    /// A concrete number, in its lexical form and without the `#` the release
    /// spells it with.
    Number(String),
    /// A concrete string, without the quotes the release spells it with.
    Text(String),
    /// A nested expression, which only an authored expression carries: the
    /// release's relationship file has no shape for one.
    ///
    /// Boxed, because an enum costs its widest variant for every value and
    /// this is the rare and by far the widest one.
    Nested(Box<Expression>),
}

/// One attribute of a definition, with the role group it belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribute {
    /// The role group; `0` is ungrouped.
    pub group: u32,
    /// The attribute type.
    pub kind: Reference,
    /// The value.
    pub value: Value,
}

/// One concept's definition, as the normal form renders it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    /// Whether the concept is sufficiently defined by these conditions.
    pub defined: bool,
    /// The inferred `is a` parents.
    pub focus: Vec<Reference>,
    /// Every other inferred relationship.
    pub attributes: Vec<Attribute>,
}

impl Reference {
    /// The reference as the grammar writes it.
    ///
    /// `conceptReference = conceptId [ws "|" ws term ws "|"]` (§5), so a
    /// reference with no term is the identifier alone.
    fn render(&self) -> String {
        match &self.term {
            Some(term) => format!("{} |{term}|", self.code),
            None => self.code.clone(),
        }
    }
}

impl Value {
    /// The value as the grammar writes it.
    ///
    /// A concrete value arrives in its lexical form, because the RF2 reader
    /// strips the `#` of a number and the quotes of a string and keeps the
    /// kind (the Release File Specification §4.2.6). The grammar spells both
    /// the way the release does, so the spelling is put back here, with the
    /// backslash and the quote escaped inside a string (the Compositional
    /// Grammar specification §5).
    fn render(&self) -> String {
        match self {
            Self::Concept(reference) => reference.render(),
            Self::Number(number) => format!("#{number}"),
            Self::Text(text) => {
                format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
            }
            // NOTE: `expressionValue = … / "(" ws subExpression ws ")"` (§5),
            // and a `subExpression` carries no definition status.
            Self::Nested(expression) => format!("({})", expression.body()),
        }
    }

    /// What this value sorts by, beside the attribute type.
    fn sort_key(&self) -> (u8, String) {
        match self {
            Self::Concept(reference) => (0, numeric(&reference.code)),
            Self::Number(number) => (1, number.clone()),
            Self::Text(text) => (2, text.clone()),
            Self::Nested(expression) => (3, expression.body()),
        }
    }
}

impl Expression {
    /// The expression in Compositional Grammar, or `None` for a concept with
    /// no definition to render.
    ///
    /// `focusConcept` is mandatory in the grammar (§5), so a concept with no
    /// inferred parent has no expression. That is the root of the hierarchy
    /// and nothing else, because every other concept is under it; no
    /// specification says what the root's normal form is, and inventing a
    /// focus concept for it would say something the release does not.
    #[must_use]
    pub fn render(&self) -> Option<String> {
        if self.focus.is_empty() {
            return None;
        }
        let prefix = if self.defined {
            EQUIVALENT_TO
        } else {
            SUBTYPE_OF
        };
        Some(format!("{prefix} {}", self.body()))
    }

    /// The `subExpression` alone: the focus concepts and the refinement,
    /// without the definition status a nested expression has no place for
    /// (§5).
    #[must_use]
    pub fn body(&self) -> String {
        let mut focus: Vec<&Reference> = self.focus.iter().collect();
        focus.sort_by_key(|reference| numeric(&reference.code));
        let focus: Vec<String> = focus.iter().map(|reference| reference.render()).collect();
        let body = focus.join(" + ");
        let refinement = self.refinement();
        if refinement.is_empty() {
            return body;
        }
        format!("{body} : {refinement}")
    }

    /// The refinement: the ungrouped attributes, then each role group.
    ///
    /// The comma between two attributes of one set is mandatory and the one
    /// between two adjacent groups is optional (§6.4 Expressions With
    /// Attribute Groups); both are written, because a separator that is always
    /// there is one fewer thing for a reader to parse. The braces are written
    /// even around a lone group, which §6.4 also makes optional, so that a
    /// grouped attribute never reads as an ungrouped one.
    fn refinement(&self) -> String {
        let mut ungrouped: Vec<&Attribute> = self
            .attributes
            .iter()
            .filter(|attribute| attribute.group == 0)
            .collect();
        ungrouped.sort_by_key(|attribute| order(attribute));

        let mut groups: Vec<u32> = self
            .attributes
            .iter()
            .map(|attribute| attribute.group)
            .filter(|group| *group != 0)
            .collect();
        groups.sort_unstable();
        groups.dedup();

        let mut parts: Vec<String> = Vec::new();
        if !ungrouped.is_empty() {
            parts.push(rendered(&ungrouped).join(", "));
        }
        // NOTE: "there is no guarantee that they will be assigned
        // sequentially" (the Release File Specification §4.2.3), so groups sort
        // by what they render to and not by their number.
        let mut written: Vec<String> = groups
            .into_iter()
            .map(|group| {
                let mut members: Vec<&Attribute> = self
                    .attributes
                    .iter()
                    .filter(|attribute| attribute.group == group)
                    .collect();
                members.sort_by_key(|attribute| order(attribute));
                format!("{{ {} }}", rendered(&members).join(", "))
            })
            .collect();
        written.sort();
        parts.extend(written);
        parts.join(", ")
    }
}

/// Each attribute as the grammar writes it: `attributeName ws "=" ws value`.
fn rendered(attributes: &[&Attribute]) -> Vec<String> {
    attributes
        .iter()
        .map(|attribute| format!("{} = {}", attribute.kind.render(), attribute.value.render()))
        .collect()
}

/// What one attribute sorts by.
///
/// No specification fixes the order of attributes, of groups, or of focus
/// concepts. The glossary says only that a canonical form is produced by
/// "arranging the focus concepts, refinements, attributes, and attribute
/// groups in a standard order" and never states the order
/// (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-glossary>,
/// "canonical form"), so this is our own design. It follows the only published
/// canonical examples, which sort ascending by the attribute's concept id.
fn order(attribute: &Attribute) -> (String, (u8, String)) {
    (numeric(&attribute.kind.code), attribute.value.sort_key())
}

/// A concept identifier as a key that sorts the way the number does.
///
/// An SCTID is 6 to 18 digits with a non-zero leading digit (the Compositional
/// Grammar specification, §5), so length before content is numeric order and
/// needs no parse that could fail on an identifier this server did not mint.
fn numeric(code: &str) -> String {
    format!("{:02}{code}", code.len().min(99))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reference carrying a term.
    fn named(code: &str, term: &str) -> Reference {
        Reference {
            code: code.to_owned(),
            term: Some(term.to_owned()),
        }
    }

    /// A reference carrying none, as the terse form renders it.
    fn bare(code: &str) -> Reference {
        Reference {
            code: code.to_owned(),
            term: None,
        }
    }

    /// One attribute between two concepts.
    fn attribute(group: u32, kind: &str, value: &str) -> Attribute {
        Attribute {
            group,
            kind: bare(kind),
            value: Value::Concept(bare(value)),
        }
    }

    // NOTE: the shape is Appendix D’s illustration of a classified concept,
    // several focus concepts and one role group, with this project’s own
    // identifiers because a fixture ships no SNOMED CT content.
    #[test]
    fn a_defined_concept_renders_its_focus_concepts_and_its_group() {
        let expression = Expression {
            defined: true,
            focus: vec![bare("100000001"), bare("100000002")],
            attributes: vec![attribute(1, "200000001", "300000001")],
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("=== 100000001 + 100000002 : { 200000001 = 300000001 }")
        );
    }

    #[test]
    fn a_primitive_concept_carries_the_subtype_prefix() {
        let expression = Expression {
            defined: false,
            focus: vec![bare("100000001")],
            attributes: Vec::new(),
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("<<< 100000001"),
            "a primitive rendered without the prefix would assert an equivalence the \
             terminology denies, because the grammar assumes `===` when none is written"
        );
    }

    #[test]
    fn an_ungrouped_attribute_precedes_the_groups() {
        let expression = Expression {
            defined: false,
            focus: vec![bare("100000001")],
            attributes: vec![
                attribute(2, "200000002", "300000002"),
                attribute(0, "200000001", "300000001"),
            ],
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("<<< 100000001 : 200000001 = 300000001, { 200000002 = 300000002 }")
        );
    }

    #[test]
    fn the_attributes_of_one_group_are_ordered_by_their_type() {
        let expression = Expression {
            defined: true,
            focus: vec![bare("100000001")],
            attributes: vec![
                attribute(1, "200000009", "300000001"),
                attribute(1, "200000001", "300000002"),
            ],
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("=== 100000001 : { 200000001 = 300000002, 200000009 = 300000001 }")
        );
    }

    #[test]
    fn the_order_does_not_follow_the_group_numbers_the_release_gave() {
        let ordered = |groups: [u32; 2]| {
            Expression {
                defined: true,
                focus: vec![bare("100000001")],
                attributes: vec![
                    attribute(groups[0], "200000002", "300000002"),
                    attribute(groups[1], "200000001", "300000001"),
                ],
            }
            .render()
        };
        assert_eq!(
            ordered([1, 2]),
            ordered([7, 3]),
            "a group number is co-membership and is not assigned sequentially, so the \
             rendering must not depend on it"
        );
    }

    #[test]
    fn a_longer_identifier_sorts_after_a_shorter_one() {
        let expression = Expression {
            defined: true,
            focus: vec![bare("1000000000000001"), bare("999999999")],
            attributes: Vec::new(),
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("=== 999999999 + 1000000000000001"),
            "an identifier is a number, so 16 digits sort after 9 rather than before"
        );
    }

    #[test]
    fn the_terms_are_written_where_the_form_carries_them() {
        let expression = Expression {
            defined: true,
            focus: vec![named("100000001", "A shaped procedure")],
            attributes: vec![Attribute {
                group: 1,
                kind: named("200000001", "Method"),
                value: Value::Concept(named("300000001", "An action")),
            }],
        };
        assert_eq!(
            expression.render().as_deref(),
            Some(
                "=== 100000001 |A shaped procedure| : \
                 { 200000001 |Method| = 300000001 |An action| }"
            )
        );
    }

    #[test]
    fn a_concrete_value_is_written_the_way_the_release_spells_it() {
        let expression = Expression {
            defined: true,
            focus: vec![bare("100000001")],
            attributes: vec![
                Attribute {
                    group: 1,
                    kind: bare("200000001"),
                    value: Value::Number(String::from("500")),
                },
                Attribute {
                    group: 1,
                    kind: bare("200000002"),
                    value: Value::Text(String::from("a shaped \"label\"")),
                },
            ],
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("=== 100000001 : { 200000001 = #500, 200000002 = \"a shaped \\\"label\\\"\" }"),
            "the reader strips the release's spelling and the grammar wants it back"
        );
    }

    #[test]
    fn a_concept_with_no_inferred_parent_has_no_expression() {
        let expression = Expression {
            defined: false,
            focus: Vec::new(),
            attributes: Vec::new(),
        };
        assert_eq!(
            expression.render(),
            None,
            "the grammar makes a focus concept mandatory, so the root of the hierarchy \
             has nothing to render"
        );
    }

    #[test]
    fn two_attributes_of_one_type_are_ordered_by_their_values() {
        let expression = Expression {
            defined: true,
            focus: vec![bare("100000001")],
            attributes: vec![
                attribute(0, "200000001", "300000002"),
                attribute(0, "200000001", "300000001"),
            ],
        };
        assert_eq!(
            expression.render().as_deref(),
            Some("=== 100000001 : 200000001 = 300000001, 200000001 = 300000002"),
            "the order is total, so one concept renders one way however the index held it"
        );
    }
}
