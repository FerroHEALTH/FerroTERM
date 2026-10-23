// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The screen an address that names no page reaches.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::shell::SelectedVersion;
use crate::locale::{Msg, tr};
use crate::routes::ui_link;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_state;
use ferrohealth_ui::view::surface::panel;

/// Says that the address names no screen, and offers the way back.
///
/// The fallback is scoped to the bundle's own prefix, so a FHIR path the
/// server does not serve still answers an `OperationOutcome` rather than this.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn NotFoundPage() -> impl IntoView {
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let home = move || {
        view! {
            <a href=ui_link("", version.get()) class=Variant::Primary.class()>
                {tr(Msg::NotFoundBack)}
            </a>
        }
    };
    view! {
        <Title text=tr(Msg::NotFoundTitle) />
        <h1 class="fui-sr-only">{tr(Msg::NotFoundHeading)}</h1>
        {move || {
            panel(
                empty_state(
                    Glyph::SEARCH_X,
                    tr(Msg::NotFoundHeading),
                    tr(Msg::NotFoundBody),
                    home(),
                ),
            )
        }}
    }
}
