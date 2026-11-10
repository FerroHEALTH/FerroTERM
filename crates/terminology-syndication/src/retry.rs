// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Trying a request again when the service could not be reached or failed.
//!
//! A request is sent again after each wait in turn while the service could
//! not be reached, the try timed out, or it answered a `5xx`; any other
//! answer is the caller's. The rule is the family outbound client's
//! (`ferrohealth_smart::client::outbound`), applied here because the shared
//! requests of a [`crate::source::Source`] are made in this crate. No
//! specification governs it: our own design.

use std::time::Duration;

/// Sends `request`, and sends it again after each of `delays` while the try
/// failed to reach the service or met a `5xx`.
///
/// A request whose body cannot be cloned is sent once.
///
/// # Errors
///
/// Returns the transport error of the last try.
pub async fn send(
    request: reqwest::RequestBuilder,
    delays: &[Duration],
) -> Result<reqwest::Response, reqwest::Error> {
    let mut waits = delays.iter();
    loop {
        let Some(again) = request.try_clone() else {
            return request.send().await;
        };
        let outcome = again.send().await;
        let failed = match &outcome {
            Ok(response) => response.status().is_server_error(),
            Err(_) => true,
        };
        match waits.next() {
            Some(&delay) if failed => {
                tracing::warn!(
                    delay_ms = delay.as_millis(),
                    "retrying a request to a syndication source"
                );
                tokio::time::sleep(delay).await;
            }
            _ => return outcome,
        }
    }
}
