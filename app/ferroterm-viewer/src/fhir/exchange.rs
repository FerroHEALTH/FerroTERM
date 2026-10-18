// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One HTTP exchange with the terminology server, the same value on both
//! halves of the console.
//!
//! Every request the console makes is written as an [`Exchange`] by
//! `crate::fhir`. On the server half [`send`] hands it to the console's own
//! client of the FHIR API, which adds the signed-in reader's access token; in
//! the browser it goes to the [`forward`] server function, which does the
//! same on the server, so no token ever reaches the browser. The answer comes
//! back as an [`Answer`] holding the status, the three headers a write reads,
//! and the body as text, which `crate::fhir` reads into its own types on
//! whichever half asked. No specification governs this: our own design.

use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

/// The HTTP method of an exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    /// `GET`, a read.
    Get,
    /// `POST`, an operation or a create.
    Post,
    /// `PUT`, an update.
    Put,
    /// `DELETE`, a delete.
    Delete,
}

impl Method {
    /// The method as the request line writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// One request to the terminology server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exchange {
    /// The method.
    pub method: Method,
    /// The path and query below the server's root, starting with `/`.
    pub path: String,
    /// The media type the request asks for.
    pub accept: String,
    /// The media type of the body, when there is one.
    pub content_type: Option<String>,
    /// The version a write replaces, as `If-Match` carries it.
    pub if_match: Option<String>,
    /// The body, when there is one.
    pub body: Option<String>,
}

impl Exchange {
    /// A request of `method` to `path` asking for `accept`, with no body.
    #[must_use]
    pub fn new(method: Method, path: String, accept: &str) -> Self {
        Self {
            method,
            path,
            accept: accept.to_owned(),
            content_type: None,
            if_match: None,
            body: None,
        }
    }

    /// The same request carrying `body` of the media type `content_type`.
    #[must_use]
    pub fn with_body(self, content_type: &str, body: String) -> Self {
        Self {
            content_type: Some(content_type.to_owned()),
            body: Some(body),
            ..self
        }
    }

    /// The same request replacing the version `if_match` names.
    #[must_use]
    pub fn with_if_match(self, if_match: Option<String>) -> Self {
        Self { if_match, ..self }
    }
}

/// What the terminology server answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The `ETag` header, when the server sent one.
    pub etag: Option<String>,
    /// The `Location` header, else `Content-Location`, when it sent one.
    pub location: Option<String>,
    /// The body, as text.
    pub body: String,
}

/// Why an exchange produced no answer.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ExchangeError {
    /// The path is not one the console forwards: it names none of the
    /// server's FHIR roots, its health route, or the sync service's runs.
    #[error("the console forwards no request to this path")]
    Forbidden,
    /// The console has no sync service configured to forward the runs to.
    #[error("the console reaches no sync service")]
    NoSync,
    /// The server could not be reached, or its answer could not be read.
    #[error("the terminology server could not be reached: {message}")]
    Unreachable {
        /// What the HTTP client reported.
        message: String,
    },
    /// The console's session store could not be read.
    #[error("the console cannot read its sessions")]
    Sessions,
    /// The console could not serve the request.
    #[error("the console could not serve this request")]
    Unavailable,
    /// The request could not reach the console's server.
    #[error("the console could not be reached: {message}")]
    Fetch {
        /// What the server function transport reported.
        message: String,
    },
}

impl FromServerFnError for ExchangeError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Fetch {
            message: value.to_string(),
        }
    }
}

/// Sends `exchange` to the terminology server from whichever half runs it.
///
/// # Errors
/// Returns the [`ExchangeError`] of an exchange that produced no answer; an
/// answer of any status is `Ok`.
pub async fn send(exchange: Exchange) -> Result<Answer, ExchangeError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::upstream::forward(exchange).await
    }
    #[cfg(target_arch = "wasm32")]
    {
        forward(exchange).await
    }
}

/// Forwards `exchange` to the terminology server as the reader whose session
/// the request carries, and answers what it answered.
///
/// The server function is a public endpoint of the console, so it reads the
/// session itself and forwards only paths below the server's FHIR roots, its
/// health route, and the sync service's runs (the Leptos book,
/// `server/25_server_functions`).
///
/// # Errors
/// As [`send`].
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        clippy::unused_async,
        reason = "the server_fn #[server] macro copies this onto the items it writes (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
#[server(endpoint = "fhir")]
pub async fn forward(
    /// The request to forward.
    exchange: Exchange,
) -> Result<Answer, ExchangeError> {
    crate::upstream::forward(exchange).await
}
