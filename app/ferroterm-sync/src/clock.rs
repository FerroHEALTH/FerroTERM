// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the service reads the time and where it waits.
//!
//! Every time the service reads and every wait it performs goes through
//! [`Clock`], so a test drives a whole schedule without sleeping. The running
//! service uses [`SystemClock`], which reads `jiff` and waits on the tokio
//! timer.

use core::fmt;

use terminology_syndication::source::BoxFuture;

/// The time the service reads, and the wait it performs.
pub trait Clock: fmt::Debug + Send + Sync {
    /// The instant now.
    fn now(&self) -> jiff::Timestamp;

    /// Waits until `deadline`, returning at once when it has passed.
    fn sleep_until(&self, deadline: jiff::Timestamp) -> BoxFuture<'_, ()>;
}

/// The clock of a running service: `jiff` for the time, tokio for the wait.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::now()
    }

    fn sleep_until(&self, deadline: jiff::Timestamp) -> BoxFuture<'_, ()> {
        let waiting = deadline.duration_since(self.now());
        Box::pin(async move {
            if waiting.is_positive() {
                tokio::time::sleep(waiting.unsigned_abs()).await;
            }
        })
    }
}
