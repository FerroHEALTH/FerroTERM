// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The key exchange of the console's clients of the server, its sync service
//! and its admin listener: each prefers X25519MLKEM768 (RFC 10024) and agrees
//! on X25519 with a peer that lacks it. The peer's certificate is trusted by
//! no root, so each call ends after the key exchange.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use ferroterm_console::config::Config;
use ferroterm_console::server::upstream_client;
use rcgen::{CertificateParams, KeyPair};
use rustls::crypto::aws_lc_rs::{self, kx_group};
use rustls::crypto::{CryptoProvider, SupportedKxGroup};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{NamedGroup, ServerConfig, ServerConnection};

/// A TLS 1.3 peer on a loopback port that offers `groups` and returns the
/// group its one connection agreed on.
fn peer(
    groups: Vec<&'static dyn SupportedKxGroup>,
) -> (SocketAddr, JoinHandle<Option<NamedGroup>>) {
    let key = KeyPair::generate().expect("a key");
    let certificate = CertificateParams::new(vec![String::from("localhost")])
        .expect("the parameters")
        .self_signed(&key)
        .expect("a certificate");
    let provider = CryptoProvider {
        kx_groups: groups,
        ..aws_lc_rs::default_provider()
    };
    let config = Arc::new(
        ServerConfig::builder_with_provider(Arc::new(provider))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("TLS 1.3")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(certificate.der().to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
            )
            .expect("the certificate"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = listener.local_addr().expect("an address");
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .ok()?;
        let mut connection = ServerConnection::new(config).ok()?;
        while connection.is_handshaking() && connection.complete_io(&mut stream).is_ok() {}
        connection
            .negotiated_key_exchange_group()
            .map(SupportedKxGroup::name)
    });
    (address, handle)
}

/// The groups the server, sync and admin clients agree on with peers that
/// offer `groups`.
async fn agreed(groups: &[&'static dyn SupportedKxGroup]) -> Vec<Option<NamedGroup>> {
    let peers: Vec<_> = (0..3).map(|_| peer(groups.to_vec())).collect();
    let mut text = String::new();
    for ((address, _), table) in peers.iter().zip(["server", "sync", "admin"]) {
        writeln!(
            text,
            "[ferroterm.{table}]\nurl = \"https://localhost:{}/\"",
            address.port()
        )
        .expect("a string takes the table");
    }
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())
        .expect("a config")
        .resolve()
        .expect("the settings resolve");
    let upstream = upstream_client(&settings).expect("the clients build");
    let clients = [Some(upstream.server), upstream.sync, upstream.admin];
    let mut agreed = Vec::new();
    for (client, (_, handle)) in clients.into_iter().zip(peers) {
        let client = client.expect("the table is configured");
        let sent = client.http().get(client.url().clone()).send().await;
        assert!(
            sent.is_err(),
            "the peer's certificate is trusted by no root"
        );
        agreed.push(handle.join().expect("the peer thread"));
    }
    agreed
}

#[tokio::test]
async fn the_console_clients_prefer_x25519mlkem768() {
    assert_eq!(
        vec![Some(NamedGroup::X25519MLKEM768); 3],
        agreed(&[kx_group::X25519, kx_group::X25519MLKEM768]).await
    );
}

#[tokio::test]
async fn the_console_clients_agree_on_x25519_with_a_classical_only_peer() {
    assert_eq!(
        vec![Some(NamedGroup::X25519); 3],
        agreed(&[kx_group::X25519]).await
    );
}
