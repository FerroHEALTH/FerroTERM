// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document around every page, and the answer it goes out in.
//!
//! Every page is one document the server draws whole (FerroUI
//! `docs/consoles.md`, "How a console builds"): a handler draws the screen
//! the address names (`crate::screen`), puts it in the shell
//! (`crate::shell`) unless it stands on its own, and `answer` writes the
//! document around it in the reader's theme, density, motion and shortcut
//! hints, read from their cookies so the page arrives drawn in them, and the
//! locale as `lang`. The stylesheet, the FerroTERM theme and the one loader
//! script are FerroUI's, served by its router under this origin, so
//! `script-src 'self'` and `style-src 'self'` hold. No specification governs
//! this: our own design.

use axum::response::{IntoResponse, Response};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::serve::Assets;
use ferrohealth_ui::style::Brand;
use ferrohealth_ui::view::shell::{Document, document};
use http::header::{CACHE_CONTROL, LOCATION};
use http::{HeaderValue, StatusCode};

use crate::locale::Msg;
use crate::markup::{h, t};
use crate::request::Request;
use crate::shell::PRODUCT;

/// The brand whose theme the console draws in: FerroTERM teal.
pub(crate) const BRAND: Brand = Brand::Term;

/// The browser's icon.
pub(crate) const FAVICON: &str = "/favicon.svg";

/// The document title of a page titled `text`: the page, then the product.
#[must_use]
pub(crate) fn title(text: Option<&str>) -> String {
    match text {
        Some(text) if !text.is_empty() => format!("{text} · {PRODUCT}"),
        _ => PRODUCT.to_owned(),
    }
}

/// The whole document of the page `body` drew for `request`.
#[must_use]
pub(crate) fn page_document(request: &Request, body: Html) -> Html {
    let said = request.answering.said();
    let description = request.tr(Msg::Description);
    let head = h(format!(
        r#"<meta name="color-scheme" content="light dark"><meta name="description" content="{}"><link rel="icon" href="{FAVICON}" type="image/svg+xml">"#,
        t(description)
    ));
    document(Document {
        locale: request.locale.clone(),
        preferences: request.prefs.family.clone(),
        title: title(said.title.as_deref()),
        assets: Assets::of(BRAND),
        nonce: None,
        head,
        body,
    })
}

/// The answer for the page `body` drew for `request`: the document with
/// the status the screen said, or the redirect it asked for in its place.
#[must_use]
pub(crate) fn answer(request: &Request, body: Html) -> Response {
    let said = request.answering.said();
    if let Some(to) = said.redirect {
        return see_other(&to);
    }
    let status = said.status.unwrap_or(StatusCode::OK);
    let html = page_document(request, body);
    (status, axum::response::Html(String::from(html))).into_response()
}

/// A `303 See Other` to `to`, one of the console's own addresses, never
/// kept by a cache.
#[must_use]
pub(crate) fn see_other(to: &str) -> Response {
    let Ok(location) = HeaderValue::from_str(to) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(LOCATION, location);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::title;

    #[test]
    fn the_title_names_the_page_then_the_product() {
        assert_eq!(
            "Code systems · FerroTERM Console",
            title(Some("Code systems"))
        );
        assert_eq!("FerroTERM Console", title(None));
        assert_eq!("FerroTERM Console", title(Some("")));
    }
}
