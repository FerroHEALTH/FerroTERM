// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS 1.3 key exchange group a FerroTERM listener or client agrees on.
//!
//! [`KxPeer`] is a server that offers only the groups a test names, and
//! [`agreed_as_client`] a client that does the same, so a test shows which
//! group FerroTERM's side negotiates (RFC 10024, RFC 8446 §4.2.7). Each records
//! the group once the key exchange is done, so a handshake that then fails on
//! the certificate still shows the group it agreed on.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rcgen::{CertificateParams, KeyPair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, SupportedKxGroup, aws_lc_rs};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, NamedGroup, ServerConfig,
    ServerConnection, SignatureScheme,
};

/// How long a connection may take, and how long [`KxPeer::agreed`] waits for
/// one to be recorded.
const WAIT: Duration = Duration::from_secs(10);

/// Why a peer or a client could not be set up.
#[derive(Debug, thiserror::Error)]
pub enum KxError {
    /// The certificate could not be generated.
    #[error("the peer's certificate could not be generated")]
    Certificate(#[from] rcgen::Error),
    /// rustls refused the configuration.
    #[error("rustls refused the configuration")]
    Config(#[from] rustls::Error),
    /// The server name is not a DNS name.
    #[error("the server name is not a DNS name")]
    Name(#[from] rustls::pki_types::InvalidDnsNameError),
    /// A socket could not be bound or connected.
    #[error("a loopback socket could not be opened")]
    Io(#[from] std::io::Error),
}

/// A provider whose key exchange offers `groups`, in that order.
fn provider(groups: Vec<&'static dyn SupportedKxGroup>) -> Arc<CryptoProvider> {
    Arc::new(CryptoProvider {
        kx_groups: groups,
        ..aws_lc_rs::default_provider()
    })
}

/// A TLS 1.3 server on a loopback port that offers only the groups it was
/// started with, answers each completed handshake `204`, and records the
/// group of every connection.
#[derive(Debug)]
pub struct KxPeer {
    address: SocketAddr,
    agreed: Arc<Mutex<Vec<Option<NamedGroup>>>>,
}

impl KxPeer {
    /// Starts a peer that offers `groups`, in that order of preference, with a
    /// self-signed certificate for `localhost` that no root trusts.
    ///
    /// # Errors
    /// Returns [`KxError`] when the certificate, the configuration or the port
    /// cannot be made.
    pub fn start(groups: Vec<&'static dyn SupportedKxGroup>) -> Result<Self, KxError> {
        let key = KeyPair::generate()?;
        let certificate =
            CertificateParams::new(vec![String::from("localhost")])?.self_signed(&key)?;
        let config = Arc::new(
            ServerConfig::builder_with_provider(provider(groups))
                .with_protocol_versions(&[&rustls::version::TLS13])?
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(certificate.der().to_vec())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
                )?,
        );
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let agreed = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&agreed);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let group = serve(Arc::clone(&config), stream);
                record
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(group);
            }
        });
        Ok(Self { address, agreed })
    }

    /// The peer's address.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// The `https://localhost:<port>/` URL of the peer.
    #[must_use]
    pub fn url(&self) -> String {
        format!("https://localhost:{}/", self.address.port())
    }

    /// The group of the `index`th connection, `None` when it agreed on none,
    /// waiting until that connection is recorded.
    ///
    /// # Errors
    /// Returns the number of connections recorded when the `index`th was not
    /// recorded in time.
    pub fn agreed(&self, index: usize) -> Result<Option<NamedGroup>, usize> {
        let deadline = Instant::now() + WAIT;
        loop {
            let seen = self
                .agreed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some(group) = seen.get(index) {
                return Ok(*group);
            }
            if Instant::now() >= deadline {
                return Err(seen.len());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Drives one connection and returns the group it agreed on.
fn serve(config: Arc<ServerConfig>, mut stream: TcpStream) -> Option<NamedGroup> {
    let mut connection = ServerConnection::new(config).ok()?;
    stream.set_read_timeout(Some(WAIT)).ok()?;
    stream.set_write_timeout(Some(WAIT)).ok()?;
    let mut handshake = Ok((0, 0));
    while connection.is_handshaking() && handshake.is_ok() {
        handshake = connection.complete_io(&mut stream);
    }
    let group = connection
        .negotiated_key_exchange_group()
        .map(SupportedKxGroup::name);
    if handshake.is_ok() {
        let mut tls = rustls::Stream::new(&mut connection, &mut stream);
        let mut request = [0_u8; 4096];
        if tls.read(&mut request).is_ok()
            && tls
                .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
                .and_then(|()| tls.flush())
                .is_ok()
        {
            tls.conn.send_close_notify();
            let _closed = tls.conn.complete_io(tls.sock);
        }
    }
    group
}

/// The group a TLS 1.3 client offering `groups` agrees on with `address`.
///
/// The client offers the groups in that order and accepts any certificate,
/// since only the key exchange is under test; `None` means it agreed on none.
///
/// # Errors
/// Returns [`KxError`] when the configuration is refused or the address cannot
/// be reached.
pub fn agreed_as_client(
    address: SocketAddr,
    groups: Vec<&'static dyn SupportedKxGroup>,
) -> Result<Option<NamedGroup>, KxError> {
    let verifier = Arc::new(AnyCertificate(provider(Vec::new())));
    let config = ClientConfig::builder_with_provider(provider(groups))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let name = ServerName::try_from("localhost")?;
    let mut connection = ClientConnection::new(Arc::new(config), name)?;
    let mut socket = TcpStream::connect(address)?;
    socket.set_read_timeout(Some(WAIT))?;
    socket.set_write_timeout(Some(WAIT))?;
    while connection.is_handshaking() {
        connection.complete_io(&mut socket)?;
    }
    Ok(connection
        .negotiated_key_exchange_group()
        .map(SupportedKxGroup::name))
}

/// A verifier that accepts every certificate and checks the handshake
/// signatures with the provider's algorithms.
#[derive(Debug)]
struct AnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
