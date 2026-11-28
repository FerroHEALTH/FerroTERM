// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The junctions of `eclrefinement`, which ECL 6.4 Conjunction and
//! Disjunction forbids mixing without brackets.

use winnow::error::ErrMode;
use winnow::prelude::*;

use super::sub_refinement;
use crate::ast::{AttributeSet, SubRefinement};
use crate::lexer::Kind;
use sct_scg::front::{Expecting, Failure, Tokens};

/// The two junctions of a refinement.
#[derive(Clone, Copy)]
pub(super) enum Junction {
    /// `AND` or the comma.
    And,
    /// `OR`.
    Or,
}

/// Whether `item` is an unbracketed attribute set joined by `junction`.
pub(super) fn joins(item: &SubRefinement, junction: Junction) -> bool {
    matches!(
        (item, junction),
        (
            SubRefinement::AttributeSet(AttributeSet::Conjunction(_)),
            Junction::And
        ) | (
            SubRefinement::AttributeSet(AttributeSet::Disjunction(_)),
            Junction::Or
        )
    )
}

/// A `subrefinement` joined by `junction`, refused when it mixes in the other.
pub(super) fn operand<'i>(
    junction: Junction,
) -> impl Parser<Tokens<'i, Kind>, SubRefinement, ErrMode<Failure>> {
    let other = match junction {
        Junction::And => Junction::Or,
        Junction::Or => Junction::And,
    };
    sub_refinement
        .verify(move |item: &SubRefinement| !joins(item, other))
        .expecting("round brackets around a mix of AND and OR")
}
