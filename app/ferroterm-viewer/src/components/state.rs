//! What a screen draws where there is no answer to draw.
//!
//! Three states, three shapes, the same on every screen. A reader who learns
//! one on the overview has learnt it on the runners.
//!
//! Loading is `Reading`, which keeps the last answer on screen while the next
//! one arrives, and a refusal is `Failure`, which renders the server's own
//! `OperationOutcome`. The two below are the states with nothing behind them:
//! an answer that holds nothing, and a thing this root cannot do.

use crate::locale::{Msg, tr};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::notice::{Announce, notice};
use leptos::prelude::*;

/// An answer that holds nothing.
///
/// The same panel a listing draws its rows in, so an empty answer reads as an
/// answer rather than as a screen that failed to draw. The sentence says what
/// was looked for, because "no results" says nothing a reader can act on.
pub(crate) fn empty(sentence: String) -> AnyView {
    empty_quiet(Glyph::INBOX, tr(Msg::NothingHere), sentence).into_any()
}

/// Something this root does not declare, so the screen does not offer it.
///
/// Tinted rather than plain, because it is a fact about the server rather than
/// about the answer, and a reader who came to do the thing needs to see why
/// they cannot. It is never an error: a root that declares less is answering
/// correctly (<https://hl7.org/fhir/R5/capabilitystatement.html>).
pub(crate) fn undeclared(sentence: String) -> AnyView {
    notice(Tone::Info, Announce::Status, None, sentence).into_any()
}

/// What a screen says before it has been given enough to run.
///
/// Not an empty answer: nothing was asked yet. It is quiet prose rather than a
/// panel, because a panel here would draw the eye to the one place on the
/// screen with nothing in it.
pub(crate) fn invitation(sentence: String) -> AnyView {
    view! { <p class="fui-muted">{sentence}</p> }.into_any()
}
