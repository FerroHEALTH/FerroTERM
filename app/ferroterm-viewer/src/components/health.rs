// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The health indicator, which asks the server's `GET /health`.

use std::time::Duration;

use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::status::status;
use leptos::prelude::*;

use crate::components::icon;
use crate::components::reading::Reading;
use crate::fhir::FhirClient;
use crate::locale::{Msg, tr};

/// How often the indicator asks the server whether it is still up.
const POLL: Duration = Duration::from_secs(15);

/// Shows whether the terminology server the console reads is answering.
///
/// The server asks once while it renders the page, and the browser asks
/// again on every tick; the resource refetches, so it is read under a
/// transition boundary, as a `<Suspense>` would flash its fallback on each
/// poll (<https://github.com/leptos-rs/book/blob/main/src/async/12_transition.md>).
pub(crate) fn health_indicator() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let health = Resource::new(
        || (),
        move |()| {
            let client = client.clone();
            async move { client.health().await.map(|status| status.as_u16()) }
        },
    );
    #[cfg(target_arch = "wasm32")]
    match set_interval_with_handle(move || health.refetch(), POLL) {
        Ok(handle) => on_cleanup(move || handle.clear()),
        Err(_refused) => {
            leptos::logging::warn!("this browser refused an interval; health is read once");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: Duration = POLL;

    // NOTE: no specification governs this: our own design; below 40rem the
    // bar holds the version and the theme on one row, so the status yields.
    view! {
        <span class="fui-roomy-only">
            <Reading inline=true label=tr(Msg::CheckingTheServer)>
                {move || {
                    health
                        .get()
                        .map(|answered| match answered {
                            Ok(_code) => {
                                status(Tone::Ok, icon::SERVING, &tr(Msg::Serving), None).into_any()
                            }
                            Err(error) => {
                                let detail = error
                                    .status()
                                    .map(|status| status.as_u16().to_string());
                                status(Tone::Bad, icon::UNREACHABLE, &tr(Msg::Unreachable), detail)
                                    .into_any()
                            }
                        })
                }}
            </Reading>
        </span>
    }
}
