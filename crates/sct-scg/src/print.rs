// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The printer: every tree node writes itself in the grammar it was read
//! from, so the output parses back to the same tree.
//!
//! Where the grammar leaves whitespace free, the printer writes one space
//! around every operator and none inside a delimiter pair. No specification
//! fixes that layout: our own design, chosen so one tree prints one way.

use std::fmt;

use crate::ast::{
    Attribute, AttributeGroup, AttributeValue, ConceptReference, DefinitionStatus, Expression,
    Refinement, SubExpression,
};

impl fmt::Display for DefinitionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.prefix())
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = self.definition_status {
            write!(f, "{status} ")?;
        }
        write!(f, "{}", self.sub_expression)
    }
}

impl fmt::Display for SubExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, concept) in self.focus.iter().enumerate() {
            if index > 0 {
                f.write_str(" + ")?;
            }
            write!(f, "{concept}")?;
        }
        match &self.refinement {
            Some(refinement) => write!(f, " : {refinement}"),
            None => Ok(()),
        }
    }
}

impl fmt::Display for ConceptReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.term {
            Some(term) => write!(f, "{} |{term}|", self.concept_id),
            None => f.write_str(&self.concept_id),
        }
    }
}

impl fmt::Display for Refinement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut written = false;
        for attribute in &self.ungrouped {
            if written {
                f.write_str(", ")?;
            }
            write!(f, "{attribute}")?;
            written = true;
        }
        for group in &self.groups {
            if written {
                f.write_str(", ")?;
            }
            write!(f, "{group}")?;
            written = true;
        }
        Ok(())
    }
}

impl fmt::Display for AttributeGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{ ")?;
        for (index, attribute) in self.attributes.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{attribute}")?;
        }
        f.write_str(" }")
    }
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.name, self.value)
    }
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concept(reference) => write!(f, "{reference}"),
            Self::Nested(sub) => write!(f, "({sub})"),
            // `escapedChar = BS QM / BS BS` is the whole escape set, so those
            // two characters are the two the printer writes back escaped.
            Self::Text(text) => {
                write!(f, "\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
            }
            Self::Number(number) => write!(f, "#{number}"),
        }
    }
}
