// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session store's tests.

use super::{
    Hint, Occupancy, PendingSignIn, SessionError, SessionId, Sessions, SignedIn, challenge,
};
use crate::config::settings::SessionSettings;
use secrecy::{ExposeSecret, SecretString};
use std::time::Duration;

pub(super) fn settings() -> SessionSettings {
    SessionSettings {
        secure_cookie: true,
        sign_in_timeout: Duration::from_secs(60),
        max_sign_ins: 4,
        idle_timeout: Duration::from_secs(60),
        absolute_timeout: Duration::from_secs(600),
        max_sessions: 2,
    }
}

fn sign_in() -> PendingSignIn {
    PendingSignIn::mint().expect("the random source works")
}

pub(super) fn signed_in() -> SignedIn {
    SignedIn {
        access_token: SecretString::from("synthetic-access-token"),
        expires_in: None,
        refresh_token: None,
        id_token: Some(SecretString::from("synthetic-id-token")),
        operator: Some(String::from("Synthetic Operator")),
        scopes: vec![String::from("user/CodeSystem.cud")],
    }
}

#[test]
fn the_s256_challenge_is_the_one_rfc_7636_appendix_b_derives() {
    let verifier = SecretString::from("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    assert_eq!(
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        challenge(&verifier)
    );
}

#[test]
fn a_minted_sign_in_carries_a_verifier_of_the_length_rfc_7636_admits() {
    let pending = sign_in();
    let verifier = pending.verifier().expose_secret().len();
    assert!((43..=128).contains(&verifier), "{verifier}");
    assert_ne!(pending.state(), pending.verifier().expose_secret());
    assert_ne!(pending.state(), pending.nonce());
}

#[test]
fn a_pending_sign_in_is_taken_once_and_only_with_its_own_id() {
    let sessions = Sessions::new(settings());
    let pending = sign_in();
    let state = pending.state().to_owned();
    let id = sessions.begin(pending).expect("usable");
    assert!(
        sessions
            .take_pending(&SessionId::from_cookie("not-a-sign-in"))
            .expect("usable")
            .is_none()
    );
    let taken = sessions
        .take_pending(&id)
        .expect("usable")
        .expect("pending");
    assert!(taken.answers(&state));
    assert!(!taken.answers("another-state"));
    assert!(sessions.take_pending(&id).expect("usable").is_none());
}

#[test]
fn a_sign_in_remembers_the_page_to_return_to_through_the_pool() {
    let sessions = Sessions::new(settings());
    assert_eq!(None, sign_in().back(), "a fresh sign-in returns nowhere");
    let pending = sign_in().returning_to(Some(String::from("/systems?fhir=r4")));
    let state = pending.state().to_owned();
    let id = sessions.begin(pending).expect("usable");
    let taken = sessions
        .take_pending(&id)
        .expect("usable")
        .expect("pending");
    assert_eq!(Some("/systems?fhir=r4"), taken.back());
    assert!(taken.answers(&state), "the page changes nothing else");
}

#[test]
fn the_operator_and_scopes_are_read_only_from_a_live_session() {
    let sessions = Sessions::new(settings());
    let id = sessions.establish(signed_in()).expect("room");
    assert_eq!(
        Some(String::from("Synthetic Operator")),
        sessions.operator(&id).expect("usable")
    );
    assert_eq!(
        Some(vec![String::from("user/CodeSystem.cud")]),
        sessions.scopes(&id).expect("usable")
    );
    let unknown = SessionId::from_cookie("not-a-session");
    assert_eq!(None, sessions.operator(&unknown).expect("usable"));
    assert_eq!(None, sessions.scopes(&unknown).expect("usable"));
    let idle = Sessions::new(SessionSettings {
        idle_timeout: Duration::ZERO,
        ..settings()
    });
    let id = idle.establish(signed_in()).expect("room");
    assert_eq!(None, idle.operator(&id).expect("usable"));
    assert_eq!(None, idle.scopes(&id).expect("usable"));
}

#[test]
fn a_full_sign_in_pool_drops_its_oldest_and_never_touches_the_sessions() {
    let sessions = Sessions::new(settings());
    let operator = sessions.establish(signed_in()).expect("room for a session");
    let first = sessions.begin(sign_in()).expect("usable");
    let mut last = first.clone();
    for _ in 0..100 {
        last = sessions.begin(sign_in()).expect("usable");
    }
    assert_eq!(
        Occupancy {
            sign_ins: 4,
            sessions: 1
        },
        sessions.occupancy().expect("usable")
    );
    assert!(sessions.take_pending(&first).expect("usable").is_none());
    assert!(sessions.take_pending(&last).expect("usable").is_some());
    assert!(sessions.access_token(&operator).expect("usable").is_some());
}

#[test]
fn an_expired_sign_in_is_refused_and_dropped() {
    let sessions = Sessions::new(SessionSettings {
        sign_in_timeout: Duration::ZERO,
        ..settings()
    });
    let id = sessions.begin(sign_in()).expect("usable");
    assert!(sessions.take_pending(&id).expect("usable").is_none());
    sessions.begin(sign_in()).expect("usable");
    assert_eq!(1, sessions.occupancy().expect("usable").sign_ins);
}

#[test]
fn a_full_session_pool_refuses_a_new_session_and_keeps_the_live_ones() {
    let sessions = Sessions::new(settings());
    let first = sessions.establish(signed_in()).expect("room");
    let second = sessions.establish(signed_in()).expect("room");
    let refused = sessions.establish(signed_in());
    assert!(matches!(refused, Err(SessionError::Full)), "{refused:?}");
    assert!(sessions.access_token(&first).expect("usable").is_some());
    assert!(sessions.access_token(&second).expect("usable").is_some());
}

#[test]
fn a_session_past_its_idle_or_absolute_timeout_is_no_longer_live() {
    for expired in [
        SessionSettings {
            idle_timeout: Duration::ZERO,
            ..settings()
        },
        SessionSettings {
            absolute_timeout: Duration::ZERO,
            ..settings()
        },
    ] {
        let sessions = Sessions::new(expired);
        let id = sessions.establish(signed_in()).expect("room");
        assert!(sessions.access_token(&id).expect("usable").is_none());
        assert_eq!(0, sessions.occupancy().expect("usable").sessions);
    }
}

#[test]
fn a_read_of_what_is_left_extends_nothing_and_an_extension_restarts_the_idle_timeout() {
    let sessions = Sessions::new(settings());
    let id = sessions.establish(signed_in()).expect("room");
    std::thread::sleep(Duration::from_millis(5));
    let read = sessions
        .remaining(&id, false)
        .expect("usable")
        .expect("live");
    assert!(read.idle < Duration::from_secs(60), "{read:?}");
    assert!(read.absolute < Duration::from_secs(600), "{read:?}");
    let again = sessions
        .remaining(&id, false)
        .expect("usable")
        .expect("live");
    assert!(again.idle <= read.idle, "a read never extends: {again:?}");
    let extended = sessions
        .remaining(&id, true)
        .expect("usable")
        .expect("live");
    assert_eq!(Duration::from_secs(60), extended.idle);
    assert!(
        extended.absolute <= read.absolute,
        "nothing extends the lifetime"
    );
    sessions.end(&id).expect("usable");
    assert_eq!(None, sessions.remaining(&id, true).expect("usable"));
}

#[test]
fn the_watch_of_a_session_whose_token_cannot_be_refreshed_ends_at_the_token_expiry() {
    let sessions = Sessions::new(settings());
    let short = sessions
        .establish(SignedIn {
            expires_in: Some(Duration::from_secs(30)),
            ..signed_in()
        })
        .expect("room");
    let left = sessions
        .remaining(&short, true)
        .expect("usable")
        .expect("live");
    assert!(left.absolute <= Duration::from_secs(30), "{left:?}");
    assert!(left.idle <= left.absolute, "{left:?}");
    let refreshable = sessions
        .establish(SignedIn {
            expires_in: Some(Duration::from_secs(30)),
            refresh_token: Some(SecretString::from("synthetic-refresh-token")),
            ..signed_in()
        })
        .expect("room");
    let left = sessions
        .remaining(&refreshable, true)
        .expect("usable")
        .expect("live");
    assert!(
        left.absolute > Duration::from_secs(30),
        "a refresh token outlives the access token: {left:?}"
    );
    assert_eq!(Duration::from_secs(60), left.idle);
}

#[test]
fn an_ended_session_names_its_refresh_token_for_revocation_when_it_has_one() {
    let sessions = Sessions::new(settings());
    let id = sessions.establish(signed_in()).expect("room");
    let (token, hint) = sessions.end(&id).expect("usable").expect("held").revocable;
    assert_eq!(
        ("synthetic-access-token", Hint::AccessToken),
        (token.expose_secret(), hint)
    );
    let id = sessions
        .establish(SignedIn {
            refresh_token: Some(SecretString::from("synthetic-refresh-token")),
            ..signed_in()
        })
        .expect("room");
    let (token, hint) = sessions.end(&id).expect("usable").expect("held").revocable;
    assert_eq!(
        ("synthetic-refresh-token", Hint::RefreshToken),
        (token.expose_secret(), hint)
    );
}

#[test]
fn an_ended_session_is_no_longer_live_and_hands_back_its_id_token() {
    let sessions = Sessions::new(settings());
    let id = sessions.establish(signed_in()).expect("room");
    let ended = sessions.end(&id).expect("usable").expect("held");
    assert_eq!(
        Some("synthetic-id-token"),
        ended.id_token.as_ref().map(ExposeSecret::expose_secret)
    );
    assert!(sessions.access_token(&id).expect("usable").is_none());
    assert!(sessions.end(&id).expect("usable").is_none());
    assert_eq!(0, sessions.occupancy().expect("usable").sessions);
}

#[test]
fn neither_an_id_nor_a_sign_in_shows_its_value_in_debug_output() {
    let pending = sign_in();
    let shown = format!("{:?} {pending:?}", SessionId::from_cookie("abc"));
    assert!(!shown.contains("abc"), "{shown}");
    assert!(!shown.contains(pending.state()), "{shown}");
    assert!(!shown.contains(pending.nonce()), "{shown}");
}

#[test]
fn a_poisoned_store_answers_poisoned_everywhere() {
    let sessions = Sessions::new(settings());
    let id = sessions.establish(signed_in()).expect("room");
    let store = std::sync::Arc::clone(&sessions.store);
    let poisoner = std::thread::spawn(move || {
        let _held = store.lock();
        panic!("a thread panics holding the session store");
    });
    assert!(poisoner.join().is_err(), "the poisoning thread panics");
    assert!(matches!(
        sessions.access_token(&id),
        Err(SessionError::Poisoned)
    ));
    assert!(matches!(
        sessions.begin(sign_in()),
        Err(SessionError::Poisoned)
    ));
    assert!(matches!(sessions.end(&id), Err(SessionError::Poisoned)));
}
