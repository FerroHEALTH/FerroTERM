// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[auth]` shapes of `ferrohealth-smart` the server refuses to
//! start with, each naming the key or the user to change.
//!
//! No specification governs the configuration: our own design, the family's
//! fail-closed rule (#1140).

use super::basic::{password, phc};
use super::{SigningKey, causes, issuer};
use crate::fixture::{Server, SmartSetup};
use ferrohealth_smart::basic::StoreError;
use std::collections::BTreeMap;

use ferrohealth_smart::config::{BasicSection, BasicUser, ConfigError};
use ferroterm_server::smart::{Smart, StartError};

/// The reason the server refuses to start under `setup`.
async fn refused(setup: SmartSetup) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    causes(&error)
}

// ferrohealth-smart 0.15 deleted the named refusals of the earlier `[auth]`
// shape, so the strict load refuses each as a key it does not know.
#[test]
fn the_earlier_auth_shape_is_refused_by_the_strict_load() {
    for text in [
        "[auth]\nissuer = \"https://idp.example\"\n",
        "[auth]\naudience = [\"ferroterm\"]\n",
    ] {
        assert!(
            ferroterm_server::config::Config::from_sources(Some(text), &BTreeMap::new()).is_err(),
            "{text}"
        );
    }
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

// A Basic credential is a person's, and a `system/` scope is a backend
// service's grant (SMART App Launch 2.2.0, Scopes and Launch Context), so
// `ferrohealth-smart` 0.9.1 refuses a Basic user granted one when the section
// loads (GHSA-jhg2-jwjp-hq74), and the start carries that refusal.
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
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    let StartError::Auth { source } = &error else {
        panic!("the section's load refuses the start: {error:?}");
    };
    let ConfigError::Basic {
        source: StoreError::SystemScope { username, scope },
    } = source.as_ref()
    else {
        panic!("the crate refuses the Basic user's system scope: {source:?}");
    };
    assert_eq!(username, "batch-loader");
    assert_eq!(scope, "system/CodeSystem.cud");
    let reason = causes(&error);
    assert!(reason.contains("batch-loader"), "{reason}");
    assert!(reason.contains("system/"), "{reason}");
}
