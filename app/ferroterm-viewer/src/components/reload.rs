//! What an authoring screen does with a refused write: the sentence its live
//! region announces, and the way back after a `412`.
//!
//! Every screen that saves states the version it replaces in `If-Match`, so a
//! change made elsewhere since the form was read is refused with `412` rather
//! than overwritten (<https://hl7.org/fhir/R4B/http.html#concurrency>). The
//! refused content stays in the form, and one control, drawn the same on every
//! screen, reads the version the server holds now.

use std::sync::Arc;

use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::fhir::error::FhirError;
use crate::fhir::outcome::OperationOutcome;
use crate::fhir::write::Refusal;
use crate::styles;

/// The words on the control, the same on every screen that offers it.
pub(crate) const RELOAD_LABEL: &str = "Reload the current version";

/// What a live region that outlives the reload says while it reads.
pub(crate) const READING_AGAIN: &str = "Reading the current version from the server.";

/// A navigation, boxed so a `Copy` handle can hold it.
type Navigate = Arc<dyn Fn(&str, NavigateOptions) + Send + Sync>;

/// What the live region says when a write was refused.
///
/// The sentence the viewer adds says what the reader can do; the rest is the
/// server's own wording, so what is announced is never a paraphrase of it.
pub(crate) fn announced(error: &FhirError) -> String {
    let said = diagnostics(error);
    let what_to_do = Refusal::of(error).what_to_do();
    if said.is_empty() {
        what_to_do.to_owned()
    } else {
        format!("{what_to_do} {said}")
    }
}

/// The server's own wording for a refusal, every issue's text in order.
pub(crate) fn diagnostics(error: &FhirError) -> String {
    error
        .outcome()
        .map(OperationOutcome::lines)
        .unwrap_or_default()
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<String>>()
        .join(" ")
}

/// Whether `refusal` holds a `412`, the one refusal a reload answers.
fn concurrent(refusal: RwSignal<Option<FhirError>>) -> bool {
    refusal.with(|held| {
        held.as_ref()
            .is_some_and(|error| Refusal::of(error) == Refusal::ConcurrentEdit)
    })
}

/// The control that reads the current version, drawn only after a `412`.
///
/// `press` does the reading. The control clears the refusal it answers, so the
/// outcome it rendered does not outlive the version it was about.
pub(crate) fn reload_offer(
    refusal: RwSignal<Option<FhirError>>,
    press: Arc<dyn Fn() + Send + Sync>,
) -> AnyView {
    let press = StoredValue::new(press);
    view! {
        <Show when=move || concurrent(refusal) fallback=|| ()>
            <p>
                <button
                    type="button"
                    class=styles::BUTTON
                    on:click=move |_| {
                        refusal.set(None);
                        press.with_value(|press| press());
                    }
                >
                    {RELOAD_LABEL}
                </button>
            </p>
        </Show>
    }
    .into_any()
}

/// Moves the keyboard to the live region `id` names.
///
/// The control that asked for the reload is gone once the refusal clears, so
/// the focus goes where the answer is announced rather than back to the page.
pub(crate) fn focus_report(id: &str) {
    crate::components::focus(id);
}

/// What the live region of a form rebuilt by a reload says.
pub(crate) fn reloaded_text(version_id: &str) -> String {
    if version_id.is_empty() {
        String::from("Reloaded. The server stated no version for it.")
    } else {
        format!("Reloaded. The form now shows version {version_id}, the one the server holds.")
    }
}

/// A reload of a screen that opens its resource by one query parameter.
///
/// The form is rebuilt from what the read answers, live region included, so
/// `asked` tells the new form it is the answer to a reload. A resource created
/// on the screen was never named in the address, so the first reload names it
/// there, which is also what makes the address say what the screen shows.
#[derive(Clone, Copy)]
pub(crate) struct Reload {
    /// Raised to read the resource again when the address already names it.
    count: RwSignal<u32>,
    /// Whether the next form built is the answer to a reload.
    asked: StoredValue<bool>,
    /// What the address names the resource by right now.
    opened: Signal<String>,
    /// Where the address goes when it names another resource.
    navigate: StoredValue<Navigate>,
}

impl Reload {
    /// A reload over the resource the address names in `opened`.
    pub(crate) fn new(opened: Signal<String>) -> Self {
        let navigate: Navigate = Arc::new(use_navigate());
        Self {
            count: RwSignal::new(0),
            asked: StoredValue::new(false),
            opened,
            navigate: StoredValue::new(navigate),
        }
    }

    /// How many reloads were asked for, read reactively, so a read whose
    /// source names it reruns on each.
    pub(crate) fn generation(self) -> u32 {
        self.count.get()
    }

    /// Reads the resource `named` names again.
    ///
    /// When the address names something else, the reload goes through
    /// `address`, the screen's own link to the resource, and the changed query
    /// is what reruns the read.
    pub(crate) fn press(self, named: &str, address: Option<String>) {
        self.asked.set_value(true);
        let target = address.filter(|_| self.opened.get_untracked() != named);
        match target {
            Some(target) => self.navigate.with_value(|navigate| {
                navigate(
                    &target,
                    NavigateOptions {
                        resolve: false,
                        ..NavigateOptions::default()
                    },
                );
            }),
            None => self.count.update(|count| *count = count.saturating_add(1)),
        }
    }

    /// Whether the form being built answers a reload, which spends the ask.
    pub(crate) fn answered(self) -> bool {
        self.asked
            .try_update_value(std::mem::take)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::*;

    #[test]
    fn a_refusal_without_an_outcome_still_says_what_to_do() {
        let error = FhirError::Status {
            url: String::from("/r4b/CodeSystem/x"),
            status: StatusCode::PRECONDITION_FAILED,
            body: String::new(),
        };
        assert_eq!(announced(&error), Refusal::ConcurrentEdit.what_to_do());
    }

    #[test]
    fn a_reloaded_form_names_the_version_it_now_shows() {
        assert_eq!(
            reloaded_text("2"),
            "Reloaded. The form now shows version 2, the one the server holds."
        );
        assert_eq!(
            reloaded_text(""),
            "Reloaded. The server stated no version for it."
        );
    }
}
