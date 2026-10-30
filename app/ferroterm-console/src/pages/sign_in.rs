// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sign-in page (R19): Sign in to edit at the identity provider, Read
//! without signing in, and the notice of why the page is shown.
//!
//! Reading is open, so the page always offers the way to read without
//! signing in. The sign-in itself is the authorization code flow with PKCE
//! at the configured OpenID Provider, which the console's server runs
//! (`crate::oidc`); the reader returns to the page they came from.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::session::session_ended;
use ferrohealth_ui::view::shell::{brand_mark, centred, lockup_plain};
use ferrohealth_ui::view::sign_in::{Notice, Provider, Retry, SignIn, State, sign_in};

use crate::auth::SignInState;
use crate::locale::{Msg, tr};
use crate::markup::{h, link, t};
use crate::request::Request;
use crate::routes::{BACK_PARAM, LOGIN_ROUTE, ui_path};
use crate::shell::{MARK_DARK, MARK_LIGHT, footer};

/// Where the book documents the console.
const DOCS: &str = "https://ferroterm.eu/docs/operate/console.html";

/// Where the maker publishes its privacy notice.
const PRIVACY: &str = "https://www.cadasto.com/privacy/";

/// The provider's host, as the button names it.
fn provider_host(issuer: &str) -> String {
    url::Url::parse(issuer)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| issuer.to_owned())
}

/// The library's state of the page for `state`, with `retry` the address
/// that tries again.
fn state_of(locale: &Locale, state: Option<SignInState>, retry: &str) -> State {
    let notice = |msg: Msg| Notice::new(tr(locale, msg));
    let again = |msg: Msg| Notice {
        retry: Some(Retry {
            label: tr(locale, Msg::SignInTryAgain),
            href: retry.to_owned(),
        }),
        ..Notice::new(tr(locale, msg))
    };
    match state {
        None => State::Ready,
        Some(SignInState::SignedOut) => State::SignedOut(notice(Msg::SignInSignedOut)),
        Some(SignInState::Expired) => State::Expired(notice(Msg::SignInExpired)),
        Some(SignInState::ExpiredAbsolute) => State::Expired(notice(Msg::SignInExpiredAbsolute)),
        Some(SignInState::Full) => State::Full(notice(Msg::SignInFull)),
        Some(SignInState::Refused) => State::Refused(again(Msg::SignInRefused)),
        Some(SignInState::Rejected) => State::Rejected(notice(Msg::SignInRejected)),
        Some(SignInState::Unreachable) => State::ProviderUnreachable(again(Msg::SignInUnreachable)),
        Some(SignInState::Unavailable) => State::Unavailable(Notice {
            detail: Some(tr(locale, Msg::SignInUnavailableDetail)),
            ..Notice::new(tr(locale, Msg::SignInUnavailable))
        }),
    }
}

/// The sign-in page.
pub(crate) fn page(request: &Request) -> Html {
    let locale = &request.locale;
    let issuer = request.issuer.clone();
    let back = request
        .param(BACK_PARAM)
        .or_else(|| request.param(ferrohealth_ui::view::session::RETURN))
        .and_then(|back| crate::routes::return_path(&back).map(str::to_owned))
        .unwrap_or_else(|| ui_path(""));
    let login = format!(
        "{LOGIN_ROUTE}?{}",
        url::form_urlencoded::Serializer::new(String::new())
            .append_pair(BACK_PARAM, &back)
            .finish()
    );
    let named = request
        .param("state")
        .and_then(|key| SignInState::parse(&key));
    let state = match &issuer {
        None => state_of(locale, Some(SignInState::Unavailable), &login),
        Some(_) => state_of(locale, named, &login),
    };
    // NOTE: FerroUI `docs/consoles.md`, "The session's end": the page a session that
    // ended lands on tells every other tab of the console, which then leaves too.
    let ended = matches!(
        named,
        Some(SignInState::SignedOut | SignInState::Expired | SignInState::ExpiredAbsolute)
    )
    .then(session_ended);
    let rejected = matches!(state, State::Rejected(_));
    let unavailable = matches!(state, State::Unavailable(_));
    let provider = issuer
        .as_ref()
        .filter(|_| !rejected && !unavailable)
        .map(|issuer| {
            let host = provider_host(issuer);
            Provider {
                href: login.clone(),
                label: format!("{} {host}", tr(locale, Msg::SignInAt)),
                busy: format!("{} {host}", tr(locale, Msg::SignInTaking)),
                glyph: Glyph::LOG_IN,
            }
        });
    let read = ui_path("");
    let line = if rejected || unavailable {
        h(format!(
            r#"<a href="{}" class="fui-button fui-button-primary">{}</a>"#,
            t(&read),
            ferrohealth_ui::view::button::label(
                Some(Glyph::ARROW_RIGHT),
                tr(locale, Msg::SignInRead)
            )
        ))
    } else {
        h(format!(
            r#"<p class="fui-small fui-sign-in-line">{}{}</p>"#,
            link(&read, t(tr(locale, Msg::SignInRead))),
            t(tr(locale, Msg::SignInReadLine))
        ))
    };
    let title = if unavailable {
        tr(locale, Msg::SignInReadOnlyTitle)
    } else {
        tr(locale, Msg::SignInTitle)
    };
    let card = sign_in(
        locale,
        SignIn {
            lockup: lockup_plain(
                brand_mark(MARK_LIGHT, MARK_DARK),
                "FerroTERM",
                tr(locale, Msg::ShellConsole),
            ),
            title: Some(title.clone()),
            state,
            provider,
            basic: None,
            line: Some(line),
            note: (!unavailable).then(|| tr(locale, Msg::SignInNote)),
            links: vec![
                (tr(locale, Msg::SignInDocumentation), String::from(DOCS)),
                (tr(locale, Msg::SignInPrivacy), String::from(PRIVACY)),
            ],
            caption: Some(tr(locale, Msg::SignInCaption)),
        },
    );
    request.answering.title(&title);
    Html::join([
        centred(
            locale,
            card,
            ferrohealth_ui::view::toast::toast_region(locale, Vec::new()),
            footer(),
        ),
        ended.unwrap_or_default(),
    ])
}
