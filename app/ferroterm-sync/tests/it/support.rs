// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness every case runs on: three mock servers, a fake build binary,
//! and a clock the test drives.
//!
//! Nothing here reaches the network or performs a real build. The feed, the
//! server's admin listener, and the webhook are `wiremock` servers, the
//! offline build is a shell script the harness writes, and time moves only
//! when a test says so.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ferroterm_sync::clock::Clock;
use ferroterm_sync::config::{Activation, Config, ScheduleConfig};
use ferroterm_sync::run::Service;
use ferroterm_sync::source::{ConfiguredSource, Corrected, FixupError, Fixups};
use sha2::Digest as _;
use terminology_syndication::model::CategoryTerm;
use terminology_syndication::select::Subscription;
use terminology_syndication::source::{Authorization, BoxFuture, Source, SourceError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A clock a test drives, with a bounded number of waits.
///
/// Each wait moves the clock to the deadline and returns. Once the budget is
/// spent the wait never completes, which parks a scheduler loop instead of
/// spinning it.
#[derive(Debug)]
pub(crate) struct TestClock {
    now: Mutex<jiff::Timestamp>,
    waits: Mutex<usize>,
}

impl TestClock {
    /// A clock at `now` that will perform `waits` waits.
    pub(crate) fn new(now: &str, waits: usize) -> Arc<Self> {
        let now = now.parse().expect("the test clock starts at a timestamp");
        Arc::new(Self {
            now: Mutex::new(now),
            waits: Mutex::new(waits),
        })
    }
}

impl Clock for TestClock {
    fn now(&self) -> jiff::Timestamp {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn sleep_until(&self, deadline: jiff::Timestamp) -> BoxFuture<'_, ()> {
        let mut waits = self.waits.lock().unwrap_or_else(PoisonError::into_inner);
        let spent = *waits == 0;
        if !spent {
            *waits -= 1;
            *self.now.lock().unwrap_or_else(PoisonError::into_inner) = deadline;
        }
        drop(waits);
        Box::pin(async move {
            if spent {
                core::future::pending::<()>().await;
            }
        })
    }
}

/// A source pointed at the harness's feed, authenticating nothing.
#[derive(Debug)]
pub(crate) struct ReferenceSource {
    name: String,
    feed_url: String,
    fhir_api_url: Option<String>,
    client: reqwest::Client,
    yields: Vec<CategoryTerm>,
}

impl ReferenceSource {
    /// The source reading the feed at `feed_url`.
    pub(crate) fn new(feed_url: &str) -> Self {
        Self {
            name: String::from("reference"),
            feed_url: feed_url.to_owned(),
            fhir_api_url: None,
            client: reqwest::Client::new(),
            yields: CategoryTerm::NAMED.to_vec(),
        }
    }

    /// The same source also listing the FHIR API at `fhir_api_url`.
    pub(crate) fn with_fhir_api(mut self, fhir_api_url: &str) -> Self {
        self.fhir_api_url = Some(fhir_api_url.to_owned());
        self
    }
}

impl Source for ReferenceSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn feed_url(&self) -> &str {
        &self.feed_url
    }

    fn fhir_api_url(&self) -> Option<&str> {
        self.fhir_api_url.as_deref()
    }

    fn yields(&self) -> &[CategoryTerm] {
        &self.yields
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async { Ok(Authorization::Open) })
    }

    fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async { Ok(Authorization::Open) })
    }
}

/// A correction that rewrites one word, so a lane can be seen applying it.
#[derive(Debug)]
pub(crate) struct WordFixup;

impl Fixups for WordFixup {
    fn apply(&self, bytes: &[u8]) -> Result<Corrected, FixupError> {
        let text =
            String::from_utf8(bytes.to_vec()).map_err(|error| FixupError::new(Box::new(error)))?;
        if !text.contains("\"experimental\":\"true\"") {
            return Ok(Corrected {
                bytes: bytes.to_vec(),
                applied: Vec::new(),
            });
        }
        Ok(Corrected {
            bytes: text
                .replace("\"experimental\":\"true\"", "\"experimental\":true")
                .into_bytes(),
            applied: vec![String::from(
                "/experimental: the string `true` became a boolean",
            )],
        })
    }
}

/// One entry the harness publishes, and the bytes behind it.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// The entry identifier.
    pub(crate) id: String,
    /// The entry title.
    pub(crate) title: String,
    /// The category term, as the feed writes it.
    pub(crate) term: String,
    /// The canonical identifier of the content item.
    pub(crate) canonical: String,
    /// The version identifier of the content item.
    pub(crate) version: String,
    /// The entry date the replace rule compares.
    pub(crate) updated: String,
    /// The path the content item is served under.
    pub(crate) path: String,
    /// The bytes served.
    pub(crate) content: Vec<u8>,
}

impl Entry {
    /// An RF2 snapshot of `edition` released on `date`.
    pub(crate) fn rf2(edition: &str, date: &str, updated: &str) -> Self {
        Self {
            id: format!("urn:uuid:00000000-0000-4000-8000-{edition}{date}"),
            title: format!("Example edition {date} (RF2 SNAPSHOT)"),
            term: String::from("SCT_RF2_SNAPSHOT"),
            canonical: format!("http://snomed.info/sct/{edition}"),
            version: format!("http://snomed.info/sct/{edition}/version/{date}"),
            updated: updated.to_owned(),
            path: format!("/content/edition-{edition}-{date}.zip"),
            content: format!("a release archive of {edition} at {date}").into_bytes(),
        }
    }

    /// A FHIR `ValueSet` at `canonical` and `version`.
    pub(crate) fn value_set(canonical: &str, version: &str, updated: &str, body: &str) -> Self {
        Self {
            id: format!("urn:uuid:00000000-0000-4000-9000-{version}"),
            title: format!("Example value set {version}"),
            term: String::from("FHIR_ValueSet"),
            canonical: canonical.to_owned(),
            version: version.to_owned(),
            updated: updated.to_owned(),
            path: format!("/content/value-set-{version}.json"),
            content: body.as_bytes().to_vec(),
        }
    }

    /// The Atom entry element, with the content link pointed at `base`.
    fn xml(&self, base: &str) -> String {
        let digest = hex(&sha2::Sha256::digest(&self.content));
        let length = self.content.len();
        let Self {
            id,
            title,
            term,
            canonical,
            version,
            updated,
            path,
            ..
        } = self;
        format!(
            r#"  <entry>
    <title>{title}</title>
    <link rel="alternate" type="application/octet-stream" href="{base}{path}" length="{length}" ncts:sha256Hash="{digest}"/>
    <category term="{term}" scheme="http://ns.electronichealth.net.au/ncts/syndication/asf/scheme/1.0.0"/>
    <id>{id}</id>
    <updated>{updated}</updated>
    <ncts:contentItemIdentifier>{canonical}</ncts:contentItemIdentifier>
    <ncts:contentItemVersion>{version}</ncts:contentItemVersion>
  </entry>
"#
        )
    }
}

/// The lowercase hexadecimal form of a digest.
fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("writing to a string cannot fail");
    }
    out
}

/// The three servers, the directories, and the configuration of one case.
#[derive(Debug)]
pub(crate) struct Harness {
    /// The directory every path in the configuration sits under.
    pub(crate) dir: tempfile::TempDir,
    /// The syndication service.
    pub(crate) feed: MockServer,
    /// The FerroTERM server's admin listener.
    pub(crate) server: MockServer,
    /// The address one summary per run is posted to.
    pub(crate) webhook: MockServer,
    /// The configuration the service runs under.
    pub(crate) config: Config,
}

impl Harness {
    /// A harness whose directories are empty and whose feed offers nothing.
    pub(crate) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let feed = MockServer::start().await;
        let server = MockServer::start().await;
        let webhook = MockServer::start().await;
        let root = dir.path();
        for name in ["index", "codesystems", "staging", "records", "state"] {
            std::fs::create_dir_all(root.join(name)).expect("the service directories");
        }
        let config = Config {
            server_admin_url: server.uri(),
            index_root: root.join("index"),
            resources: root.join("codesystems"),
            staging: root.join("staging"),
            records: root.join("records"),
            state: root.join("state"),
            retention: 2,
            activation: Activation::Auto,
            webhook_url: Some(format!("{}/hook", webhook.uri())),
            build_command: build_script(root),
            ..Config::default()
        };
        Self {
            dir,
            feed,
            server,
            webhook,
            config,
        }
    }

    /// The index root the server reads.
    pub(crate) fn index_root(&self) -> PathBuf {
        self.config.index_root.clone()
    }

    /// The managed resource directory the server reads.
    pub(crate) fn resources(&self) -> PathBuf {
        self.config.resources.clone()
    }

    /// Publishes `entries` as the feed, and serves the bytes behind each one.
    pub(crate) async fn publish(&self, entries: &[Entry]) {
        let base = self.feed.uri();
        let mut body = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:ncts="http://ns.electronichealth.net.au/ncts/syndication/asf/extensions/1.0.0">
  <title>The harness feed</title>
  <id>urn:uuid:00000000-0000-4000-8000-000000000000</id>
  <updated>2026-09-22T00:00:00Z</updated>
"#,
        );
        for entry in entries {
            body.push_str(&entry.xml(&base));
            Mock::given(method("GET"))
                .and(path(entry.path.clone()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(entry.content.clone()))
                .mount(&self.feed)
                .await;
        }
        body.push_str("</feed>\n");
        Mock::given(method("GET"))
            .and(path("/synd/syndication.xml"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&self.feed)
            .await;
    }

    /// Serves `resources` on the FHIR API beside the feed: one search page
    /// per type, and each resource by its id.
    ///
    /// A resource is a full FHIR JSON body; the search summary the harness
    /// answers carries its `id`, `url`, `version`, `content`, and
    /// `meta.lastUpdated`, which is what the listing reads.
    pub(crate) async fn publish_api(&self, resources: &[serde_json::Value]) {
        for kind in ["CodeSystem", "ValueSet", "ConceptMap"] {
            let entries: Vec<serde_json::Value> = resources
                .iter()
                .filter(|resource| {
                    resource
                        .get("resourceType")
                        .and_then(serde_json::Value::as_str)
                        == Some(kind)
                })
                .map(|resource| {
                    let mut summary = serde_json::Map::new();
                    for name in ["resourceType", "id", "url", "version", "content", "meta"] {
                        if let Some(value) = resource.get(name) {
                            summary.insert(name.to_owned(), value.clone());
                        }
                    }
                    serde_json::json!({"resource": summary})
                })
                .collect();
            Mock::given(method("GET"))
                .and(path(format!("/fhir/{kind}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "resourceType": "Bundle",
                    "type": "searchset",
                    "link": [{"relation": "self", "url": "self"}],
                    "entry": entries
                })))
                .mount(&self.feed)
                .await;
        }
        for resource in resources {
            let kind = resource["resourceType"].as_str().expect("a resource type");
            let id = resource["id"].as_str().expect("an id");
            Mock::given(method("GET"))
                .and(path(format!("/fhir/{kind}/{id}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(resource))
                .mount(&self.feed)
                .await;
        }
    }

    /// The address of the harness FHIR API.
    pub(crate) fn fhir_api_url(&self) -> String {
        format!("{}/fhir", self.feed.uri())
    }

    /// The source reading the feed and the FHIR API, under `subscription`.
    pub(crate) fn source_with_api(&self, subscription: Subscription) -> ConfiguredSource {
        ConfiguredSource::new(
            Box::new(ReferenceSource::new(&self.feed_url()).with_fhir_api(&self.fhir_api_url())),
            subscription,
        )
    }

    /// Answers every reload with `status`.
    pub(crate) async fn reloads_with(&self, status: u16) {
        Mock::given(method("POST"))
            .and(path("/reload"))
            .respond_with(
                ResponseTemplate::new(status).set_body_string(r#"{"outcome":"ok","systems":[]}"#),
            )
            .mount(&self.server)
            .await;
    }

    /// Accepts every webhook delivery.
    pub(crate) async fn accepts_webhooks(&self) {
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&self.webhook)
            .await;
    }

    /// The address of the harness feed.
    pub(crate) fn feed_url(&self) -> String {
        format!("{}/synd/syndication.xml", self.feed.uri())
    }

    /// The source every case reads, taking snapshots and FHIR resources.
    pub(crate) fn source(&self) -> ConfiguredSource {
        ConfiguredSource::new(
            Box::new(ReferenceSource::new(&self.feed_url())),
            subscription(),
        )
    }

    /// The same source with the correction the harness offers.
    pub(crate) fn source_with_fixups(&self) -> ConfiguredSource {
        self.source().with_fixups(Box::new(WordFixup))
    }

    /// The service this harness describes, on `clock`.
    pub(crate) fn service(
        &self,
        sources: Vec<ConfiguredSource>,
        clock: Arc<TestClock>,
    ) -> Arc<Service> {
        Arc::new(
            Service::new(self.config.clone(), sources, clock, telemetry())
                .expect("the service is configured"),
        )
    }

    /// How many summaries the webhook received.
    pub(crate) async fn webhook_deliveries(&self) -> usize {
        self.webhook
            .received_requests()
            .await
            .map_or(0, |requests| requests.len())
    }

    /// How many reloads the server was asked for.
    pub(crate) async fn reload_requests(&self) -> usize {
        self.server
            .received_requests()
            .await
            .map_or(0, |requests| requests.len())
    }
}

/// The subscription every case runs under: every system, both lanes.
pub(crate) fn subscription() -> Subscription {
    Subscription::new()
        .with_category(CategoryTerm::SnomedRf2Snapshot)
        .with_category(CategoryTerm::FhirValueSet)
        .with_category(CategoryTerm::FhirCodeSystem)
}

/// The schedule `every`, with no daily time.
pub(crate) fn every(interval: &str) -> ScheduleConfig {
    ScheduleConfig {
        every: Some(interval.to_owned()),
        at: None,
    }
}

/// Writes the fake offline build and answers where it is.
///
/// It takes the arguments `ferroterm-build` takes and writes the manifest an
/// artifact directory carries, reading the edition and the release date out of
/// the directory name the service chose. That is what makes the RF2 lane
/// exercisable without a release.
fn build_script(root: &Path) -> PathBuf {
    let path = root.join("ferroterm-build");
    std::fs::write(
        &path,
        r#"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    *) shift ;;
  esac
done
name=$(basename "$out")
edition=$(echo "$name" | cut -d- -f2)
date=$(echo "$name" | cut -d- -f3)
mkdir -p "$out"
printf 'an index of %s' "$name" > "$out/store.redb"
cat > "$out/manifest.json" <<EOF
{"manifest":1,
 "system":"http://snomed.info/sct",
 "edition":"http://snomed.info/sct/$edition",
 "version":"http://snomed.info/sct/$edition/version/$date",
 "releaseDate":"$date",
 "store":"store.redb"}
EOF
"#,
    )
    .expect("the fake build binary");
    executable(&path);
    path
}

/// Writes a failing offline build and answers where it is.
pub(crate) fn failing_build(root: &Path) -> PathBuf {
    let path = root.join("failing-build");
    std::fs::write(
        &path,
        "#!/bin/sh\necho 'the release is not readable' >&2\nexit 2\n",
    )
    .expect("the failing build binary");
    executable(&path);
    path
}

/// Makes `path` executable by its owner.
fn executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("the build binary is executable");
}

/// A telemetry of its own for one service, so its scrape counts only its
/// own runs.
fn telemetry() -> Arc<ferrohealth_telemetry::telemetry::Telemetry> {
    Arc::new(
        ferrohealth_telemetry::telemetry::Telemetry::new(
            &ferrohealth_telemetry::section::TelemetrySection::default(),
            ferrohealth_telemetry::telemetry::Service::new("ferroterm-sync", "test"),
        )
        .expect("the default telemetry builds"),
    )
}
