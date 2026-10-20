//! One section's answer, drawn while the read behind it settles.

use leptos::prelude::*;

use ferrohealth_ui::view::empty::{Busy, busy};

/// Draws a section's answer, keeping the last one on screen while it reloads.
///
/// `<Transition>` rather than `<Suspense>`, because every read on a screen
/// whose parameters live in the address refetches, and a suspense boundary
/// flashes its fallback on each reload
/// (<https://github.com/leptos-rs/book/blob/main/src/async/12_transition.md>).
/// The children arrive boxed, so the transition inside is instantiated once
/// for every section that uses this rather than once per section: it is
/// generic over its children, and each use of it monomorphizes the subtree
/// under it.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn Reading(
    /// What the section is waiting on, read out while it waits.
    #[prop(into)]
    label: String,
    /// Draws the indicator at the size of the surrounding text, for a section
    /// that sits inside a line rather than under a heading of its own.
    #[prop(optional)]
    inline: bool,
    /// The answer to draw once the read settles.
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <Transition fallback=move || {
            busy(label, if inline { Busy::Inline } else { Busy::Section })
        }>{move || children()}</Transition>
    }
}
