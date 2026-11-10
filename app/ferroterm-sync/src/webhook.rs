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

use ferrohealth_smart::client::outbound::{OutboundClient, OutboundError};
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::EndpointSection;

use crate::record::Summary;

/// Where one JSON summary per run is posted.
#[derive(Debug, Clone)]
pub struct Webhook {
    outbound: OutboundClient,
}

impl Webhook {
    /// The webhook `endpoint` names.
    ///
    /// # Errors
    ///
    /// Returns [`OutboundError`] when the table, its TLS or its credential
    /// is refused.
    pub fn new(endpoint: &EndpointSection) -> Result<Self, OutboundError> {
        Ok(Self {
            outbound: OutboundClient::build(endpoint, "webhook", &ServerTls)?,
        })
    }

    /// Posts `summary`, reporting a delivery that failed through `tracing`.
    pub async fn deliver(&self, summary: &Summary) {
        let sent = self
            .outbound
            .send(&reqwest::Method::POST, self.outbound.url(), |call| {
                call.json(summary)
            })
            .await;
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
