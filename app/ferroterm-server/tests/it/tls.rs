// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[server.tls]` through FerroTERM's own wiring: both listeners speak TLS
//! with the configured certificate, and `ferroterm healthcheck` probes over
//! TLS pinned to that certificate.

use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use ferrohealth_server::healthcheck::Outcome;
use ferrohealth_server::section::{ServerSection, TlsSection};
use ferrohealth_server::serve::{Drained, ServeError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::fixture::Server;

/// A self-signed certificate for the loopback and its key, written as PEM
/// under `dir`; generated per test, so no key is committed.
fn certificate(dir: &Path, name: &str) -> TlsSection {
    let generated =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned(), "127.0.0.1".to_owned()])
            .expect("a certificate is generated");
    let certificate_file = dir.join(format!("{name}.pem"));
    let key_file = dir.join(format!("{name}.key"));
    std::fs::write(&certificate_file, generated.cert.pem()).expect("the certificate is written");
    std::fs::write(&key_file, generated.signing_key.serialize_pem()).expect("the key is written");
    TlsSection::new(certificate_file, key_file)
}

/// The `[server]` section of a probe to `listen` over `tls`.
fn probing(listen: SocketAddr, tls: &TlsSection) -> ServerSection {
    ServerSection {
        listen,
        tls: Some(tls.clone()),
        ..ServerSection::default()
    }
}

/// The FHIR and admin listeners on ephemeral loopback ports, until `stop`.
struct Served {
    fhir: SocketAddr,
    admin: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<Drained, ServeError>>,
}

impl Served {
    async fn start(server: &Server) -> Result<Self, Box<dyn std::error::Error>> {
        let fhir = TcpListener::bind("127.0.0.1:0").await?;
        let admin = TcpListener::bind("127.0.0.1:0").await?;
        let (fhir_address, admin_address) = (fhir.local_addr()?, admin.local_addr()?);
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(ferroterm_server::serve_until(
            fhir,
            Some(admin),
            server.serving.clone(),
            async {
                let _stop_or_dropped = stopped.await;
            },
        ));
        Ok(Self {
            fhir: fhir_address,
            admin: admin_address,
            stop,
            task,
        })
    }

    async fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        self.stop
            .send(())
            .map_err(|()| "the server dropped its shutdown receiver")?;
        let drained = tokio::time::timeout(Duration::from_secs(15), self.task).await???;
        assert_eq!(Drained::Complete, drained, "the drain completes");
        Ok(())
    }
}

/// What a plain HTTP `GET /health/readiness` on `address` reads back before
/// the connection closes.
async fn plain_get(address: SocketAddr) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(address).await?;
    stream
        .write_all(
            b"GET /health/readiness HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await?;
    let mut answer = Vec::new();
    // A reset is as much a refusal as a close; only the bytes matter.
    let _read =
        tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut answer)).await?;
    Ok(answer)
}

#[tokio::test]
async fn both_listeners_answer_over_tls_and_refuse_plain_http()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let tls = certificate(dir.path(), "server");
    let server = Server::start_with_server(ServerSection {
        tls: Some(tls.clone()),
        ..ServerSection::default()
    });
    let running = Served::start(&server).await?;

    for (name, address) in [("FHIR", running.fhir), ("admin", running.admin)] {
        assert_eq!(
            Outcome::Healthy,
            ferroterm_server::healthcheck(&probing(address, &tls)).await,
            "the {name} listener answers readiness over TLS"
        );
        let plain =
            ferrohealth_server::healthcheck::check(address, "", Duration::from_secs(2)).await;
        assert!(
            !matches!(plain, Outcome::Healthy),
            "the {name} listener gives a plain HTTP probe no answer: {plain:?}"
        );
        let answer = plain_get(address).await?;
        assert!(
            !answer.starts_with(b"HTTP/"),
            "the {name} listener answers plain HTTP with no HTTP response: {:?}",
            String::from_utf8_lossy(&answer)
        );
    }

    running.stop().await
}

#[tokio::test]
async fn the_probe_is_pinned_to_the_configured_certificate()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let tls = certificate(dir.path(), "server");
    let other = certificate(dir.path(), "other");
    let server = Server::start_with_server(ServerSection {
        tls: Some(tls.clone()),
        ..ServerSection::default()
    });
    let running = Served::start(&server).await?;

    assert_eq!(
        Outcome::Healthy,
        ferroterm_server::healthcheck(&probing(running.fhir, &tls)).await,
        "a probe pinned to the server's certificate passes"
    );
    let refused = ferroterm_server::healthcheck(&probing(running.fhir, &other)).await;
    assert!(
        matches!(&refused, Outcome::Failed(reason) if reason.contains("TLS")),
        "a probe pinned to another certificate refuses the server's: {refused:?}"
    );
    let missing = TlsSection::new(dir.path().join("absent.pem"), dir.path().join("absent.key"));
    let unread = ferroterm_server::healthcheck(&probing(running.fhir, &missing)).await;
    assert!(
        matches!(&unread, Outcome::Failed(reason) if reason.starts_with("the TLS files do not read")),
        "a certificate file that does not read fails the probe: {unread:?}"
    );

    running.stop().await
}

#[tokio::test]
async fn the_subcommand_probes_over_the_tls_its_configuration_names()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let tls = certificate(dir.path(), "server");
    let other = certificate(dir.path(), "other");
    let server = Server::start_with_server(ServerSection {
        tls: Some(tls.clone()),
        ..ServerSection::default()
    });
    let running = Served::start(&server).await?;

    let probe = |name: &str, pinned: &TlsSection| -> Result<std::path::PathBuf, std::io::Error> {
        let file = dir.path().join(format!("{name}.toml"));
        let toml = format!(
            "[server]\nlisten = \"{}\"\n\n[server.tls]\ncertificate_file = {:?}\nkey_file = {:?}\n",
            running.fhir,
            pinned.certificate_file.display().to_string(),
            pinned.key_file.display().to_string(),
        );
        std::fs::write(&file, toml)?;
        Ok(file)
    };
    let pinned = probe("pinned", &tls)?;
    let mismatched = probe("mismatched", &other)?;

    let run = |config: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_ferroterm"))
                .arg("--config")
                .arg(config)
                .arg("healthcheck")
                .env_remove("FERROTERM_CONFIG")
                .env_remove("FERROTERM__SERVER__LISTEN")
                .output()
        })
    };
    let ready = run(pinned).await??;
    assert!(
        ready.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ready.stderr)
    );
    let refused = run(mismatched).await??;
    assert!(
        !refused.status.success(),
        "a probe pinned to another certificate is unhealthy"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.starts_with("ferroterm healthcheck: unhealthy: ") && stderr.contains("TLS"),
        "the reason names the TLS failure: {stderr}"
    );

    running.stop().await
}
