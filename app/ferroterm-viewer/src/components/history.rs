// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The way from a screen that edits a resource to the versions of it.
//!
//! One offer, drawn the same on every authoring screen, so a terminologist
//! who finds it on one has found it on all three. It appears only for a
//! resource the server already holds, because a resource that was never
//! written has no version to read
//! (<https://hl7.org/fhir/R4B/http.html#history>).

use leptos::prelude::*;

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::routes::history_link;
use crate::styles;

/// The link to one resource's versions, or nothing when it has none yet.
///
/// `id` is the logical id the server assigned, which is what the history and
/// version-read interactions address an instance by.
pub(crate) fn history_offer(
    resource_type: &'static str,
    id: Signal<String>,
    version: Signal<FhirVersion>,
) -> AnyView {
    let address = move || history_link(resource_type, &id.get(), version.get());
    view! {
        <Show when=move || !id.read().trim().is_empty() fallback=|| ()>
            <p>
                <a href=address class=styles::LINK>
                    {tr(Msg::HistoryOfThisResource)}
                </a>
            </p>
        </Show>
    }
    .into_any()
}
