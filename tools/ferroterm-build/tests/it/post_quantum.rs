// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The key exchange of the WHO ICD-API client the offline build reads an
//! ICD-11 deployment with: it prefers X25519MLKEM768 (RFC 10024) and agrees on
//! X25519 with a peer that lacks it. The peer's certificate is trusted by no
//! root, so the call ends after the key exchange.

use ferroterm_testkit::kx::KxPeer;
use rustls::NamedGroup;
use rustls::crypto::SupportedKxGroup;
use rustls::crypto::aws_lc_rs::kx_group;

/// The group the ICD-API client agrees on with a peer that offers `groups`.
fn agreed(groups: Vec<&'static dyn SupportedKxGroup>) -> Option<NamedGroup> {
    let peer = KxPeer::start(groups).expect("the peer starts");
    let client = icd11::api::Client::new(&peer.url(), "2026-01").expect("the client builds");
    assert!(
        client.get(&peer.url(), "en").is_err(),
        "the peer's certificate is trusted by no root"
    );
    peer.agreed(0).expect("the call is recorded")
}

#[test]
fn the_icd_api_client_prefers_x25519mlkem768() {
    assert_eq!(
        Some(NamedGroup::X25519MLKEM768),
        agreed(vec![kx_group::X25519, kx_group::X25519MLKEM768])
    );
}

#[test]
fn the_icd_api_client_agrees_on_x25519_with_a_classical_only_peer() {
    assert_eq!(Some(NamedGroup::X25519), agreed(vec![kx_group::X25519]));
}
