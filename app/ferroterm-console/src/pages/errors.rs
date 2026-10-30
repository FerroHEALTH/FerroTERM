// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error pages (R20): an address that is no screen, and a write the
//! reader's account may not make.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::error_page::{ErrorPage, Status, Way, error_page};

use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, t};
use crate::request::Request;
use crate::routes::{VALUE_SETS_PATH, ui_path};

/// The page for an address that names no screen: `404`, the address, and
/// the ways on.
pub(crate) fn not_found_page(request: &Request) -> Html {
    let locale = &request.locale;
    request.answering.status(http::StatusCode::NOT_FOUND);
    let title = tr(locale, Msg::ErrorsNotFoundTitle);
    request.answering.title(&title);
    let body = t(tr_with(
        locale,
        Msg::ErrorsNotFoundBody,
        &[("path", request.path.clone().into())],
    ));
    error_page(
        locale,
        ErrorPage {
            status: Status::NotFound,
            title,
            body,
            hint: None,
            ways: vec![
                Way {
                    label: tr(locale, Msg::ShellDashboard),
                    href: ui_path(""),
                    glyph: Some(Glyph::LAYOUT_DASHBOARD),
                },
                Way {
                    label: tr(locale, Msg::ShellValueSets),
                    href: ui_path(VALUE_SETS_PATH),
                    glyph: Some(Glyph::LAYERS),
                },
            ],
            detail: None,
        },
    )
}

/// The page for a write the reader's account may not make: who they are,
/// what still works, and the scope it needs as a quiet hint.
pub(crate) fn forbidden_page(
    request: &Request,
    what: Msg,
    scope: &str,
    back: (String, String),
) -> Html {
    let locale = &request.locale;
    request.answering.status(http::StatusCode::FORBIDDEN);
    let reader = request
        .session
        .who()
        .unwrap_or_else(|| tr(locale, Msg::ShellNotSignedIn));
    let title = tr(locale, what);
    request.answering.title(&title);
    let body = t(tr_with(
        locale,
        Msg::ErrorsForbiddenBody,
        &[("who", reader.into()), ("what", title.clone().into())],
    ));
    let hint = h(format!(
        "{} {}",
        t(tr(locale, Msg::ErrorsForbiddenScope)),
        code(scope)
    ));
    let (label, href) = back;
    error_page(
        locale,
        ErrorPage {
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
                    label: tr(locale, Msg::ErrorsSignInAgain),
                    href: crate::routes::sign_in_link(&request.back()),
                    glyph: Some(Glyph::LOG_IN),
                },
            ],
            detail: None,
        },
    )
}
