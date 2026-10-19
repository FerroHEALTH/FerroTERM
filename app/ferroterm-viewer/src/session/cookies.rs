// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The cookies that carry a session's and a pending sign-in's id, and the
//! read of one from a request.

use cookie::{Cookie, SameSite};
use http::HeaderMap;

use crate::session::{COOKIE, HOST_PREFIX, SIGN_IN_COOKIE, SessionId, Sessions, THEME_COOKIE};

impl Sessions {
    /// The name `base` takes on this console: [`HOST_PREFIX`] and `base` when
    /// the cookies are `Secure`, and `base` alone when they are not.
    #[must_use]
    pub fn cookie_name(&self, base: &str) -> String {
        if self.settings.secure_cookie {
            format!("{HOST_PREFIX}{base}")
        } else {
            base.to_owned()
        }
    }

    /// The id the request's `Cookie` header carries under the cookie `base`
    /// names on this console, if any.
    #[must_use]
    pub fn cookie_in(&self, headers: &HeaderMap, base: &str) -> Option<SessionId> {
        let name = self.cookie_name(base);
        // NOTE: RFC 6265 §5.4: a header or pair that does not read as a cookie
        // is not the cookie asked for, so it is skipped and never an error.
        headers
            .get_all(http::header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(Cookie::split_parse)
            .filter_map(Result::ok)
            .find(|cookie| cookie.name() == name)
            .map(|cookie| SessionId::from_cookie(cookie.value()))
    }

    /// The `Set-Cookie` value that hands the browser the pending sign-in
    /// `id`, expiring with the sign-in.
    #[must_use]
    pub fn sign_in_cookie(&self, id: &SessionId) -> Cookie<'static> {
        let max_age = i64::try_from(self.settings.sign_in_timeout.as_secs()).unwrap_or(i64::MAX);
        self.cookie(SIGN_IN_COOKIE, id.as_str())
            .max_age(cookie::time::Duration::seconds(max_age))
            .build()
    }

    /// The `Set-Cookie` value that removes the pending sign-in cookie.
    #[must_use]
    pub fn sign_in_cookie_removal(&self) -> Cookie<'static> {
        self.cookie(SIGN_IN_COOKIE, "")
            .max_age(cookie::time::Duration::ZERO)
            .build()
    }

    /// The `Set-Cookie` value that hands the browser the signed-in session
    /// `id`.
    #[must_use]
    pub fn session_cookie(&self, id: &SessionId) -> Cookie<'static> {
        self.cookie(COOKIE, id.as_str()).build()
    }

    /// The `Set-Cookie` value that removes the signed-in session cookie.
    #[must_use]
    pub fn session_cookie_removal(&self) -> Cookie<'static> {
        self.cookie(COOKIE, "")
            .max_age(cookie::time::Duration::ZERO)
            .build()
    }

    /// The `Set-Cookie` value that keeps the operator's theme choice `theme`
    /// for 400 days, the longest a browser keeps a cookie (RFC 6265bis
    /// §5.6.2).
    #[must_use]
    pub fn theme_cookie(&self, theme: &str) -> Cookie<'static> {
        self.preference_cookie(THEME_COOKIE, theme)
    }

    /// The `Set-Cookie` value that keeps the reader's preference `base` at
    /// `value` for 400 days, as [`Self::theme_cookie`] keeps the theme.
    #[must_use]
    pub fn preference_cookie(&self, base: &str, value: &str) -> Cookie<'static> {
        self.cookie(base, value)
            .max_age(cookie::time::Duration::days(400))
            .build()
    }

    /// A cookie that no script reads (`HttpOnly`), that a cross-site
    /// subrequest does not carry while the provider's redirect back does
    /// (`SameSite=Lax`, RFC 6265bis §5.4.7), and that is `Secure` unless the
    /// configuration turns it off.
    fn cookie(&self, base: &str, value: &str) -> cookie::CookieBuilder<'static> {
        Cookie::build((self.cookie_name(base), value.to_owned()))
            .path("/")
            .http_only(true)
            .secure(self.settings.secure_cookie)
            .same_site(SameSite::Lax)
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderMap;
    use http::header::COOKIE;

    use crate::config::settings::SessionSettings;
    use crate::session::tests::settings;
    use crate::session::{SessionId, Sessions};

    #[test]
    fn the_cookies_are_host_bound_http_only_lax_and_secure_and_the_sign_in_one_expires() {
        let sessions = Sessions::new(settings());
        let id = SessionId::from_cookie("abc");
        assert_eq!(
            "__Host-ferroterm_viewer_session=abc; HttpOnly; SameSite=Lax; Secure; Path=/",
            sessions.session_cookie(&id).to_string()
        );
        assert_eq!(
            "__Host-ferroterm_viewer_sign_in=abc; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=60",
            sessions.sign_in_cookie(&id).to_string()
        );
        assert!(
            sessions
                .sign_in_cookie_removal()
                .to_string()
                .contains("Max-Age=0")
        );
    }

    #[test]
    fn the_theme_cookie_is_host_bound_http_only_lax_secure_and_kept_400_days() {
        let sessions = Sessions::new(settings());
        assert_eq!(
            "__Host-ferroterm_viewer_theme=dark; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=34560000",
            sessions.theme_cookie("dark").to_string()
        );
    }

    #[test]
    fn a_cookie_that_is_not_secure_carries_no_host_prefix_a_browser_would_refuse() {
        let sessions = Sessions::new(SessionSettings {
            secure_cookie: false,
            ..settings()
        });
        assert_eq!(
            "ferroterm_viewer_session",
            sessions.cookie_name(super::COOKIE)
        );
    }

    #[test]
    fn the_session_cookie_removal_expires_the_same_host_bound_cookie() {
        let sessions = Sessions::new(settings());
        assert_eq!(
            "__Host-ferroterm_viewer_session=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0",
            sessions.session_cookie_removal().to_string()
        );
    }

    #[test]
    fn only_the_prefixed_cookie_is_read_and_a_malformed_pair_is_skipped() {
        let sessions = Sessions::new(settings());
        let mut headers = HeaderMap::new();
        headers.append(
            COOKIE,
            "ferroterm_viewer_session=shadow; =broken"
                .parse()
                .expect("a header"),
        );
        headers.append(
            COOKIE,
            "__Host-ferroterm_viewer_session=real"
                .parse()
                .expect("a header"),
        );
        let read = sessions.cookie_in(&headers, super::COOKIE);
        assert_eq!(Some("real"), read.as_ref().map(SessionId::as_str));
    }
}
