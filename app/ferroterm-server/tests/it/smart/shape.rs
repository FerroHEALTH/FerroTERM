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

use ferrohealth_smart::config::{BasicGrant, BasicSection, BasicUser};
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

/// `setup` granting `user` the one `scope` under `[basic_grants]`.
fn granting(mut setup: SmartSetup, user: &str, scope: &str) -> SmartSetup {
    setup.basic_grants.users.insert(
        String::from(user),
        BasicGrant {
            scopes: vec![String::from(scope)],
            ..BasicGrant::default()
        },
    );
    setup
}

/// `setup` admitting the one Basic user `user`.
fn admitting(mut setup: SmartSetup, user: &str) -> SmartSetup {
    setup.auth.basic = Some(BasicSection {
        users: vec![BasicUser {
            user: String::from(user),
            password_hash: Some(phc(&password())),
            ..BasicUser::default()
        }],
        ..BasicSection::default()
    });
    setup
}

/// The `[basic_grants]` refusal the server does not start under `setup` with.
async fn grant_refusal(setup: SmartSetup) -> (StoreError, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    let reason = causes(&error);
    let StartError::BasicGrants { source } = error else {
        panic!("the grants refuse the start: {error:?}");
    };
    (source, reason)
}

// A Basic credential is a person's, and a `system/` scope is a backend
// service's grant (SMART App Launch 2.2.0, Scopes and Launch Context), so
// `ferrohealth-smart` refuses a Basic user granted one (GHSA-jhg2-jwjp-hq74),
// and the start carries that refusal.
#[tokio::test]
async fn a_basic_user_with_a_system_scope_refuses_the_start_naming_the_user() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let setup = admitting(SmartSetup::for_issuer(&mock.uri()), "batch-loader");
    let setup = granting(setup, "batch-loader", "system/CodeSystem.cud");
    let (source, reason) = grant_refusal(setup).await;
    let StoreError::SystemScope { username, scope } = source else {
        panic!("the crate refuses the Basic user's system scope: {source:?}");
    };
    assert_eq!(username, "batch-loader");
    assert_eq!(scope, "system/CodeSystem.cud");
    assert!(reason.contains("batch-loader"), "{reason}");
    assert!(reason.contains("system/"), "{reason}");
}

// A grant whose user-id `[[auth.basic.users]]` does not list would grant no
// one, so a misspelt user-id refuses the start (our own design).
#[tokio::test]
async fn a_grant_naming_no_listed_user_refuses_the_start_naming_the_user() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    for setup in [
        admitting(SmartSetup::for_issuer(&mock.uri()), "operator"),
        SmartSetup::for_issuer(&mock.uri()),
    ] {
        let setup = granting(setup, "opertor", "__ferroterm.admin");
        let (source, reason) = grant_refusal(setup).await;
        let StoreError::UnknownUser { username } = source else {
            panic!("the crate refuses the unknown user: {source:?}");
        };
        assert_eq!(username, "opertor");
        assert!(reason.contains("opertor"), "{reason}");
    }
}

// A Basic user's scopes and roles are `[basic_grants]`'s since
// ferrohealth-smart 0.23, so the strict load refuses either under
// `[[auth.basic.users]]` as a key it does not know.
#[test]
fn a_scope_or_a_role_under_the_basic_user_is_refused_by_the_strict_load() {
    let user = "[[auth.basic.users]]\nuser = \"operator\"\npassword_hash_file = \"/run/secrets/operator\"\n";
    assert!(
        ferroterm_server::config::Config::from_sources(Some(user), &BTreeMap::new()).is_ok(),
        "the credential alone loads"
    );
    for key in ["scopes = [\"__ferroterm.admin\"]", "roles = [\"auditor\"]"] {
        let text = format!("{user}{key}\n");
        assert!(
            ferroterm_server::config::Config::from_sources(Some(&text), &BTreeMap::new()).is_err(),
            "{text}"
        );
    }
}
