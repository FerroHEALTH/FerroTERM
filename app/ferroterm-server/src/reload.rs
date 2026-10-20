//! Reloading the served set: re-scan the configured directories, build a new
//! state, swap it in, and drop the old one when the last request on it ends.
//!
//! No FHIR specification governs a reload: our own design. The served set is
//! behind an [`Arc`], so a swap is a pointer exchange and a handler that took
//! the old state answers from it to the end. The triggers are `SIGHUP` and
//! `POST /reload` on the admin listener.

use std::sync::{Arc, Mutex, PoisonError, RwLock};

use axum::Router;
use axum::extract::{FromRef, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use http::StatusCode;

use crate::config::Config;
use crate::metrics::Outcome;
use crate::state::{AppState, LoadError};

/// The state every request resolves, and the configuration a reload reads
/// again.
///
/// Cloning is a pointer copy: every clone reads and swaps the one served set.
#[derive(Clone, Debug)]
pub struct Serving {
    inner: Arc<Inner>,
}

/// What the clones share.
#[derive(Debug)]
struct Inner {
    /// The set every new request takes, swapped whole by a reload.
    current: RwLock<Arc<AppState>>,
    /// What a reload reads again.
    config: Config,
    /// Held for the length of one rebuild, so two triggers arriving together
    /// open the artifacts once and swap once.
    rebuilding: Mutex<()>,
    /// The resource server, when the deployment authenticates its callers.
    smart: Option<Arc<crate::smart::Smart>>,
    /// The usage report window the request middleware records into; it
    /// outlives every swap of the served set.
    usage: Arc<crate::usage_report::window::Window>,
}

/// One code system version the served set carries.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Served {
    /// The `CodeSystem` instance id.
    pub id: String,
    /// The system URI.
    pub system: String,
    /// The version served.
    pub version: String,
}

/// A reload that did not happen; the old set still answers.
#[derive(Debug, thiserror::Error)]
pub enum ReloadError {
    /// A source of the new set does not load.
    #[error("the new set does not load")]
    Load(#[from] LoadError),
}

impl Serving {
    /// The set `state`, reloaded from `config` on request, authenticating no
    /// caller.
    #[must_use]
    pub fn new(config: Config, state: Arc<AppState>) -> Self {
        Self::with_smart(config, state, None)
    }

    /// The set `state`, guarded by `smart` when the deployment authenticates
    /// its callers.
    #[must_use]
    pub fn with_smart(
        config: Config,
        state: Arc<AppState>,
        smart: Option<Arc<crate::smart::Smart>>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                current: RwLock::new(state),
                config,
                rebuilding: Mutex::new(()),
                smart,
                usage: Arc::default(),
            }),
        }
    }

    /// The resource server, when the deployment authenticates its callers.
    #[must_use]
    pub fn smart(&self) -> Option<Arc<crate::smart::Smart>> {
        self.inner.smart.clone()
    }

    /// The usage report window of this process.
    #[must_use]
    pub fn usage_window(&self) -> Arc<crate::usage_report::window::Window> {
        Arc::clone(&self.inner.usage)
    }

    /// The set as of now.
    ///
    /// A caller holding the returned handle keeps that set for as long as it
    /// holds it, whatever a concurrent reload does.
    #[must_use]
    pub fn current(&self) -> Arc<AppState> {
        Arc::clone(
            &self
                .inner
                .current
                .read()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    /// The configuration a reload reads again.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Reads the configured directories again, builds a new set, and swaps it
    /// in; the systems it now serves come back.
    ///
    /// The call opens files and reads indexes, so an async caller runs it on a
    /// blocking task
    /// (<https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html>).
    ///
    /// # Errors
    ///
    /// Returns [`ReloadError`] when the new set does not build, in which case
    /// nothing is swapped and the old set keeps answering.
    pub fn reload(&self) -> Result<Vec<Served>, ReloadError> {
        let rebuilding = self
            .inner
            .rebuilding
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let current = self.current();
        let outcome = current.reloaded(&self.inner.config);
        let next = match outcome {
            Ok(next) => Arc::new(next),
            Err(error) => {
                current.metrics().reloaded(Outcome::Failed);
                tracing::error!(error = reason(&error), "the served set was not reloaded");
                drop(rebuilding);
                return Err(ReloadError::Load(error));
            }
        };
        let served: Vec<Served> = next
            .instances()
            .map(|(id, system, version)| Served {
                id: id.to_owned(),
                system: system.to_owned(),
                version: version.to_owned(),
            })
            .collect();
        *self
            .inner
            .current
            .write()
            .unwrap_or_else(PoisonError::into_inner) = next;
        current.metrics().reloaded(Outcome::Ok);
        tracing::info!(code_systems = served.len(), "the served set was reloaded");
        drop(rebuilding);
        Ok(served)
    }
}

impl FromRef<Serving> for Arc<AppState> {
    fn from_ref(serving: &Serving) -> Self {
        serving.current()
    }
}

impl FromRef<Serving> for Arc<crate::usage_report::window::Window> {
    fn from_ref(serving: &Serving) -> Self {
        serving.usage_window()
    }
}

impl FromRef<Serving> for Option<Arc<crate::smart::Smart>> {
    fn from_ref(serving: &Serving) -> Self {
        serving.smart()
    }
}

/// The admin application over `serving`: `POST /reload` and nothing else.
///
/// The FHIR listener never carries these routes, so a client of the
/// terminology API cannot reach them. Every route here requires the admin
/// scope `[auth]` names, unless the deployment turned authentication off.
pub fn router(serving: Serving) -> Router {
    let routes = Router::new()
        .route("/reload", post(reload))
        .fallback(unknown);
    let routes = match serving.smart() {
        Some(smart) => crate::smart::guard::admin(routes, &smart),
        None => routes,
    };
    routes.with_state(serving)
}

/// `POST /reload`: the systems now served on success, the reason on failure.
async fn reload(State(serving): State<Serving>) -> Response {
    let rebuilt = tokio::task::spawn_blocking(move || serving.reload()).await;
    match rebuilt {
        Ok(Ok(systems)) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({ "outcome": Outcome::Ok.as_str(), "systems": systems })),
        )
            .into_response(),
        Ok(Err(error)) => refused(&reason(&error)),
        Err(error) => {
            tracing::error!(%error, "the reload task did not finish");
            refused("the reload did not finish")
        }
    }
}

/// The answer to a reload that did not happen: the old set still answers.
fn refused(reason: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(serde_json::json!({ "outcome": Outcome::Failed.as_str(), "reason": reason })),
    )
        .into_response()
}

/// Anything else on the admin listener.
async fn unknown() -> Response {
    (StatusCode::NOT_FOUND, "ferroterm admin: POST /reload\n").into_response()
}

/// Reloads the served set on every `SIGHUP`, until the process ends.
///
/// An operator that has written a new artifact beside the running server sends
/// the signal and the server picks it up; a container that cannot send a signal
/// uses the admin listener instead.
pub async fn on_hangup(serving: Serving) {
    let mut signal = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::error!(%error, "cannot listen for SIGHUP");
            return;
        }
    };
    while signal.recv().await.is_some() {
        tracing::info!("SIGHUP received, reloading the served set");
        let serving = serving.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || serving.reload()).await {
            tracing::error!(%error, "the reload task did not finish");
        }
    }
}

/// An error and every cause under it, so one line names the file and the
/// reason it did not open.
fn reason(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::reason;

    #[derive(Debug, thiserror::Error)]
    #[error("the outer failure")]
    struct Outer(#[source] Inner);

    #[derive(Debug, thiserror::Error)]
    #[error("the inner failure")]
    struct Inner;

    #[test]
    fn a_reason_carries_every_cause_under_it() {
        assert_eq!(
            reason(&Outer(Inner)),
            "the outer failure: the inner failure"
        );
    }
}
