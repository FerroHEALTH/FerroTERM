// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sign-in page (R19): Sign in to edit at the identity provider, Read
//! without signing in, and the notice of why the page is shown.
//!
//! Reading is open, so the page always offers the way to read without
//! signing in. The sign-in itself is the authorization code flow with PKCE
//! at the configured OpenID Provider, which the console's server runs
//! (`crate::oidc`); the reader returns to the page they came from.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::shell::{brand_mark, centred, lockup_plain};
use ferrohealth_ui::view::sign_in::{Notice, Provider, Retry, SignIn, State, sign_in};
use leptos::prelude::*;
use leptos_meta::Title;

use crate::auth::SignInState;
use crate::locale::{Msg, tr};
use crate::request::Request;
use crate::routes::{BACK_PARAM, LOGIN_ROUTE, ui_path};
use crate::shell::footer;

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
fn state_of(state: Option<SignInState>, retry: &str) -> State {
    let notice = |msg: Msg| Notice::new(tr(msg));
    let again = |msg: Msg| Notice {
        retry: Some(Retry {
            label: tr(Msg::SignInTryAgain),
            href: retry.to_owned(),
        }),
        ..Notice::new(tr(msg))
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
            detail: Some(tr(Msg::SignInUnavailableDetail)),
            ..Notice::new(tr(Msg::SignInUnavailable))
        }),
    }
}

/// The sign-in page.
pub(crate) async fn page(request: Request) -> AnyView {
    let issuer = request.issuer.clone();
    let back = request
        .param(BACK_PARAM)
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
        None => state_of(Some(SignInState::Unavailable), &login),
        Some(_) => state_of(named, &login),
    };
    let rejected = matches!(state, State::Rejected(_));
    let unavailable = matches!(state, State::Unavailable(_));
    let provider = issuer
        .as_ref()
        .filter(|_| !rejected && !unavailable)
        .map(|issuer| {
            let host = provider_host(issuer);
            Provider {
                href: login.clone(),
                label: format!("{} {host}", tr(Msg::SignInAt)),
                busy: format!("{} {host}", tr(Msg::SignInTaking)),
                glyph: Glyph::LOG_IN,
            }
        });
    let read = ui_path("");
    let line = if rejected || unavailable {
        view! {
            <a href=read class="fui-button fui-button-primary">
                {ferrohealth_ui::view::button::label(Some(Glyph::ARROW_RIGHT), tr(Msg::SignInRead))}
            </a>
        }
        .into_any()
    } else {
        view! {
            <p class="fui-small fui-sign-in-line">
                <a href=read>{tr(Msg::SignInRead)}</a>
                {tr(Msg::SignInReadLine)}
            </p>
        }
        .into_any()
    };
    let title = if unavailable {
        tr(Msg::SignInReadOnlyTitle)
    } else {
        tr(Msg::SignInTitle)
    };
    let card = sign_in(SignIn {
        lockup: lockup_plain(
            brand_mark("/mark-light.svg", "/mark-dark.svg"),
            "FerroTERM",
            tr(Msg::ShellConsole),
        )
        .into_any(),
        title: Some(title.clone()),
        state,
        provider,
        basic: None,
        line: Some(line),
        note: (!unavailable).then(|| tr(Msg::SignInNote)),
        links: vec![
            (tr(Msg::SignInDocumentation), String::from(DOCS)),
            (tr(Msg::SignInPrivacy), String::from(PRIVACY)),
        ],
        caption: Some(tr(Msg::SignInCaption)),
    })
    .into_any();
    view! {
        <Title text=title />
        {centred(card, footer())}
    }
    .into_any()
}
