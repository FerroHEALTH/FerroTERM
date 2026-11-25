// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The key exchange of the outbound clients the sync service builds: each
//! prefers X25519MLKEM768 (RFC 10024) and agrees on X25519 with a peer that
//! lacks it. The peer's certificate is trusted by no root, so each call ends
//! after the key exchange.

use ferroterm_sync::record::{Outcome, Summary};
use ferroterm_sync::reload::AdminClient;
use ferroterm_sync::webhook::Webhook;
use ferroterm_testkit::kx::KxPeer;
use rustls::NamedGroup;
use rustls::crypto::SupportedKxGroup;
use rustls::crypto::aws_lc_rs::kx_group;

use crate::support::endpoint;

/// A synthetic run summary for the webhook to carry.
fn summary() -> Summary {
    Summary {
        id: String::from("run-1"),
        trigger: String::from("manual"),
        outcome: Outcome::Ok,
        started: jiff::Timestamp::UNIX_EPOCH,
        finished: jiff::Timestamp::UNIX_EPOCH,
        duration_ms: 0,
        entries_taken: 0,
        entries_skipped: 0,
        entries_not_subscribed: 0,
        activated: 0,
        bytes_staged: 0,
        bytes_used: 0,
        revalidation_findings: 0,
        revalidation: String::new(),
        errors: Vec::new(),
    }
}

/// The groups the admin client and the webhook agree on with a peer that
/// offers `groups`.
async fn agreed(groups: Vec<&'static dyn SupportedKxGroup>) -> [Option<NamedGroup>; 2] {
    let admin_peer = KxPeer::start(groups.clone()).expect("the peer starts");
    let admin = AdminClient::new(&endpoint(&admin_peer.url())).expect("the client builds");
    assert!(
        admin.reload().await.is_err(),
        "the peer's certificate is trusted by no root"
    );
    let webhook_peer = KxPeer::start(groups).expect("the peer starts");
    Webhook::new(&endpoint(&webhook_peer.url()))
        .expect("the client builds")
        .deliver(&summary())
        .await;
    [
        admin_peer.agreed(0).expect("the admin call is recorded"),
        webhook_peer
            .agreed(0)
            .expect("the webhook call is recorded"),
    ]
}

#[tokio::test]
async fn the_sync_clients_prefer_x25519mlkem768() {
    assert_eq!(
        [Some(NamedGroup::X25519MLKEM768); 2],
        agreed(vec![kx_group::X25519, kx_group::X25519MLKEM768]).await
    );
}

#[tokio::test]
async fn the_sync_clients_agree_on_x25519_with_a_classical_only_peer() {
    assert_eq!(
        [Some(NamedGroup::X25519); 2],
        agreed(vec![kx_group::X25519]).await
    );
}
