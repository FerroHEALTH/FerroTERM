// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Subsumption over the closure, in the vocabulary FHIR `$subsumes` returns.

use crate::closure::Closure;
use crate::ordinal::Ordinal;

/// The outcome of testing whether A subsumes B
/// (<https://hl7.org/fhir/R4B/codesystem-concept-subsumption-outcome.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A and B are the same concept.
    Equivalent,
    /// A is an ancestor of B.
    Subsumes,
    /// A is a descendant of B.
    SubsumedBy,
    /// Neither is an ancestor of the other.
    NotSubsumed,
}

impl Outcome {
    /// The FHIR code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Equivalent => "equivalent",
            Self::Subsumes => "subsumes",
            Self::SubsumedBy => "subsumed-by",
            Self::NotSubsumed => "not-subsumed",
        }
    }
}

/// Tests whether `a` subsumes `b` under `closure`.
#[must_use]
pub fn subsumes(closure: &Closure, a: Ordinal, b: Ordinal) -> Outcome {
    if a == b {
        Outcome::Equivalent
    } else if closure.is_ancestor(a, b) {
        Outcome::Subsumes
    } else if closure.is_ancestor(b, a) {
        Outcome::SubsumedBy
    } else {
        Outcome::NotSubsumed
    }
}

#[cfg(test)]
mod tests {
    use super::{Outcome, subsumes};
    use crate::closure::Closure;
    use crate::csr::Csr;
    use crate::ordinal::Ordinal;

    #[test]
    fn the_four_outcomes() {
        let o = Ordinal::new;
        let is_a = Csr::build(4, [(o(1), o(0)), (o(2), o(0)), (o(3), o(1))]).expect("builds");
        let closure = Closure::compute(&is_a).expect("acyclic");
        assert_eq!(subsumes(&closure, o(0), o(0)), Outcome::Equivalent);
        assert_eq!(subsumes(&closure, o(0), o(3)), Outcome::Subsumes);
        assert_eq!(subsumes(&closure, o(3), o(0)), Outcome::SubsumedBy);
        assert_eq!(subsumes(&closure, o(2), o(3)), Outcome::NotSubsumed);
        assert_eq!(Outcome::SubsumedBy.code(), "subsumed-by");
    }
}
