// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[auth]` shapes of `ferrohealth-smart` 0.9.0 the server refuses to
//! start with, each naming the key or the user to change.
//!
//! No specification governs the configuration: our own design, the family's
//! fail-closed rule (#1140).

use super::basic::{password, phc};
use super::{SigningKey, causes, issuer};
use crate::fixture::{Server, SmartSetup};
use ferrohealth_smart::config::{BasicSection, BasicUser, IssuerKey};
use ferroterm_server::smart::Smart;

/// The reason the server refuses to start under `setup`.
async fn refused(setup: SmartSetup) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    causes(&error)
}

#[tokio::test]
async fn the_flat_issuer_of_the_old_shape_refuses_the_start_naming_the_table() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.issuer = IssuerKey::Flat(mock.uri());
    let reason = refused(setup).await;
    assert!(reason.contains("auth.issuer = "), "{reason}");
    assert!(reason.contains("[[auth.issuer]]"), "{reason}");
}

#[tokio::test]
async fn a_section_level_issuer_setting_of_the_old_shape_refuses_the_start_naming_it() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.audience = Some(serde_json::json!(["ferroterm"]));
    let reason = refused(setup).await;
    assert!(reason.contains("auth.audience is removed"), "{reason}");
}

#[tokio::test]
async fn an_issuer_without_a_kind_refuses_the_start_naming_the_key() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.issuer_mut().kind = None;
    let reason = refused(setup).await;
    assert!(reason.contains("auth.issuer.kind"), "{reason}");
}

// A Basic credential is a person's (`ferrohealth-smart` 0.9.0), and a
// `system/` scope is a backend service's grant (SMART App Launch 2.2.0,
// Scopes and Launch Context), so a machine authenticates with a client
// credentials token (RFC 6749 §4.4) and a Basic user granted one refuses the
// start.
#[tokio::test]
async fn a_basic_user_with_a_system_scope_refuses_the_start_naming_the_user() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.basic = Some(BasicSection {
        users: vec![BasicUser {
            user: String::from("batch-loader"),
            password_hash: Some(phc(&password())),
            scopes: vec![String::from("system/CodeSystem.cud")],
            ..BasicUser::default()
        }],
        ..BasicSection::default()
    });
    let reason = refused(setup).await;
    assert!(reason.contains("batch-loader"), "{reason}");
    assert!(reason.contains("system/"), "{reason}");
}
