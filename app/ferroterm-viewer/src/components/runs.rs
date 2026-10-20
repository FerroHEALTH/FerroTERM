//! The runs a reader has made, as the links that re-run them.
//!
//! Every runner draws this the same way and records into it the same way, so
//! a reader who learns it on one screen has learnt it on all three.

use leptos::prelude::*;

use crate::locale::{Msg, tr};
use crate::runs::Run;
use crate::runs::RunList;
use crate::runs::Runs;
use crate::styles;

/// The list a runner holds, and the recorder that keeps it current.
///
/// The signal is the page's own run list, which every runner shares. A runner
/// creates one of these and hands the `Memo` its own address reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct History(RwSignal<Runs>);

impl History {
    /// A history over the page's run list, which records every run `made`
    /// names.
    ///
    /// The list accumulates: each run is added to what earlier runs left, so
    /// it is no value derived from `made` alone, and the effect is what keeps
    /// it (<https://github.com/leptos-rs/book/blob/main/src/reactivity/working_with_signals.md>).
    pub(crate) fn recording(made: Memo<Option<Run>>) -> Self {
        let RunList(held) = use_context::<RunList>().unwrap_or_default();
        let location = leptos_router::hooks::use_location();
        Effect::new(move |_| {
            // NOTE: no specification governs this: our own design; a run is
            // remembered only while its screen is the one on the address, so a
            // value read during a navigation away records no run of its own.
            let here = location.pathname.get();
            if let Some(run) = made.get()
                && run.address.split('?').next() == Some(here.as_str())
            {
                held.update(|runs| *runs = runs.remembering(&run));
            }
        });
        Self(held)
    }

    /// The list, as the section a runner draws.
    pub(crate) fn view(self) -> AnyView {
        let Self(held) = self;
        view! {
            <Show when=move || !held.with(Runs::is_empty) fallback=|| ()>
                {move || {
                    let rows: Vec<AnyView> = held
                        .with(|held| held.entries().iter().map(row).collect());
                    view! {
                        <details class="mt-loose rounded-md border border-edge text-small">
                            <summary class=format!(
                                "cursor-pointer px-default py-tight font-medium {}",
                                styles::MUTED,
                            )>{tr(Msg::RunsMadePage)}</summary>
                            <div class="border-t border-edge px-default py-tight">
                                <p class=styles::HINT>{tr(Msg::EachOneAddressMadeTheyLive)}</p>
                                <ul class="mt-default grid gap-tight">{rows}</ul>
                            </div>
                        </details>
                    }
                        .into_any()
                }}
            </Show>
        }
        .into_any()
    }
}

/// One remembered run: the link that re-runs it, and what it was about.
fn row(run: &Run) -> AnyView {
    let address = run.address.clone();
    let screen = run.screen.clone();
    let subject = run.subject.clone();
    view! {
        <li>
            <a href=address class="flex flex-wrap items-baseline gap-default">
                <span class=styles::LINK>{screen}</span>
                <span class=styles::CODE_MUTED>{subject}</span>
            </a>
        </li>
    }
    .into_any()
}
