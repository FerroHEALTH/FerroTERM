// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A server state over the testkit's synthetic SNOMED edition, and request helpers.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrohealth_smart::config::{AuthSection, IssuerKey};
use ferrohealth_telemetry::section::TelemetrySection;
use ferrohealth_telemetry::telemetry::{Service, Telemetry};
use ferroterm_server::config::{AdminSection, Config, FhirSection, TerminologySection};
use ferroterm_server::reload::Serving;
use ferroterm_server::state::AppState;
use http::{Request, Response, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

/// The audience a test token names unless the test says otherwise.
pub(crate) const AUDIENCE: &str = "ferroterm";

/// What a test's resource server is configured with: the `[auth]` table and
/// the base URL.
///
/// There is no `Default`: `[auth]` without an issuer is a configuration the
/// server refuses to start a guarded surface on, so the issuer is named
/// through [`SmartSetup::for_issuer`] and the rest is spread over it.
#[derive(Debug)]
pub(crate) struct SmartSetup {
    /// The `[auth]` table.
    pub(crate) auth: AuthSection,
    /// `FERROTERM__FHIR__BASE_URL`, the address clients reach this server at.
    pub(crate) base_url: Option<String>,
}

impl SmartSetup {
    /// The `[auth]` table of `issuer` with the audience [`AUDIENCE`], and
    /// everything else at its default.
    pub(crate) fn for_issuer(issuer: &str) -> Self {
        Self {
            auth: AuthSection {
                issuer: IssuerKey::Flat(issuer.to_owned()),
                audience: vec![String::from(AUDIENCE)],
                ..AuthSection::default()
            },
            base_url: None,
        }
    }
}

/// The authority every test request names, which the server answers under
/// when the deployment declares no base URL of its own.
pub(crate) const AUTHORITY: &str = "ferroterm.test";

/// The FHIR base of `segment` on that authority.
pub(crate) fn base_of(segment: &str) -> String {
    format!("http://{AUTHORITY}/{segment}")
}

/// The edition in a temporary directory, loaded the way the binary loads it.
/// A telemetry of its own for one test server, so its scrape counts only
/// its own requests.
pub(crate) fn telemetry() -> Arc<Telemetry> {
    Arc::new(
        Telemetry::new(
            &TelemetrySection::default(),
            Service::new("ferroterm", "test"),
        )
        .expect("the default telemetry builds"),
    )
}

pub(crate) struct Server {
    _dir: Arc<tempfile::TempDir>,
    config: Config,
    pub(crate) serving: Serving,
}

impl Server {
    pub(crate) fn start() -> Self {
        Self::start_with(false, false)
    }

    /// The edition behind a proxy that reaches it at `base`.
    pub(crate) fn start_with_base_url(base: &str) -> Self {
        let mut server = Self::start_with(false, false);
        let config = Config {
            fhir: FhirSection {
                base_url: Some(base.to_owned()),
                ..FhirSection::default()
            },
            ..server.config.clone()
        };
        let state = Arc::new(AppState::load(&config).expect("loads"));
        server.serving = Serving::new(config.clone(), state, telemetry());
        server.config = config;
        server
    }

    /// The edition plus the testkit's `CodeSystem` and `ValueSet` resources.
    pub(crate) fn start_with_resources() -> Self {
        Self::start_with(true, false)
    }

    /// The edition, the testkit's resources, and a resource database, so the
    /// persisted-resource routes answer.
    pub(crate) fn start_persisting() -> Self {
        Self::start_with(true, true)
    }

    /// The edition, the testkit's `CodeSystem` resources, and the LOINC and
    /// `RxNorm` artifacts, so a test reaches a system of every loader beside
    /// the registry systems the binary always serves.
    pub(crate) fn start_with_every_loader() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let snomed = dir.path().join("snomed");
        let loinc = dir.path().join("loinc");
        let rxnorm = dir.path().join("rxnorm");
        let fhir = dir.path().join("fhir");
        for path in [&snomed, &loinc, &rxnorm, &fhir] {
            std::fs::create_dir_all(path).expect("creates");
        }
        ferroterm_testkit::snomed::write(&snomed).expect("writes the edition");
        ferroterm_testkit::loinc::write_artifact(&loinc).expect("builds loinc");
        ferroterm_testkit::rxnorm::write_artifact(&rxnorm).expect("builds rxnorm");
        ferroterm_testkit::fhir::write_code_systems(&fhir).expect("writes the resources");
        let config = Config {
            terminology: TerminologySection {
                index: vec![snomed, loinc, rxnorm],
                code_systems: vec![fhir],
                ..TerminologySection::default()
            },
            ..Config::default()
        };
        Self::assembled(Arc::new(dir), config)
    }

    /// A server over the resources the browser journeys are driven against,
    /// each of which carries the `id` it was authored with.
    pub(crate) fn start_with_authored_ids() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = Config {
            terminology: TerminologySection {
                code_systems: vec![authored_resources()],
                ..TerminologySection::default()
            },
            ..Config::default()
        };
        Self::assembled(Arc::new(dir), config)
    }

    /// The state `config` names over the resources a test wrote into `dir`,
    /// which stays alive for as long as the server does.
    pub(crate) fn start_with_config(dir: tempfile::TempDir, config: Config) -> Self {
        Self::assembled(Arc::new(dir), config)
    }

    /// The state `config` names, held the way the binary holds it.
    fn assembled(dir: Arc<tempfile::TempDir>, config: Config) -> Self {
        let state = Arc::new(AppState::load(&config).expect("loads"));
        Self {
            _dir: dir,
            serving: Serving::new(config.clone(), state, telemetry()),
            config,
        }
    }

    /// The set the server answers from now.
    pub(crate) fn state(&self) -> Arc<AppState> {
        self.serving.current()
    }

    /// The `CodeSystem` instance id the server addresses `url` by.
    pub(crate) fn instance_id_of(&self, url: &str) -> String {
        self.state()
            .instances()
            .find(|(_, served, _)| *served == url)
            .map_or_else(|| panic!("{url} is loaded"), |(id, _, _)| id.to_owned())
    }

    /// The `ValueSet` instance id the server addresses `url` by.
    pub(crate) fn value_set_id_of(&self, url: &str) -> String {
        self.state()
            .value_set_instances()
            .into_iter()
            .find(|(_, served, _)| served == url)
            .map_or_else(|| panic!("{url} is loaded"), |(id, _, _)| id)
    }

    /// The `ConceptMap` instance id the server addresses `url` by.
    pub(crate) fn concept_map_id_of(&self, url: &str) -> String {
        self.state()
            .concept_map_instances()
            .into_iter()
            .find(|(_, served, _)| served == url)
            .map_or_else(|| panic!("{url} is loaded"), |(id, _, _)| id)
    }

    /// The same configuration loaded again, as a restart loads it.
    ///
    /// The running server is dropped first: `redb` holds the database file for
    /// as long as its handle lives
    /// (<https://docs.rs/redb/latest/redb/struct.Database.html>).
    pub(crate) fn restarted(self) -> Self {
        let Self {
            _dir: dir,
            config,
            serving,
        } = self;
        drop(serving);
        Self::assembled(dir, config)
    }

    /// A server holding one archetype's local terminology and nothing else.
    ///
    /// The four resources are what a producer derives from an openEHR
    /// archetype, written into a directory the way any other supplied resource
    /// is, so the server reads them through the ordinary path.
    pub(crate) fn start_with_archetype_terminology() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let derived = dir.path().join("openehr");
        std::fs::create_dir_all(&derived).expect("creates");
        ferroterm_testkit::openehr::write_archetype_terminology(&derived)
            .expect("writes the archetype's terminology");
        // A second producer of the same archetype, under its own domain, so a
        // test can show the two canonicals do not collide.
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("creates");
        ferroterm_testkit::openehr::write_second_deployer(&elsewhere)
            .expect("writes the second deployer's resources");
        let config = Config {
            terminology: TerminologySection {
                code_systems: vec![derived, elsewhere],
                ..TerminologySection::default()
            },
            ..Config::default()
        };
        Self::assembled(Arc::new(dir), config)
    }

    /// The edition with a resource database, an admin listener and a resource
    /// server over both.
    ///
    /// The audience is [`AUDIENCE`] and the admin scope the default product
    /// scope when the test names none; everything else is what
    /// [`Server::start_persisting`] loads.
    pub(crate) async fn start_persisting_with_smart(
        issuer: &str,
        audience: Option<&str>,
        admin_scope: Option<&str>,
    ) -> Self {
        let mut setup = SmartSetup::for_issuer(issuer);
        if let Some(audience) = audience {
            setup.auth.audience = vec![audience.to_owned()];
        }
        if let Some(scope) = admin_scope {
            setup.auth.product_scope = scope.to_owned();
        }
        Self::start_persisting_with_smart_setup(setup).await
    }

    /// The configuration of that server under `setup`, over the edition
    /// written into `dir`.
    pub(crate) fn smart_config(dir: &std::path::Path, setup: SmartSetup) -> Config {
        let SmartSetup { auth, base_url } = setup;
        ferroterm_testkit::snomed::write(dir).expect("writes the edition");
        let fhir = dir.join("fhir");
        std::fs::create_dir_all(&fhir).expect("creates");
        ferroterm_testkit::fhir::write_code_systems(&fhir).expect("writes the resources");
        Config {
            terminology: TerminologySection {
                index: vec![dir.to_path_buf()],
                code_systems: vec![fhir],
                resources: Some(dir.join("resources.redb")),
                ..TerminologySection::default()
            },
            fhir: FhirSection {
                base_url,
                ..FhirSection::default()
            },
            // The listener is never bound here; naming it builds the admin gate.
            admin: AdminSection {
                listen: Some(String::from("127.0.0.1:0")),
            },
            auth,
            ..Config::default()
        }
    }

    /// The same server, configured by `setup`.
    pub(crate) async fn start_persisting_with_smart_setup(setup: SmartSetup) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = Self::smart_config(dir.path(), setup);
        let smart = ferroterm_server::smart::Smart::start(&config)
            .await
            .expect("the issuer answers")
            .map(Arc::new);
        let state = Arc::new(AppState::load(&config).expect("loads"));
        Self {
            _dir: Arc::new(dir),
            serving: Serving::with_smart(config.clone(), state, smart, telemetry()),
            config,
        }
    }

    /// The admin application, which the second listener serves.
    pub(crate) fn admin_router(&self) -> Router {
        ferroterm_server::reload::router(self.serving.clone())
    }

    fn start_with(resources: bool, persists: bool) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        ferroterm_testkit::snomed::write(dir.path()).expect("writes the edition");
        let fhir = dir.path().join("fhir");
        std::fs::create_dir_all(&fhir).expect("creates");
        ferroterm_testkit::fhir::write_code_systems(&fhir).expect("writes the resources");
        let config = Config {
            terminology: TerminologySection {
                index: vec![dir.path().to_path_buf()],
                code_systems: if resources { vec![fhir] } else { Vec::new() },
                resources: persists.then(|| dir.path().join("resources.redb")),
                ..TerminologySection::default()
            },
            ..Config::default()
        };
        Self::assembled(Arc::new(dir), config)
    }

    /// Any request, answered as the raw response so a test can read its headers.
    pub(crate) async fn send(&self, mut request: Request<Body>) -> Response<Body> {
        // An HTTP/1.1 request always names the authority it was sent to, and
        // the server builds a searchset's absolute `fullUrl` from it
        // (<https://www.rfc-editor.org/rfc/rfc9112.html#section-3.2>).
        request
            .headers_mut()
            .entry(http::header::HOST)
            .or_insert(http::HeaderValue::from_static(AUTHORITY));
        self.router().oneshot(request).await.expect("response")
    }

    /// A `PUT` of a FHIR JSON body, answered as the raw response.
    pub(crate) async fn put(&self, uri: &str, body: &Value) -> Response<Body> {
        let request = Request::put(uri)
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(body.to_string()))
            .expect("request");
        self.send(request).await
    }

    pub(crate) fn router(&self) -> Router {
        ferroterm_server::router(self.serving.clone())
    }

    /// The `CodeSystem` instance id of the synthetic edition.
    pub(crate) fn snomed_id(&self) -> String {
        self.state()
            .instances()
            .next()
            .map(|(id, _, _)| id.to_owned())
            .expect("one instance")
    }

    pub(crate) async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::get(uri).body(Body::empty()).expect("request");
        json(self.send(request).await).await
    }

    pub(crate) async fn get_with_header(
        &self,
        uri: &str,
        name: &str,
        value: &str,
    ) -> (StatusCode, Value) {
        let request = Request::get(uri)
            .header(name, value)
            .body(Body::empty())
            .expect("request");
        json(self.send(request).await).await
    }

    pub(crate) async fn post_with_header(
        &self,
        uri: &str,
        body: &Value,
        name: &str,
        value: &str,
    ) -> (StatusCode, Value) {
        let request = Request::post(uri)
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .header(name, value)
            .body(Body::from(body.to_string()))
            .expect("request");
        json(self.send(request).await).await
    }

    pub(crate) async fn post(&self, uri: &str, body: &Value) -> (StatusCode, Value) {
        let request = Request::post(uri)
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(body.to_string()))
            .expect("request");
        json(self.send(request).await).await
    }

    /// A `GET` with an optional `Accept`, answered as the status, the
    /// `Content-Type`, and the body text (for a response that is not FHIR JSON).
    pub(crate) async fn get_text(
        &self,
        uri: &str,
        accept: Option<&str>,
    ) -> (StatusCode, String, String) {
        let mut request = Request::get(uri);
        if let Some(accept) = accept {
            request = request.header(http::header::ACCEPT, accept);
        }
        let request = request.body(Body::empty()).expect("request");
        text(self.send(request).await).await
    }

    /// A `POST` of `body` with `content_type` and an optional `Accept`, answered
    /// as the status, the `Content-Type`, and the body text.
    pub(crate) async fn post_text(
        &self,
        uri: &str,
        content_type: &str,
        body: &str,
        accept: Option<&str>,
    ) -> (StatusCode, String, String) {
        let mut request = Request::post(uri).header(http::header::CONTENT_TYPE, content_type);
        if let Some(accept) = accept {
            request = request.header(http::header::ACCEPT, accept);
        }
        let request = request.body(Body::from(body.to_owned())).expect("request");
        text(self.send(request).await).await
    }

    pub(crate) async fn post_raw(
        &self,
        uri: &str,
        content_type: &str,
        body: &str,
    ) -> (StatusCode, Value) {
        let request = Request::post(uri)
            .header(http::header::CONTENT_TYPE, content_type)
            .body(Body::from(body.to_owned()))
            .expect("request");
        json(self.send(request).await).await
    }
}

pub(crate) async fn json(response: Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/fhir+json; charset=utf-8"),
        "every FHIR response is FHIR JSON"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    let value: Value = serde_json::from_slice(&bytes).expect("json body");
    (status, value)
}

/// The status, `Content-Type`, and text of any response.
pub(crate) async fn text(response: Response<Body>) -> (StatusCode, String, String) {
    let status = response.status();
    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).expect("utf-8"),
    )
}

/// The value of a named parameter of a `Parameters` body.
pub(crate) fn parameter<'a>(body: &'a Value, name: &str) -> Option<&'a Value> {
    body["parameter"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
}

/// A `Parameters` body from (name, value) pairs.
pub(crate) fn parameters(pairs: &[(&str, Value)]) -> Value {
    let list: Vec<Value> = pairs
        .iter()
        .map(|(name, value)| {
            let mut object = serde_json::Map::new();
            object.insert(String::from("name"), Value::String((*name).to_owned()));
            if let Value::Object(v) = value {
                for (k, val) in v {
                    object.insert(k.clone(), val.clone());
                }
            }
            Value::Object(object)
        })
        .collect();
    serde_json::json!({"resourceType": "Parameters", "parameter": list})
}

/// A response header as text.
pub(crate) fn header(response: &Response<Body>, name: http::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// A response body in the codec document model, ready to decode.
pub(crate) fn document(body: &Value) -> fhir_types::codec::Value {
    serde_json::from_str(&body.to_string()).expect("the body parses")
}

/// The directory of `CodeSystem`, `ValueSet`, and `ConceptMap` resources the
/// browser journeys load, each carrying the `id` it was authored with.
fn authored_resources() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../e2e/fixtures/codesystems")
        .canonicalize()
        .expect("the browser journeys' resources are in the repository")
}

/// A resource the codec wrote, as a `serde_json` document.
pub(crate) fn written(object: &fhir_types::codec::Object) -> Value {
    let text = serde_json::to_string(object).expect("the document writes");
    serde_json::from_str(&text).expect("the document parses")
}
