// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's client of the terminology server: the one place an
//! [`Exchange`] leaves the console, server to server.
//!
//! A path below one of the FHIR roots (`/r4`, `/r4b`, `/r5`, `/r6`) or the
//! health route goes to the configured FHIR server, `/runs`, `/run` and
//! `/activate` to the configured sync service's admin listener, and
//! `/reload` to the configured server's admin listener; any other path is
//! refused before a request is sent. A signed-in reader's access token rides every
//! request to either as a bearer credential (RFC 6750 §2.1), so a write is
//! the reader's own and the server decides what it allows, and the sync
//! service admits the reader whose token carries its product scope; a
//! visitor's request carries none, as the server's read surface asks for
//! none. The token never leaves this process. An answer of any status is
//! handed back as it came, never turned into an empty one. No specification
//! governs this: our own design.

use http::header::{
    ACCEPT, AUTHORIZATION, CONTENT_LOCATION, CONTENT_TYPE, ETAG, IF_MATCH, LOCATION,
};
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;

use crate::fhir::exchange::{ACTIVATE, Answer, Exchange, ExchangeError, Method, RELOAD, RUN, RUNS};
use crate::server::ConsoleState;

/// The FHIR roots the console forwards to, each a first path segment.
pub const FHIR_ROOTS: [&str; 4] = ["r4", "r4b", "r5", "r6"];

/// The terminology server's health route.
pub const HEALTH: &str = "health";

/// The largest body the console reads from an answer.
pub const BODY_LIMIT: usize = 32 * 1024 * 1024;

/// Where a path goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The FHIR server: a FHIR root or the health route.
    Fhir,
    /// The sync service's admin listener.
    Sync,
    /// The terminology server's admin listener.
    Admin,
}

/// Where `path` goes, or `None` for a path the console forwards nowhere.
///
/// The path must start with `/`, name a known first segment, and hold no
/// `..` or `.` segment and no backslash, so a forwarded request never leaves
/// the root it names.
#[must_use]
pub fn target(path: &str) -> Option<Target> {
    let rest = path.strip_prefix('/')?;
    let (route, _query) = rest.split_once('?').unwrap_or((rest, ""));
    if route.contains('\\')
        || route
            .split('/')
            .any(|segment| segment == ".." || segment == ".")
    {
        return None;
    }
    let first = route.split('/').next().unwrap_or_default();
    if FHIR_ROOTS.contains(&first) || route == HEALTH {
        Some(Target::Fhir)
    } else if first == RUNS || route == RUN || route == ACTIVATE {
        Some(Target::Sync)
    } else if route == RELOAD {
        Some(Target::Admin)
    } else {
        None
    }
}

/// The URL `path` names below `base`, whose path ends in `/`.
///
/// The joined URL is read back: a segment the URL parser resolves as a dot
/// segment (`%2e%2e` among them, WHATWG URL, path state) could otherwise
/// climb out of the root `path` named, so the path below `base` must still
/// name the same target.
fn joined(base: &Url, path: &str, expected: Target) -> Result<Url, ExchangeError> {
    let url = base
        .join(path.trim_start_matches('/'))
        .map_err(|_unjoinable| ExchangeError::Forbidden)?;
    let below = url
        .path()
        .strip_prefix(base.path())
        .ok_or(ExchangeError::Forbidden)?;
    if url.origin() != base.origin() || target(&format!("/{below}")) != Some(expected) {
        return Err(ExchangeError::Forbidden);
    }
    Ok(url)
}

/// The console's way to the terminology server for one request: the
/// console's state and the access token of the request's reader, if one is
/// signed in.
#[derive(Clone)]
pub struct Upstream {
    state: ConsoleState,
    /// The reader's token, `None` for a visitor; an `Err` when the session
    /// store could not be read, which refuses every exchange.
    token: Result<Option<SecretString>, ExchangeError>,
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let token = match &self.token {
            Ok(Some(_token)) => "[redacted]",
            Ok(None) => "none",
            Err(_unread) => "unread",
        };
        f.debug_struct("Upstream")
            .field("token", &token)
            .finish_non_exhaustive()
    }
}

impl Upstream {
    /// The way to the server for the reader of the request whose headers
    /// are `headers`: its live session's access token, none for a visitor.
    #[must_use]
    pub fn of(state: &ConsoleState, headers: &http::HeaderMap) -> Self {
        let token = crate::session::request::access_token(state, headers).map_err(|error| {
            tracing::error!(%error, "the session store could not be read");
            ExchangeError::Sessions
        });
        Self {
            state: state.clone(),
            token,
        }
    }

    /// Sends `exchange` as the reader and answers what came back.
    ///
    /// # Errors
    /// Returns [`ExchangeError::Forbidden`] for a path the console forwards
    /// nowhere, [`ExchangeError::NoSync`] for the runs on a console with no
    /// sync service, [`ExchangeError::NoAdmin`] for a reload on a console
    /// with no admin listener, [`ExchangeError::Sessions`] when the session
    /// store could not be read, and [`ExchangeError::Unreachable`] when no
    /// answer came.
    pub async fn send(&self, exchange: Exchange) -> Result<Answer, ExchangeError> {
        let token = self.token.clone()?;
        forward(&self.state, token.as_ref(), exchange).await
    }
}

/// Sends `exchange` through `state`'s client, carrying `token`, and answers
/// what came back.
async fn forward(
    state: &ConsoleState,
    token: Option<&SecretString>,
    exchange: Exchange,
) -> Result<Answer, ExchangeError> {
    let target = target(&exchange.path).ok_or(ExchangeError::Forbidden)?;
    let url = match target {
        Target::Fhir => joined(&state.settings().ferroterm.base, &exchange.path, target)?,
        Target::Sync => {
            let base = state
                .settings()
                .ferroterm
                .sync
                .as_ref()
                .ok_or(ExchangeError::NoSync)?;
            joined(base, &exchange.path, target)?
        }
        Target::Admin => {
            let base = state
                .settings()
                .ferroterm
                .admin
                .as_ref()
                .ok_or(ExchangeError::NoAdmin)?;
            joined(base, &exchange.path, target)?
        }
    };
    let client = state.upstream();
    let method = match exchange.method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Delete => reqwest::Method::DELETE,
    };
    let mut request = client
        .request(method, url)
        .header(ACCEPT, exchange.accept.as_str());
    if let Some(content_type) = &exchange.content_type {
        request = request.header(CONTENT_TYPE, content_type.as_str());
    }
    if let Some(if_match) = &exchange.if_match {
        request = request.header(IF_MATCH, if_match.as_str());
    }
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {}", token.expose_secret()));
    }
    if let Some(body) = exchange.body {
        request = request.body(body);
    }
    let response = request.send().await.map_err(|error| {
        tracing::warn!(method = exchange.method.name(), %error, "the terminology server could not be reached");
        ExchangeError::Unreachable {
            message: error.without_url().to_string(),
        }
    })?;
    let status = response.status().as_u16();
    let header = |name: http::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let etag = header(ETAG);
    let location = header(LOCATION).or_else(|| header(CONTENT_LOCATION));
    let bytes = response
        .bytes()
        .await
        .map_err(|error| ExchangeError::Unreachable {
            message: error.without_url().to_string(),
        })?;
    if bytes.len() > BODY_LIMIT {
        return Err(ExchangeError::Unreachable {
            message: format!("the answer is over {BODY_LIMIT} bytes"),
        });
    }
    Ok(Answer {
        status,
        etag,
        location,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Target, target};

    #[test]
    fn only_the_fhir_roots_the_health_route_and_the_runs_are_forwarded() {
        for path in [
            "/r4/metadata",
            "/r4b/ValueSet?url=http%3A%2F%2Fx",
            "/r5/CodeSystem/$lookup?system=a&code=b",
            "/r6",
            "/health",
        ] {
            assert_eq!(Some(Target::Fhir), target(path), "{path}");
        }
        assert_eq!(Some(Target::Sync), target("/runs/abc"));
        assert_eq!(Some(Target::Sync), target("/run"));
        assert_eq!(Some(Target::Sync), target("/activate"));
        assert_eq!(Some(Target::Admin), target("/reload"));
        for path in [
            "",
            "r4/metadata",
            "/admin",
            "/healthz",
            "/r4/../reload",
            "/r4/./x",
            "/r4\\x",
            "//attacker.example/r4",
            "/metrics",
            "/run/x",
            "/reload/x",
            "/activate/../reload",
        ] {
            assert_eq!(None, target(path), "{path}");
        }
    }
}
