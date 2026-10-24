// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checked settings the console runs on, resolved from the written
//! [`Config`] with every `_file` secret read.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use ferrohealth_ui::i18n::Locale;
use secrecy::SecretString;
use url::Url;

use crate::config::error::Error;
use crate::config::{Config, Oidc};

/// The resolved configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The socket address the console listens on.
    pub listen: SocketAddr,
    /// The directory holding the site bundle.
    pub site_root: PathBuf,
    /// The locale a page renders in when the request names none the
    /// console's catalog holds.
    pub locale: Locale,
    /// The terminology server the console is a client of.
    pub ferroterm: FerrotermSettings,
    /// The server-side sign-in sessions.
    pub session: SessionSettings,
    /// The OpenID Provider operators sign in with, when one is configured.
    pub oidc: Option<OidcSettings>,
    /// The configuration file the settings were read from, when one was
    /// named, which the Configuration page names.
    pub file: Option<PathBuf>,
}

/// The resolved `[ferroterm]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FerrotermSettings {
    /// The root of the server's HTTP surface, an `http` or `https` URL whose
    /// path ends in `/`, so a relative route joins beneath it.
    pub base: Url,
    /// The root the readers reach the server at, which the console shows.
    pub public: Url,
    /// The root of the sync service's admin listener, when one is configured.
    pub sync: Option<Url>,
    /// The root of the terminology server's admin listener, when one is
    /// configured.
    pub admin: Option<Url>,
    /// How long one call to the server may take.
    pub timeout: Duration,
}

impl FerrotermSettings {
    /// The public root as the console shows it, without its final `/`.
    #[must_use]
    pub fn shown_root(&self) -> String {
        self.public.as_str().trim_end_matches('/').to_owned()
    }
}

/// The resolved `[session]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    /// Whether the cookies carry `Secure`, and with it the `__Host-` prefix.
    pub secure_cookie: bool,
    /// How long a pending sign-in waits for the provider's redirect back.
    pub sign_in_timeout: Duration,
    /// How many pending sign-ins the console holds at once.
    pub max_sign_ins: usize,
    /// How long a signed-in session lives without a request.
    pub idle_timeout: Duration,
    /// How long a signed-in session lives at most.
    pub absolute_timeout: Duration,
    /// How many signed-in sessions the console holds at once.
    pub max_sessions: usize,
}

/// The resolved `[oidc]`.
#[derive(Debug, Clone)]
pub struct OidcSettings {
    /// The provider's issuer identifier, exactly as written: an ID Token's
    /// `iss` must equal it (OpenID Connect Core 1.0 §3.1.3.7).
    pub issuer: String,
    /// The provider's authorization endpoint.
    pub authorization_endpoint: Url,
    /// The provider's token endpoint.
    pub token_endpoint: Url,
    /// The provider's JWK Set location.
    pub jwks_uri: Url,
    /// The console's client identifier.
    pub client_id: String,
    /// The console's client secret, when it is a confidential client.
    pub client_secret: Option<SecretString>,
    /// The file the client secret was read from, when it was read from one,
    /// which the Configuration page names in its place.
    pub client_secret_file: Option<PathBuf>,
    /// The console's redirection endpoint.
    pub redirect_uri: Url,
    /// The scopes requested, `openid` among them.
    pub scopes: Vec<String>,
    /// Whether the sign-in asks for `offline_access`.
    pub offline_access: bool,
    /// The provider's end-session endpoint, when it publishes one.
    pub end_session_endpoint: Option<Url>,
    /// Where the provider sends the operator after a sign-out.
    pub post_logout_redirect_uri: Option<Url>,
    /// The provider's token revocation endpoint, when it publishes one.
    pub revocation_endpoint: Option<Url>,
}

impl OidcSettings {
    /// The console's own origin, `scheme://host:port`: the origin of its
    /// redirection endpoint, which an unsafe request must come from.
    #[must_use]
    pub fn origin(&self) -> String {
        self.redirect_uri.origin().ascii_serialization()
    }
}

/// The scope value that asks for a refresh token that outlives the
/// operator's session at the provider (OpenID Connect Core 1.0 §11).
pub const OFFLINE_ACCESS: &str = "offline_access";

impl Config {
    /// Checks every value and reads every `_file` secret.
    ///
    /// # Errors
    /// Returns [`Error::Address`] for a listen address that does not parse,
    /// [`Error::Locale`] for a `server.locale` that is no language tag,
    /// [`Error::Missing`], [`Error::Url`] and [`Error::UrlShape`] for a URL
    /// that is absent, does not parse or is not one its key admits,
    /// [`Error::Zero`] for a zero timeout or pool bound, [`Error::Scope`] for
    /// `offline_access` listed in `oidc.scopes`, [`Error::InsecureCookie`]
    /// for `session.secure_cookie = false` on a console that is not `http`
    /// on loopback, and the secret errors of `oidc.client_secret`.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self
            .server
            .listen
            .parse()
            .map_err(|source| Error::Address {
                key: String::from("server.listen"),
                source,
            })?;
        let locale = Locale::parse(&self.server.locale).map_err(|source| Error::Locale {
            key: String::from("server.locale"),
            source,
        })?;
        let base = base_url("ferroterm.base_url", &self.ferroterm.base_url)?;
        let public = self
            .ferroterm
            .public_url
            .as_deref()
            .map(|text| base_url("ferroterm.public_url", text))
            .transpose()?
            .unwrap_or_else(|| base.clone());
        let sync = self
            .ferroterm
            .sync_url
            .as_deref()
            .map(|text| base_url("ferroterm.sync_url", text))
            .transpose()?;
        let admin = self
            .ferroterm
            .admin_url
            .as_deref()
            .map(|text| base_url("ferroterm.admin_url", text))
            .transpose()?;
        let timeout = positive("ferroterm.timeout_ms", self.ferroterm.timeout_ms)
            .map(Duration::from_millis)?;
        let seconds = |key: &str, value: u64| positive(key, value).map(Duration::from_secs);
        let session = &self.session;
        let session = SessionSettings {
            secure_cookie: session.secure_cookie,
            sign_in_timeout: seconds("session.sign_in_timeout_s", session.sign_in_timeout_s)?,
            max_sign_ins: bound("session.max_sign_ins", session.max_sign_ins)?,
            idle_timeout: seconds("session.idle_timeout_s", session.idle_timeout_s)?,
            absolute_timeout: seconds("session.absolute_timeout_s", session.absolute_timeout_s)?,
            max_sessions: bound("session.max_sessions", session.max_sessions)?,
        };
        let oidc = self.oidc.as_ref().map(resolve_oidc).transpose()?;
        // NOTE: RFC 6265bis §4.1.2.5: a cookie without `Secure` travels over plain
        // HTTP, so it is admitted only where the console itself is HTTP on loopback.
        if !session.secure_cookie
            && oidc.as_ref().is_some_and(|oidc| {
                !(oidc.redirect_uri.scheme() == "http" && is_loopback(&oidc.redirect_uri))
            })
        {
            return Err(Error::InsecureCookie {
                key: String::from("session.secure_cookie"),
            });
        }
        Ok(Settings {
            listen,
            site_root: self.server.site_root.clone(),
            locale,
            ferroterm: FerrotermSettings {
                base,
                public,
                sync,
                admin,
                timeout,
            },
            session,
            oidc,
            file: None,
        })
    }
}

/// Resolves `[oidc]`, reading its client secret.
fn resolve_oidc(oidc: &Oidc) -> Result<OidcSettings, Error> {
    provider_url("oidc.issuer", &oidc.issuer)?;
    let authorization_endpoint =
        provider_url("oidc.authorization_endpoint", &oidc.authorization_endpoint)?;
    let token_endpoint = provider_url("oidc.token_endpoint", &oidc.token_endpoint)?;
    let jwks_uri = provider_url("oidc.jwks_uri", &oidc.jwks_uri)?;
    let redirect_uri = web_url("oidc.redirect_uri", &oidc.redirect_uri)?;
    // NOTE: RFC 6749 §3.1.2: the redirection endpoint URI MUST NOT include a
    // fragment component.
    if redirect_uri.fragment().is_some() {
        return Err(Error::UrlShape {
            key: String::from("oidc.redirect_uri"),
            reason: "must not carry a fragment",
        });
    }
    if oidc.client_id.trim().is_empty() {
        return Err(Error::Missing {
            key: String::from("oidc.client_id"),
        });
    }
    // NOTE: OpenID Connect Core 1.0 §3.1.2.1: an OpenID Connect request MUST
    // carry the `openid` scope value.
    if !oidc.scopes.iter().any(|scope| scope == "openid") {
        return Err(Error::Missing {
            key: String::from("oidc.scopes (openid)"),
        });
    }
    // NOTE: OpenID Connect Core 1.0 §11: `offline_access` goes with `prompt`
    // `consent`, which the console sends only for `oidc.offline_access`.
    if oidc.scopes.iter().any(|scope| scope == OFFLINE_ACCESS) {
        return Err(Error::Scope {
            key: String::from("oidc.scopes"),
            reason: "must not list offline_access; set oidc.offline_access = true",
        });
    }
    let end_session_endpoint = oidc
        .end_session_endpoint
        .as_deref()
        .map(|text| provider_url("oidc.end_session_endpoint", text))
        .transpose()?;
    let post_logout_redirect_uri = oidc
        .post_logout_redirect_uri
        .as_deref()
        .map(|text| web_url("oidc.post_logout_redirect_uri", text))
        .transpose()?;
    // NOTE: OpenID Connect RP-Initiated Logout 1.0 §3: the post-logout redirect
    // is sent to the end-session endpoint, so one without the other is refused.
    if post_logout_redirect_uri.is_some() && end_session_endpoint.is_none() {
        return Err(Error::Missing {
            key: String::from("oidc.end_session_endpoint (for post_logout_redirect_uri)"),
        });
    }
    let revocation_endpoint = oidc
        .revocation_endpoint
        .as_deref()
        .map(|text| provider_url("oidc.revocation_endpoint", text))
        .transpose()?;
    let client_secret = crate::config::secret::resolve(
        "oidc.client_secret",
        oidc.client_secret.as_ref(),
        oidc.client_secret_file.as_deref(),
    )?;
    Ok(OidcSettings {
        issuer: oidc.issuer.clone(),
        authorization_endpoint,
        token_endpoint,
        jwks_uri,
        client_id: oidc.client_id.clone(),
        client_secret,
        client_secret_file: oidc.client_secret_file.clone(),
        redirect_uri,
        scopes: oidc.scopes.clone(),
        offline_access: oidc.offline_access,
        end_session_endpoint,
        post_logout_redirect_uri,
        revocation_endpoint,
    })
}

/// Returns `value`, refusing zero.
fn positive(key: &str, value: u64) -> Result<u64, Error> {
    if value == 0 {
        Err(Error::Zero {
            key: key.to_owned(),
        })
    } else {
        Ok(value)
    }
}

/// Returns the pool bound `value`, refusing zero.
fn bound(key: &str, value: usize) -> Result<usize, Error> {
    if value == 0 {
        Err(Error::Zero {
            key: key.to_owned(),
        })
    } else {
        Ok(value)
    }
}

/// Parses an `http` or `https` URL with a host and no userinfo.
fn web_url(key: &str, text: &str) -> Result<Url, Error> {
    if text.trim().is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    let url = Url::parse(text).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    let shape = |reason| Error::UrlShape {
        key: key.to_owned(),
        reason,
    };
    if !matches!(url.scheme(), "http" | "https") {
        return Err(shape("must be an http or https URL"));
    }
    if url.host().is_none() {
        return Err(shape("must name a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(shape("must not carry userinfo"));
    }
    Ok(url)
}

/// Parses an OpenID Provider URL: `https`, or `http` on a loopback host.
fn provider_url(key: &str, text: &str) -> Result<Url, Error> {
    let url = web_url(key, text)?;
    // NOTE: RFC 6749 §3.1: the authorization server MUST require TLS at its
    // authorization endpoint, so plain `http` is admitted only on loopback.
    if url.scheme() == "https" || is_loopback(&url) {
        Ok(url)
    } else {
        Err(Error::UrlShape {
            key: key.to_owned(),
            reason: "must be an https URL unless its host is loopback",
        })
    }
}

/// Parses an `http` or `https` URL with a host, no userinfo, no query and no
/// fragment, and ends its path in `/`.
fn base_url(key: &str, text: &str) -> Result<Url, Error> {
    let mut url = web_url(key, text)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::UrlShape {
            key: key.to_owned(),
            reason: "must not carry a query or a fragment",
        });
    }
    // NOTE: RFC 3986 §5.2.3: a reference merges with the base path up to its
    // last `/`, so the base ends in one or its last segment would be dropped.
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// Whether `url` names a loopback host: `localhost`, `127.0.0.0/8` or `::1`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::config::Config;
    use crate::config::error::Error;

    #[test]
    fn the_locale_is_english_unless_configured_and_a_bad_tag_is_refused_by_key() {
        let settings = Config::from_sources(None, &BTreeMap::new())
            .expect("the defaults read")
            .resolve()
            .expect("the defaults resolve");
        assert_eq!("en", settings.locale.tag());
        let configured =
            Config::from_sources(Some("[server]\nlocale = \"en-GB\"\n"), &BTreeMap::new())
                .expect("the tree reads")
                .resolve()
                .expect("a language tag resolves");
        assert_eq!("en-GB", configured.locale.tag());
        let refused =
            Config::from_sources(Some("[server]\nlocale = \"not a tag\"\n"), &BTreeMap::new())
                .expect("the tree reads")
                .resolve()
                .expect_err("no language tag");
        assert!(
            matches!(&refused, Error::Locale { key, .. } if key == "server.locale"),
            "{refused:?}"
        );
    }
}
