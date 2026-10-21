// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one JSON post per run end.
//!
//! The service posts [`crate::record::Summary`] to the configured address when
//! a run ends, whether it worked or not. A delivery that fails is logged and
//! never fails the run: the run record is the durable account, and the webhook
//! is the notification.
//!
//! No FHIR specification governs this: our own design.

use crate::record::Summary;

/// The address one JSON summary per run is posted to.
#[derive(Debug, Clone)]
pub struct Webhook {
    client: reqwest::Client,
    url: String,
}

impl Webhook {
    /// The webhook at `url`.
    #[must_use]
    pub fn new(client: reqwest::Client, url: &str) -> Self {
        Self {
            client,
            url: url.to_owned(),
        }
    }

    /// Posts `summary`, reporting a delivery that failed through `tracing`.
    pub async fn deliver(&self, summary: &Summary) {
        let sent = self.client.post(&self.url).json(summary).send().await;
        match sent {
            Ok(response) if response.status().is_success() => {
                tracing::debug!(run = %summary.id, "the run webhook was delivered");
            }
            Ok(response) => {
                tracing::warn!(
                    run = %summary.id,
                    status = %response.status(),
                    "the run webhook was refused"
                );
            }
            Err(error) => {
                tracing::warn!(run = %summary.id, %error, "the run webhook was not delivered");
            }
        }
    }
}
