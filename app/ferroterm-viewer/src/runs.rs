// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The runs a reader has made, kept in this browser and nowhere else.
//!
//! A run is already a URL: every runner puts its parameters in the address, so
//! remembering one is remembering a link. That is what makes this list cheap,
//! and it is why nothing here holds an answer: the address is re-run when the
//! reader returns to it, so a remembered run can never show a stale answer
//! beside a live one.
//!
//! The list lives in the page for as long as it is open, and never in a URL
//! or the browser's storage: the console keeps nothing of a reader's in the
//! browser. The server is neither asked about it nor told about it.

/// How many runs are kept.
///
/// Enough to walk back through an afternoon's comparisons, short of a list a
/// reader has to read rather than glance at. The oldest falls off the end.
const CAPACITY: usize = 12;

/// One run a reader made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Run {
    /// The runner it was made on, as a reader reads it.
    pub(crate) screen: String,
    /// What it was about: the code, the canonical, the phrase.
    pub(crate) subject: String,
    /// The address that reproduces it.
    pub(crate) address: String,
}

/// The runs this page holds, newest first.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Runs(Vec<Run>);

impl Runs {
    /// This list with `run` at the front.
    ///
    /// A run already in the list moves to the front rather than appearing
    /// twice, so re-running one address does not fill the list with it.
    pub(crate) fn remembering(&self, run: &Run) -> Self {
        let mut kept = vec![run.clone()];
        kept.extend(
            self.0
                .iter()
                .filter(|held| held.address != run.address)
                .cloned(),
        );
        Self(kept).capped()
    }

    /// The runs, newest first.
    pub(crate) fn entries(&self) -> &[Run] {
        &self.0
    }

    /// Whether the list holds nothing.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// This list, trimmed to what is kept.
    fn capped(mut self) -> Self {
        self.0.truncate(CAPACITY);
        self
    }
}

/// The runs of the open page, shared by every runner through the context.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RunList(pub(crate) leptos::prelude::RwSignal<Runs>);

#[cfg(test)]
mod tests {
    use super::*;

    /// One run, named by its subject so a case reads.
    fn run(subject: &str) -> Run {
        Run {
            screen: "Expand".to_owned(),
            subject: subject.to_owned(),
            address: format!("/expand?fhir=r5&url={subject}"),
        }
    }

    #[test]
    fn the_newest_run_is_first() {
        let held = Runs::default()
            .remembering(&run("a"))
            .remembering(&run("b"));
        assert_eq!(
            held.entries()
                .iter()
                .map(|held| held.subject.as_str())
                .collect::<Vec<&str>>(),
            ["b", "a"],
            "a reader looks for what they just did"
        );
    }

    #[test]
    fn re_running_one_address_moves_it_rather_than_repeating_it() {
        let held = Runs::default()
            .remembering(&run("a"))
            .remembering(&run("b"))
            .remembering(&run("a"));
        assert_eq!(
            held.entries().len(),
            2,
            "the list holds one of each address"
        );
        assert_eq!(
            held.entries().first().map(|held| held.subject.as_str()),
            Some("a"),
            "the one just re-run is the one at the front"
        );
    }

    #[test]
    fn the_oldest_run_falls_off_the_end() {
        let held = (0..CAPACITY + 3).fold(Runs::default(), |held, index| {
            held.remembering(&run(&index.to_string()))
        });
        assert_eq!(held.entries().len(), CAPACITY);
        assert_eq!(
            held.entries().last().map(|held| held.subject.as_str()),
            Some("3"),
            "the three oldest are gone, and in the order they were made"
        );
    }
}
