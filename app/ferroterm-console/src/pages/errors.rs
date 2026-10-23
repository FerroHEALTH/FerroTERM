// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error pages (R20): an address that is no screen, a write the
//! reader's account may not make, and a session store the console cannot
//! read.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::error_page::{ErrorPage, Status, Way, error_page};
use leptos::prelude::*;
use leptos_meta::Title;

use crate::locale::{Msg, tr, tr_with};
use crate::request::request;
use crate::routes::{VALUE_SETS_PATH, ui_path};

/// Sets the status of the answer the page is drawn into.
fn status(code: http::StatusCode) {
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(code);
    }
}

/// The page for an address that names no screen: `404`, the address, and
/// the ways on.
pub(crate) fn not_found_page() -> AnyView {
    status(http::StatusCode::NOT_FOUND);
    let path = request().path;
    let body = view! { {tr_with(Msg::ErrorsNotFoundBody, &[("path", path.into())])} }.into_any();
    view! {
        <Title text=tr(Msg::ErrorsNotFoundTitle) />
        {error_page(ErrorPage {
            status: Status::NotFound,
            title: tr(Msg::ErrorsNotFoundTitle),
            body,
            hint: None,
            ways: vec![
                Way {
                    label: tr(Msg::ShellDashboard),
                    href: ui_path(""),
                    glyph: Some(Glyph::LAYOUT_DASHBOARD),
                },
                Way {
                    label: tr(Msg::ShellValueSets),
                    href: ui_path(VALUE_SETS_PATH),
                    glyph: Some(Glyph::LAYERS),
                },
            ],
            detail: None,
        })}
    }
    .into_any()
}

/// The page for a write the reader's account may not make: who they are,
/// what still works, and the scope it needs as a quiet hint.
pub(crate) fn forbidden_page(what: Msg, scope: &str, back: (String, String)) -> AnyView {
    status(http::StatusCode::FORBIDDEN);
    let request = request();
    let reader = request
        .session
        .who()
        .unwrap_or_else(|| tr(Msg::ShellNotSignedIn));
    let title = tr(what);
    let body = tr_with(
        Msg::ErrorsForbiddenBody,
        &[("who", reader.into()), ("what", title.clone().into())],
    )
    .into_any();
    let hint = view! {
        {tr(Msg::ErrorsForbiddenScope)}
        " "
        <code>{scope.to_owned()}</code>
    }
    .into_any();
    let (label, href) = back;
    view! {
        <Title text=title.clone() />
        {error_page(ErrorPage {
            status: Status::Forbidden,
            title,
            body,
            hint: Some(hint),
            ways: vec![
                Way {
                    label,
                    href,
                    glyph: Some(Glyph::ARROW_LEFT),
                },
                Way {
                    label: tr(Msg::ErrorsSignInAgain),
                    href: crate::routes::sign_in_link(&request.back()),
                    glyph: Some(Glyph::LOG_IN),
                },
            ],
            detail: None,
        })}
    }
    .into_any()
}
