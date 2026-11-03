// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's sign-in state, held on the server in two separate pools.
//!
//! The browser carries opaque, random ids in `HttpOnly` cookies and nothing
//! else; every fact behind an id stays in this process (RFC 6749 §10.12,
//! RFC 7636 §4.1).
//!
//! - **Sign-ins** are the pre-login state of `GET /login`: the `state`, the
//!   OpenID Connect `nonce` and the PKCE verifier. They live for a short
//!   timeout and are taken once by the callback. The pool is bounded, and a
//!   full pool drops its oldest sign-in to admit a new one, so a flood of
//!   login starts costs at most the bound and stops costing anything once it
//!   ends.
//! - **Sessions** are signed-in operators. They are created only once a
//!   sign-in completes, have an idle and an absolute timeout, and a bound of
//!   their own that refuses a new session rather than evicting one. No
//!   amount of sign-in traffic touches this pool. A session that holds a
//!   refresh token outlives its access token ([`refresh`]); its idle and
//!   absolute timeouts still end it.
//!
//! No specification governs the session store: our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use aws_lc_rs::rand::{SecureRandom as _, SystemRandom};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret as _, SecretString};

use crate::config::settings::SessionSettings;

mod cookies;
pub mod refresh;
pub mod request;
pub mod watch;

/// The name of the cookie that carries a signed-in session, under the
/// `__Host-` prefix when the cookie is `Secure`.
pub const COOKIE: &str = "ferroterm_console_session";

/// The name of the short-lived cookie that carries a pending sign-in, under
/// the `__Host-` prefix when the cookie is `Secure`.
pub const SIGN_IN_COOKIE: &str = "ferroterm_console_sign_in";

/// The cookie name prefix that binds a cookie to the host that set it.
///
/// It holds for a `Secure`, `Path=/` cookie with no `Domain`, so no sibling
/// host can set or shadow it (RFC 6265bis §4.1.3.2), and a browser refuses
/// it on a cookie that is not `Secure`.
pub const HOST_PREFIX: &str = "__Host-";

/// How many random bytes an id, a `state`, a `nonce` and a PKCE verifier
/// carry: 256 bits, the entropy RFC 7636 §7.1 recommends for the verifier.
const RANDOM_BYTES: usize = 32;

/// Why a sign-in or a session could not be begun or read.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The system random source failed.
    #[error("the system random source failed")]
    Random,
    /// The session pool holds as many signed-in sessions as it may.
    #[error("the session pool is full")]
    Full,
    /// A thread panicked while it held the store.
    #[error("the session store is unusable after a panic")]
    Poisoned,
}

/// An opaque id, the value of a cookie.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionId(String);

impl SessionId {
    /// Takes the value a browser sent in a cookie.
    #[must_use]
    pub fn from_cookie(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// The id as the cookie carries it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Mints a fresh random id.
    fn mint() -> Result<Self, SessionError> {
        random_token().map(Self)
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId(..)")
    }
}

/// A sign-in the console sent to the OpenID Provider and is waiting on.
pub struct PendingSignIn {
    /// The `state` the authorization request carried (RFC 6749 §4.1.1).
    state: String,
    /// The `nonce` the ID Token must carry back (OpenID Connect Core 1.0
    /// §3.1.2.1).
    nonce: String,
    /// The PKCE code verifier its challenge was made from (RFC 7636 §4.1).
    verifier: SecretString,
    /// The console's page the reader returns to once signed in.
    back: Option<String>,
}

impl PendingSignIn {
    /// Mints a fresh `state`, `nonce` and PKCE verifier.
    ///
    /// # Errors
    /// Returns [`SessionError::Random`] when the system random source fails.
    pub fn mint() -> Result<Self, SessionError> {
        Ok(Self {
            state: random_token()?,
            nonce: random_token()?,
            verifier: SecretString::from(random_token()?),
            back: None,
        })
    }

    /// This sign-in, returning to the console's page `back` once it is done.
    #[must_use]
    pub fn returning_to(self, back: Option<String>) -> Self {
        Self { back, ..self }
    }

    /// The console's page the reader returns to once signed in.
    #[must_use]
    pub fn back(&self) -> Option<&str> {
        self.back.as_deref()
    }

    /// The `state` the authorization request carries.
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }

    /// The `nonce` the authorization request carries.
    #[must_use]
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    /// The PKCE code verifier.
    #[must_use]
    pub fn verifier(&self) -> &SecretString {
        &self.verifier
    }

    /// Whether `state` is the one this sign-in sent, compared in constant
    /// time.
    #[must_use]
    pub fn answers(&self, state: &str) -> bool {
        aws_lc_rs::constant_time::verify_slices_are_equal(self.state.as_bytes(), state.as_bytes())
            .is_ok()
    }
}

impl fmt::Debug for PendingSignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingSignIn").finish_non_exhaustive()
    }
}

/// How many entries each pool holds, expired ones not yet dropped included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occupancy {
    /// Pending sign-ins.
    pub sign_ins: usize,
    /// Signed-in sessions.
    pub sessions: usize,
}

/// One pending sign-in in the pool.
#[derive(Debug)]
struct Pending {
    /// Its place in the order of arrival, the key of [`Store::order`].
    sequence: u64,
    /// When it was begun.
    created: Instant,
    /// What the callback checks the redirect against.
    sign_in: PendingSignIn,
}

/// One signed-in session.
#[derive(Debug)]
struct Session {
    /// When the sign-in completed.
    created: Instant,
    /// When the session last saw a request.
    touched: Instant,
    /// The reader's access token, which the console calls the FHIR server with.
    access_token: SecretString,
    /// When the access token expires, when the provider said.
    expires: Option<Instant>,
    /// How long the access token lasts in all, when the provider said.
    lifetime: Option<Duration>,
    /// The refresh token the provider issued, which never leaves the server
    /// (RFC 6749 §10.4).
    refresh_token: Option<SecretString>,
    /// Held by the one refresh of this session in flight, so a rotated
    /// refresh token is never spent twice.
    refresh_gate: Arc<tokio::sync::Mutex<()>>,
    /// The ID Token of the sign-in, the hint a sign-out sends the provider.
    id_token: Option<SecretString>,
    /// The operator's name as the provider gave it.
    operator: Option<String>,
    /// The scopes the access token grants.
    scopes: Vec<String>,
}

/// What an ended session held that the provider is told about.
#[derive(Debug)]
pub struct Ended {
    /// The ID Token of the sign-in, the hint of the end-session request.
    pub id_token: Option<SecretString>,
    /// The token a revocation names: the refresh token when the provider
    /// issued one, since revoking it ends its access tokens too (RFC 7009
    /// §2.1), and the access token otherwise.
    pub revocable: (SecretString, Hint),
}

/// The `token_type_hint` of a revocation request (RFC 7009 §2.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hint {
    /// `access_token`.
    AccessToken,
    /// `refresh_token`.
    RefreshToken,
}

impl Hint {
    /// The hint as the request names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccessToken => "access_token",
            Self::RefreshToken => "refresh_token",
        }
    }
}

/// What a completed sign-in leaves the session.
///
/// It is the operator's access token, how long the provider says it lasts
/// (RFC 6749 §5.1), the refresh token when the provider issued one, and the
/// ID Token a sign-out hints with (OpenID Connect RP-Initiated Logout 1.0
/// §2).
pub struct SignedIn {
    /// The access token.
    pub access_token: SecretString,
    /// Its lifetime, `expires_in`, when the provider sent one.
    pub expires_in: Option<Duration>,
    /// The refresh token, when the provider issued one.
    pub refresh_token: Option<SecretString>,
    /// The verified ID Token of the sign-in.
    pub id_token: Option<SecretString>,
    /// The operator's name, from the verified ID Token's `name` or
    /// `preferred_username` claim (OpenID Connect Core 1.0 §5.1).
    pub operator: Option<String>,
    /// The scopes the access token grants (RFC 6749 §5.1, §3.3).
    pub scopes: Vec<String>,
}

impl fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedIn")
            .field("expires_in", &self.expires_in)
            .field("operator", &self.operator)
            .finish_non_exhaustive()
    }
}

/// Both pools, under one lock.
#[derive(Debug, Default)]
struct Store {
    /// The pending sign-ins by id.
    pending: BTreeMap<SessionId, Pending>,
    /// The pending sign-ins by order of arrival, oldest first.
    order: BTreeMap<u64, SessionId>,
    /// The sequence number the next sign-in takes.
    next: u64,
    /// The signed-in sessions by id.
    sessions: BTreeMap<SessionId, Session>,
}

impl Store {
    /// Removes the pending sign-in `id`, from both of its indexes.
    fn remove_pending(&mut self, id: &SessionId) -> Option<Pending> {
        let pending = self.pending.remove(id)?;
        self.order.remove(&pending.sequence);
        Some(pending)
    }

    /// Drops the oldest pending sign-in.
    fn drop_oldest_pending(&mut self) {
        if let Some((_, id)) = self.order.pop_first() {
            self.pending.remove(&id);
        }
    }
}

/// The session store, shared by every request.
#[derive(Debug, Clone)]
pub struct Sessions {
    settings: SessionSettings,
    store: Arc<Mutex<Store>>,
}

impl Sessions {
    /// An empty store under `settings`.
    #[must_use]
    pub fn new(settings: SessionSettings) -> Self {
        Self {
            settings,
            store: Arc::new(Mutex::new(Store::default())),
        }
    }

    /// Begins a pending sign-in and returns the id its cookie carries.
    ///
    /// Expired sign-ins are dropped first; when the pool is still full, the
    /// oldest pending sign-in is dropped to make room. The session pool is
    /// never read or changed.
    ///
    /// # Errors
    /// Returns [`SessionError::Random`] when no id could be minted and
    /// [`SessionError::Poisoned`] when the store is unusable.
    pub fn begin(&self, sign_in: PendingSignIn) -> Result<SessionId, SessionError> {
        let id = SessionId::mint()?;
        let now = Instant::now();
        let mut store = self.lock()?;
        let timeout = self.settings.sign_in_timeout;
        while let Some(oldest) = store.order.first_key_value().map(|(_, id)| id.clone()) {
            let expired = store
                .pending
                .get(&oldest)
                .is_none_or(|pending| now.saturating_duration_since(pending.created) >= timeout);
            if !expired {
                break;
            }
            store.remove_pending(&oldest);
        }
        while store.pending.len() >= self.settings.max_sign_ins {
            store.drop_oldest_pending();
        }
        let sequence = store.next;
        store.next = sequence.saturating_add(1);
        store.order.insert(sequence, id.clone());
        store.pending.insert(
            id.clone(),
            Pending {
                sequence,
                created: now,
                sign_in,
            },
        );
        Ok(id)
    }

    /// Takes the pending sign-in `id`, once.
    ///
    /// Answers `None` for an unknown, dropped or expired sign-in.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn take_pending(&self, id: &SessionId) -> Result<Option<PendingSignIn>, SessionError> {
        let now = Instant::now();
        let mut store = self.lock()?;
        let Some(pending) = store.remove_pending(id) else {
            return Ok(None);
        };
        if now.saturating_duration_since(pending.created) >= self.settings.sign_in_timeout {
            return Ok(None);
        }
        Ok(Some(pending.sign_in))
    }

    /// Creates a signed-in session, once a sign-in has completed, and returns
    /// the id its cookie carries.
    ///
    /// Expired sessions are dropped first; a pool still full refuses the new
    /// session and keeps every live one.
    ///
    /// # Errors
    /// Returns [`SessionError::Full`] when the pool is at its bound,
    /// [`SessionError::Random`] when no id could be minted, and
    /// [`SessionError::Poisoned`] when the store is unusable.
    pub fn establish(&self, signed_in: SignedIn) -> Result<SessionId, SessionError> {
        let id = SessionId::mint()?;
        let now = Instant::now();
        let mut store = self.lock()?;
        let settings = self.settings;
        store
            .sessions
            .retain(|_, session| live(&settings, session, now));
        if store.sessions.len() >= settings.max_sessions {
            return Err(SessionError::Full);
        }
        store.sessions.insert(
            id.clone(),
            Session {
                created: now,
                touched: now,
                access_token: signed_in.access_token,
                expires: signed_in
                    .expires_in
                    .and_then(|lifetime| now.checked_add(lifetime)),
                lifetime: signed_in.expires_in,
                refresh_token: signed_in.refresh_token,
                refresh_gate: Arc::default(),
                id_token: signed_in.id_token,
                operator: signed_in.operator,
                scopes: signed_in.scopes,
            },
        );
        Ok(id)
    }

    /// The access token of the live signed-in session `id`, which the
    /// request now touches; an expired session is dropped and answers `None`.
    ///
    /// A session that holds a refresh token outlives its access token, and
    /// answers `None` while its access token is expired and not yet
    /// refreshed.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn access_token(&self, id: &SessionId) -> Result<Option<SecretString>, SessionError> {
        let now = Instant::now();
        let mut store = self.lock()?;
        let Some(session) = store.sessions.get_mut(id) else {
            return Ok(None);
        };
        if live(&self.settings, session, now) {
            session.touched = now;
            let valid = session.expires.is_none_or(|expires| now < expires);
            Ok(valid.then(|| session.access_token.clone()))
        } else {
            store.sessions.remove(id);
            Ok(None)
        }
    }

    /// The name of the operator of the live signed-in session `id`, as the
    /// provider gave it; `None` for a session that is not live, or whose
    /// provider gave no name.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn operator(&self, id: &SessionId) -> Result<Option<String>, SessionError> {
        let now = Instant::now();
        let store = self.lock()?;
        Ok(store
            .sessions
            .get(id)
            .filter(|session| live(&self.settings, session, now))
            .and_then(|session| session.operator.clone()))
    }

    /// The scopes the access token of the live signed-in session `id`
    /// grants; `None` for a session that is not live.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn scopes(&self, id: &SessionId) -> Result<Option<Vec<String>>, SessionError> {
        let now = Instant::now();
        let store = self.lock()?;
        Ok(store
            .sessions
            .get(id)
            .filter(|session| live(&self.settings, session, now))
            .map(|session| session.scopes.clone()))
    }

    /// What the live signed-in session `id` has left until its idle timeout
    /// and its absolute lifetime; `None` for a session that is not live.
    ///
    /// A read leaves the session as it is; with `extend` the request counts
    /// as the reader's activity and starts the idle timeout again, which
    /// never reaches past the absolute lifetime.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn remaining(
        &self,
        id: &SessionId,
        extend: bool,
    ) -> Result<Option<Remaining>, SessionError> {
        let now = Instant::now();
        let mut store = self.lock()?;
        let Some(session) = store.sessions.get_mut(id) else {
            return Ok(None);
        };
        if !live(&self.settings, session, now) {
            return Ok(None);
        }
        if extend {
            session.touched = now;
        }
        let mut absolute = self
            .settings
            .absolute_timeout
            .saturating_sub(now.saturating_duration_since(session.created));
        // An access token no refresh token can replace ends the session at its
        // expiry, as `live` decides, so the watch reports no time past it.
        if session.refresh_token.is_none()
            && let Some(expires) = session.expires
        {
            absolute = absolute.min(expires.saturating_duration_since(now));
        }
        let idle = self
            .settings
            .idle_timeout
            .saturating_sub(now.saturating_duration_since(session.touched))
            .min(absolute);
        Ok(Some(Remaining { idle, absolute }))
    }

    /// How many entries each pool holds.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn occupancy(&self) -> Result<Occupancy, SessionError> {
        let store = self.lock()?;
        Ok(Occupancy {
            sign_ins: store.pending.len(),
            sessions: store.sessions.len(),
        })
    }

    /// Ends the signed-in session `id`, if it is held, and returns the
    /// tokens it held, for the provider's revocation and end-session
    /// requests.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn end(&self, id: &SessionId) -> Result<Option<Ended>, SessionError> {
        Ok(self.lock()?.sessions.remove(id).map(|session| Ended {
            id_token: session.id_token,
            revocable: match session.refresh_token {
                Some(refresh) => (refresh, Hint::RefreshToken),
                None => (session.access_token, Hint::AccessToken),
            },
        }))
    }

    /// Locks the store.
    fn lock(&self) -> Result<MutexGuard<'_, Store>, SessionError> {
        self.store
            .lock()
            .map_err(|_poisoned| SessionError::Poisoned)
    }
}

/// What a live session has left until each of its timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Remaining {
    /// Until the idle timeout, unless the reader acts.
    pub idle: Duration,
    /// Until the absolute lifetime ends, which nothing extends.
    pub absolute: Duration,
}

/// Whether `session` is inside its idle and its absolute timeout at `now`,
/// and its access token has not expired or a refresh token can replace it.
fn live(settings: &SessionSettings, session: &Session, now: Instant) -> bool {
    now.saturating_duration_since(session.touched) < settings.idle_timeout
        && now.saturating_duration_since(session.created) < settings.absolute_timeout
        && (session.refresh_token.is_some() || session.expires.is_none_or(|expires| now < expires))
}

/// The PKCE `S256` challenge of `verifier`: the base64url of its SHA-256
/// (RFC 7636 §4.2).
#[must_use]
pub fn challenge(verifier: &SecretString) -> String {
    let digest = aws_lc_rs::digest::digest(
        &aws_lc_rs::digest::SHA256,
        verifier.expose_secret().as_bytes(),
    );
    URL_SAFE_NO_PAD.encode(digest.as_ref())
}

/// Returns [`RANDOM_BYTES`] fresh random bytes as base64url, 43 characters
/// of the unreserved set RFC 7636 §4.1 admits for a verifier.
fn random_token() -> Result<String, SessionError> {
    let mut bytes = [0_u8; RANDOM_BYTES];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_unspecified| SessionError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests;
