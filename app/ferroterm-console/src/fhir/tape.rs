// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The exchanges a page was drawn from, recorded where the page is read and
//! replayed where it is drawn.
//!
//! A page reads the terminology server through a run of exchanges, each
//! decided by the answer before it. The console's server runs that read
//! once, recording each exchange with its answer ([`Tape::recording`]), and
//! the recording crosses to the browser with the page. Drawing the page
//! again from it ([`Tape::replaying`]) asks the server for nothing, so the
//! page the browser hydrates or draws after a navigation is the page the
//! server read, from one round trip. An exchange the recording does not hold
//! goes to the server as it would with no recording. No specification
//! governs this: our own design.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::fhir::exchange::{self, Answer, Exchange, ExchangeError};

/// One exchange of a recording and what it answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    /// The request.
    pub(crate) exchange: Exchange,
    /// What it answered, or why it produced no answer.
    pub(crate) answer: Result<Answer, ExchangeError>,
}

/// The exchanges of one read, in the order they were sent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Recording(pub(crate) Vec<Entry>);

/// How a client sends its exchanges.
#[derive(Clone, Debug, Default)]
pub(crate) enum Tape {
    /// Each exchange goes to the server.
    #[default]
    Live,
    /// Each exchange goes to the server and is kept with its answer.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(
            dead_code,
            reason = "the server records a read; the browser only replays one"
        )
    )]
    Record(Arc<Mutex<Vec<Entry>>>),
    /// Each exchange is answered from a recording, and goes to the server
    /// only when the recording holds no such exchange.
    Replay(Arc<Mutex<Vec<Option<Entry>>>>),
}

impl Tape {
    /// A tape that keeps every exchange it sends.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(
            dead_code,
            reason = "the server records a read; the browser only replays one"
        )
    )]
    #[must_use]
    pub(crate) fn recording() -> Self {
        Self::Record(Arc::default())
    }

    /// A tape that answers from `recording`.
    #[must_use]
    pub(crate) fn replaying(recording: Recording) -> Self {
        Self::Replay(Arc::new(Mutex::new(
            recording.0.into_iter().map(Some).collect(),
        )))
    }

    /// What the tape has recorded so far; empty for a tape that records
    /// nothing.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(
            dead_code,
            reason = "the server records a read; the browser only replays one"
        )
    )]
    #[must_use]
    pub(crate) fn recorded(&self) -> Recording {
        match self {
            // NOTE: no specification governs this: our own design; a poisoned lock
            // crosses no recording, so the browser asks the server for each exchange.
            Self::Record(entries) => Recording(
                entries
                    .lock()
                    .map(|entries| entries.clone())
                    .unwrap_or_default(),
            ),
            Self::Live | Self::Replay(_) => Recording::default(),
        }
    }

    /// Sends `exchange`, or answers it from the recording.
    ///
    /// # Errors
    /// Returns the [`ExchangeError`] of an exchange that produced no answer,
    /// as [`exchange::send`] does.
    pub(crate) async fn send(&self, exchange: Exchange) -> Result<Answer, ExchangeError> {
        if let Self::Replay(entries) = self
            && let Some(answer) = taken(entries, &exchange)
        {
            return answer;
        }
        let answer = exchange::send(exchange.clone()).await;
        if let Self::Record(entries) = self
            && let Ok(mut entries) = entries.lock()
        {
            entries.push(Entry {
                exchange,
                answer: answer.clone(),
            });
        }
        answer
    }
}

/// Takes the first recorded answer to `exchange` out of `entries`, so a
/// read that sends the same exchange twice is answered in the recorded
/// order.
fn taken(
    entries: &Mutex<Vec<Option<Entry>>>,
    exchange: &Exchange,
) -> Option<Result<Answer, ExchangeError>> {
    // NOTE: no specification governs this: our own design; a poisoned lock answers
    // nothing from the recording, and the exchange goes to the server instead.
    let mut entries = entries.lock().ok()?;
    entries
        .iter_mut()
        .find(|entry| {
            entry
                .as_ref()
                .is_some_and(|entry| entry.exchange == *exchange)
        })
        .and_then(Option::take)
        .map(|entry| entry.answer)
}

#[cfg(test)]
mod tests {
    use super::{Entry, Recording, Tape, taken};
    use crate::fhir::exchange::{Answer, Exchange, ExchangeError, Method};

    fn read(path: &str) -> Exchange {
        Exchange::new(Method::Get, path.to_owned(), "application/fhir+json")
    }

    fn answered(body: &str) -> Answer {
        Answer {
            status: 200,
            etag: None,
            location: None,
            body: body.to_owned(),
        }
    }

    #[test]
    fn a_replay_answers_each_exchange_once_in_the_recorded_order() {
        let tape = Tape::replaying(Recording(vec![
            Entry {
                exchange: read("/r4/metadata"),
                answer: Ok(answered("first")),
            },
            Entry {
                exchange: read("/r4/CodeSystem"),
                answer: Ok(answered("search")),
            },
            Entry {
                exchange: read("/r4/metadata"),
                answer: Ok(answered("second")),
            },
        ]));
        let Tape::Replay(entries) = &tape else {
            panic!("a replaying tape replays");
        };
        assert_eq!(
            Some(Ok(answered("first"))),
            taken(entries, &read("/r4/metadata"))
        );
        assert_eq!(
            Some(Ok(answered("second"))),
            taken(entries, &read("/r4/metadata"))
        );
        assert_eq!(
            None,
            taken(entries, &read("/r4/metadata")),
            "both are spent"
        );
        assert_eq!(
            None,
            taken(entries, &read("/r5/metadata")),
            "never recorded"
        );
        assert_eq!(
            Some(Ok(answered("search"))),
            taken(entries, &read("/r4/CodeSystem"))
        );
    }

    #[test]
    fn a_recording_crosses_as_json_and_reads_back_whole() {
        let recording = Recording(vec![Entry {
            exchange: read("/r4/metadata"),
            answer: Err(ExchangeError::Unreachable {
                message: String::from("refused"),
            }),
        }]);
        let text = serde_json::to_string(&recording).expect("a recording serializes");
        let back: Recording = serde_json::from_str(&text).expect("and reads back");
        assert_eq!(recording, back);
        assert_eq!(Recording::default(), Tape::Live.recorded());
    }
}
