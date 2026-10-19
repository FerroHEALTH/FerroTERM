// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refresh of a session's access token as the store holds it: when one
//! is due, the refresh token it is made with, and the tokens it answers
//! (RFC 6749 §6).
//!
//! The refresh token stays in this process and never reaches the browser
//! (RFC 6749 §10.4). The request itself is [`crate::oidc::refresh`]'s.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use secrecy::SecretString;

use crate::session::{Session, SessionError, SessionId, Sessions, live};

/// What a refresh of the access token answered (RFC 6749 §6).
pub struct Refreshed {
    /// The new access token.
    pub access_token: SecretString,
    /// Its lifetime, `expires_in`, when the provider sent one.
    pub expires_in: Option<Duration>,
    /// The new refresh token, when the provider rotated it.
    pub refresh_token: Option<SecretString>,
    /// The scopes the refreshed token grants, when the provider stated them.
    pub scopes: Option<Vec<String>>,
}

impl fmt::Debug for Refreshed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Refreshed")
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Whether the access token expires within `margin` of `now` and a
    /// refresh token is held to replace it.
    ///
    /// The margin is at most half the token's lifetime, so a token that
    /// lasts no longer than the margin is not refreshed on every request.
    fn refresh_due(&self, margin: Duration, now: Instant) -> bool {
        let margin = self
            .lifetime
            .map_or(margin, |lifetime| margin.min(lifetime / 2));
        self.refresh_token.is_some()
            && self
                .expires
                .is_some_and(|expires| expires.saturating_duration_since(now) <= margin)
    }
}

impl Sessions {
    /// The gate of the live session `id` when its access token expires
    /// within `margin` and a refresh token is held to replace it.
    ///
    /// A refresh holds the gate while it runs and asks [`Self::refresh_grant`]
    /// again once it has it, so concurrent requests refresh once.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn refresh_due(
        &self,
        id: &SessionId,
        margin: Duration,
    ) -> Result<Option<Arc<tokio::sync::Mutex<()>>>, SessionError> {
        let now = Instant::now();
        let store = self.lock()?;
        Ok(store
            .sessions
            .get(id)
            .filter(|session| {
                live(&self.settings, session, now) && session.refresh_due(margin, now)
            })
            .map(|session| Arc::clone(&session.refresh_gate)))
    }

    /// The refresh token of the live session `id` when its access token
    /// still expires within `margin`, for the refresh request.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn refresh_grant(
        &self,
        id: &SessionId,
        margin: Duration,
    ) -> Result<Option<SecretString>, SessionError> {
        let now = Instant::now();
        let store = self.lock()?;
        Ok(store
            .sessions
            .get(id)
            .filter(|session| {
                live(&self.settings, session, now) && session.refresh_due(margin, now)
            })
            .and_then(|session| session.refresh_token.clone()))
    }

    /// Replaces the access token of the session `id` with a refreshed one,
    /// and its refresh token with a rotated one, which the old one is then
    /// discarded for (RFC 6749 §6).
    ///
    /// The idle and the absolute timeouts are unchanged. Answers whether the
    /// session was still held.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn refreshed(&self, id: &SessionId, refreshed: Refreshed) -> Result<bool, SessionError> {
        let now = Instant::now();
        let mut store = self.lock()?;
        let Some(session) = store.sessions.get_mut(id) else {
            return Ok(false);
        };
        session.access_token = refreshed.access_token;
        session.expires = refreshed
            .expires_in
            .and_then(|lifetime| now.checked_add(lifetime));
        session.lifetime = refreshed.expires_in;
        if let Some(rotated) = refreshed.refresh_token {
            session.refresh_token = Some(rotated);
        }
        if let Some(scopes) = refreshed.scopes {
            session.scopes = scopes;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use secrecy::{ExposeSecret, SecretString};

    use crate::config::settings::SessionSettings;
    use crate::session::refresh::Refreshed;
    use crate::session::tests::{settings, signed_in};
    use crate::session::{SessionId, Sessions, SignedIn};

    const MARGIN: Duration = Duration::from_secs(60);

    /// A sign-in whose access token is already expired and that holds the
    /// refresh token `refresh`.
    fn expired(refresh: &str) -> SignedIn {
        SignedIn {
            expires_in: Some(Duration::ZERO),
            refresh_token: Some(SecretString::from(refresh)),
            ..signed_in()
        }
    }

    fn grant(sessions: &Sessions, id: &SessionId) -> Option<String> {
        sessions
            .refresh_grant(id, MARGIN)
            .expect("usable")
            .map(|token| token.expose_secret().to_owned())
    }

    #[test]
    fn a_session_with_a_refresh_token_outlives_its_access_token_until_refreshed() {
        let sessions = Sessions::new(settings());
        let id = sessions
            .establish(expired("synthetic-refresh-1"))
            .expect("room");
        assert!(sessions.access_token(&id).expect("usable").is_none());
        assert_eq!(1, sessions.occupancy().expect("usable").sessions);
        assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_some());
        assert_eq!(
            Some("synthetic-refresh-1"),
            grant(&sessions, &id).as_deref()
        );
        let refreshed = Refreshed {
            access_token: SecretString::from("synthetic-access-2"),
            expires_in: Some(Duration::from_secs(3600)),
            refresh_token: None,
            scopes: None,
        };
        assert!(sessions.refreshed(&id, refreshed).expect("usable"));
        let token = sessions.access_token(&id).expect("usable").expect("live");
        assert_eq!("synthetic-access-2", token.expose_secret());
        assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_none());
    }

    #[test]
    fn a_rotated_refresh_token_replaces_the_old_one() {
        let sessions = Sessions::new(settings());
        let id = sessions
            .establish(expired("synthetic-refresh-1"))
            .expect("room");
        let refreshed = Refreshed {
            access_token: SecretString::from("synthetic-access-2"),
            expires_in: Some(Duration::ZERO),
            refresh_token: Some(SecretString::from("synthetic-refresh-2")),
            scopes: None,
        };
        assert!(sessions.refreshed(&id, refreshed).expect("usable"));
        assert_eq!(
            Some("synthetic-refresh-2"),
            grant(&sessions, &id).as_deref()
        );
    }

    #[test]
    fn a_session_with_no_refresh_token_ends_with_its_access_token() {
        let sessions = Sessions::new(settings());
        let id = sessions
            .establish(SignedIn {
                expires_in: Some(Duration::ZERO),
                ..signed_in()
            })
            .expect("room");
        assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_none());
        assert!(sessions.access_token(&id).expect("usable").is_none());
        assert_eq!(0, sessions.occupancy().expect("usable").sessions);
    }

    #[test]
    fn the_idle_and_absolute_timeouts_end_a_session_whatever_its_refresh_token() {
        for timed_out in [
            SessionSettings {
                idle_timeout: Duration::ZERO,
                ..settings()
            },
            SessionSettings {
                absolute_timeout: Duration::ZERO,
                ..settings()
            },
        ] {
            let sessions = Sessions::new(timed_out);
            let id = sessions
                .establish(expired("synthetic-refresh-1"))
                .expect("room");
            assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_none());
            assert!(grant(&sessions, &id).is_none());
            assert!(sessions.access_token(&id).expect("usable").is_none());
            assert_eq!(0, sessions.occupancy().expect("usable").sessions);
        }
    }

    #[test]
    fn a_token_not_yet_within_the_margin_is_not_refreshed() {
        let sessions = Sessions::new(settings());
        let id = sessions
            .establish(SignedIn {
                expires_in: Some(Duration::from_secs(3600)),
                refresh_token: Some(SecretString::from("synthetic-refresh-1")),
                ..signed_in()
            })
            .expect("room");
        assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_none());
        assert!(sessions.access_token(&id).expect("usable").is_some());
    }

    #[test]
    fn a_short_lived_token_is_refreshed_in_the_second_half_of_its_lifetime_only() {
        let sessions = Sessions::new(settings());
        let id = sessions
            .establish(SignedIn {
                expires_in: Some(Duration::from_secs(30)),
                refresh_token: Some(SecretString::from("synthetic-refresh-1")),
                ..signed_in()
            })
            .expect("room");
        assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_none());
        assert!(sessions.access_token(&id).expect("usable").is_some());
        let refreshed = Refreshed {
            access_token: SecretString::from("synthetic-access-2"),
            expires_in: Some(Duration::from_secs(10)),
            refresh_token: None,
            scopes: None,
        };
        assert!(sessions.refreshed(&id, refreshed).expect("usable"));
        assert!(sessions.refresh_due(&id, MARGIN).expect("usable").is_none());
    }

    #[test]
    fn neither_a_sign_in_nor_a_refresh_answer_shows_a_token_in_debug_output() {
        let shown = format!(
            "{:?} {:?}",
            Refreshed {
                access_token: SecretString::from("synthetic-access-2"),
                expires_in: None,
                refresh_token: Some(SecretString::from("synthetic-refresh-2")),
                scopes: None,
            },
            expired("synthetic-refresh-1")
        );
        assert!(!shown.contains("synthetic-"), "{shown}");
    }
}
