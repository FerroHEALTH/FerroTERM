//! The versions of one resource: what they are, what two of them differ by,
//! and putting an earlier one back.
//!
//! The list is read with whichever interaction the root's capability statement
//! declares, `history-instance` or `vread`
//! (<https://hl7.org/fhir/R4B/http.html#history>,
//! <https://hl7.org/fhir/R4B/http.html#vread>), so a root that declares
//! neither draws no list and says why.
//!
//! A restore is an ordinary update of the whole resource with `If-Match` of
//! the version the server holds now
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>), so an edit made
//! elsewhere since this screen was opened is refused with `412` rather than
//! overwritten. The difference between two versions is our own design; no FHIR
//! specification governs how a client renders one.

use std::sync::Arc;

use leptos::ev::MouseEvent;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_query_map;

use crate::auth::Session;
use crate::auth::scopes::Letter;
use crate::authoring::history::Change;
use crate::authoring::history::Comparison;
use crate::authoring::history::Difference;
use crate::authoring::history::Opened;
use crate::authoring::history::Subject;
use crate::authoring::history::Support;
use crate::authoring::history::open;
use crate::authoring::history::restore;
use crate::components::failure::Failure;
use crate::components::reload::READING_AGAIN;
use crate::components::reload::announced;
use crate::components::reload::focus_report;
use crate::components::reload::reload_offer;
use crate::components::shell::Console;
use crate::components::shell::SelectedVersion;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::CONCEPT_MAP;
use crate::fhir::FhirClient;
use crate::fhir::VALUE_SET;
use crate::fhir::error::FhirError;
use crate::fhir::sync;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::Refusal;
use crate::fhir::write::Version;
use crate::fhir::write::Written;
use crate::routes::HISTORY_PATH;
use crate::routes::ID_PARAM;
use crate::routes::TYPE_PARAM;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::routes::screen_link;
use crate::styles;
use ferrohealth_ui::view::empty::{Busy, busy};

/// The live region every outcome of this screen is announced in.
const REPORT_ID: &str = "history-report";

/// The query parameter naming the earlier version of a comparison.
const FROM_PARAM: &str = "from";

/// The query parameter naming the later version of a comparison.
const TO_PARAM: &str = "to";

/// The resource types this editor writes, and so the ones it reads versions of.
const WRITTEN_TYPES: [&str; 3] = [CODE_SYSTEM, VALUE_SET, CONCEPT_MAP];

/// Which two versions a comparison is between, read out of the address.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Compared {
    /// The earlier version's identifier, empty when none is chosen.
    from: String,
    /// The later version's identifier, empty when none is chosen.
    to: String,
}

impl Compared {
    /// Whether both sides were chosen.
    fn chosen(&self) -> bool {
        !self.from.is_empty() && !self.to.is_empty()
    }
}

/// The screen's own address, with one comparison selected.
fn address(subject: &Subject, compared: &Compared, version: FhirVersion) -> String {
    let mut url = base_url()
        .segment(HISTORY_PATH)
        .query(VERSION_PARAM, version.segment())
        .query(TYPE_PARAM, &subject.resource_type)
        .query(ID_PARAM, &subject.id);
    if !compared.from.is_empty() {
        url = url.query(FROM_PARAM, &compared.from);
    }
    if !compared.to.is_empty() {
        url = url.query(TO_PARAM, &compared.to);
    }
    url.render("")
}

/// Reads one resource's versions, compares two of them, and restores one.
///
/// Every parameter is read reactively: a comparison is a navigation matching
/// this same route, and `leptos_router` then updates the query without
/// re-running this body.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn HistoryPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let session = expect_context::<Session>();
    let query = use_query_map();
    let subject: Memo<Subject> = Memo::new(move |_| {
        query.with(|map| Subject {
            resource_type: map.get(TYPE_PARAM).unwrap_or_default().trim().to_owned(),
            id: map.get(ID_PARAM).unwrap_or_default().trim().to_owned(),
        })
    });
    let compared: Memo<Compared> = Memo::new(move |_| {
        query.with(|map| Compared {
            from: map.get(FROM_PARAM).unwrap_or_default().trim().to_owned(),
            to: map.get(TO_PARAM).unwrap_or_default().trim().to_owned(),
        })
    });

    // Reading this is what a restore re-runs: the versions depend on it, so
    // raising it reads the resource and its versions again and rebuilds the
    // list around what came back. The subject travels into the answer, so
    // nothing drawn from the answer has to read the address a second time.
    let reloads = RwSignal::new(0_u32);
    let reader = client.clone();
    let read = Resource::new(
        move || (version.get(), subject.get(), reloads.get()),
        move |(at, held, _reload)| {
            let client = reader.clone();
            async move {
                if !WRITTEN_TYPES.contains(&held.resource_type.as_str()) {
                    return Ok(Opened {
                        subject: held,
                        support: Support::Neither,
                        ..Opened::default()
                    });
                }
                let statement = client.capability_statement(at).await?;
                let support = Support::of(&statement, &held.resource_type);
                open(&client, at, support, held).await
            }
        },
    );

    // Everything a restore reports lives here rather than inside the section
    // below: a taken restore raises `reloads`, which rebuilds that section, and
    // a report created inside it would be destroyed by the reload it asked for.
    // Standing here also puts the live region in the document from the start.
    let restoring = Restoring {
        client: StoredValue::new(client.clone()),
        version,
        session,
        reloads,
        report: RwSignal::new(String::new()),
        refusal: RwSignal::new(None),
        in_flight: RwSignal::new(false),
    };

    let heading = heading_section(subject);
    let outcome = outcome_section(restoring);
    let finder = client.clone();
    let body = view! {
        <Transition fallback=|| {
            busy("Reading the versions", Busy::Section)
        }>
            {move || {
                read.with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer.as_ref() {
                            Err(error) => view! { <Failure error=error.clone() /> }.into_any(),
                            Ok(
                                opened,
                            ) if !WRITTEN_TYPES.contains(&opened.subject.resource_type.as_str()) => {
                                unwritten_section(&opened.subject.resource_type)
                            }
                            Ok(opened) => versions_body(restoring, opened, compared),
                        })
                })
            }}
        </Transition>
    }
    .into_any();
    // NOTE: no specification governs this: our own design; a console that
    // reaches no sync service has no runs to read, so it asks for none.
    let findings = if expect_context::<Console>().sync {
        findings_section(&finder, read)
    } else {
        ().into_any()
    };

    view! {
        {heading}
        {outcome}
        {body}
        {findings}
    }
}

/// The screen's title and what it is showing.
///
/// This is the one section drawn from the address rather than from the answer,
/// because it says what the screen is reading rather than what it read.
fn heading_section(subject: Memo<Subject>) -> AnyView {
    let named = move || {
        let held = subject.get();
        if held.id.is_empty() {
            "Versions".to_owned()
        } else {
            format!("{}/{}", held.resource_type, held.id)
        }
    };
    view! {
        <Title text=named />
        {move || {
            ferrohealth_ui::view::surface::page_heading(
                ferrohealth_ui::view::surface::Header {
                    title: named(),
                    subtitle: Some(
                        String::from(
                            "Every version the server holds, what any two of them differ by, and putting an earlier one back.",
                        ),
                    ),
                    crumbs: listing_crumb(&subject.get().resource_type).into_iter().collect(),
                    mono: true,
                },
                (),
            )
        }}
    }
    .into_any()
}

/// The listing a resource of `resource_type` is found in, as a crumb.
fn listing_crumb(resource_type: &str) -> Option<ferrohealth_ui::view::surface::Crumb> {
    let (label, href) = match resource_type {
        "CodeSystem" => ("Code systems", "/systems"),
        "ValueSet" => ("Value sets", "/valuesets"),
        "ConceptMap" => ("Concept maps", "/conceptmaps"),
        _ => return None,
    };
    Some(ferrohealth_ui::view::surface::Crumb::new(label, href))
}

/// What the screen says about a resource type it does not write.
fn unwritten_section(resource_type: &str) -> AnyView {
    let named = if resource_type.is_empty() {
        "The address names no resource type, so there is nothing to read the versions of."
            .to_owned()
    } else {
        format!("This editor does not write {resource_type}, so it reads no versions of one.")
    };
    ferrohealth_ui::view::notice::notice(
        ferrohealth_ui::tone::Tone::Info,
        ferrohealth_ui::view::notice::Announce::Status,
        None,
        named,
    )
    .into_any()
}

/// The version list, the comparison, and everything a restore reports.
///
/// Every part of it is built from the answer rather than from the address. A
/// same-route navigation updates the address while this answer is still the
/// one on screen, and a restore drawn from the new address over the old answer
/// would send one resource's document to another resource's id.
fn versions_body(restoring: Restoring, opened: &Opened, compared: Memo<Compared>) -> AnyView {
    let current = opened.current_version_id();
    let writable = !current.is_empty()
        && restoring
            .session
            .can(&opened.subject.resource_type, Letter::Update);

    let back = back_section(opened, restoring.version);
    let listed = list_section(restoring, opened, writable);
    let compare = compare_section(opened, compared, restoring.version);
    let difference = difference_section(opened, compared);

    ferrohealth_ui::view::surface::stack(view! {
        {back}
        {listed}
        {compare}
        {difference}
    })
    .into_any()
}

/// The link back to the resource's own screen.
///
/// The canonical comes off the resource the server sent, because that is what
/// a code system's screen addresses it by.
fn back_section(opened: &Opened, version: Signal<FhirVersion>) -> AnyView {
    let Some((address, words)) = screen_link(
        &opened.subject.resource_type,
        &opened.canonical(),
        &opened.subject.id,
        version.get_untracked(),
    ) else {
        return ().into_any();
    };
    view! {
        <p>
            <a href=address class=styles::LINK>
                {words}
            </a>
        </p>
    }
    .into_any()
}

/// Everything a restore needs, so one argument carries it.
#[derive(Clone, Copy)]
struct Restoring {
    /// The client that sends the update.
    client: StoredValue<FhirClient>,
    /// The served FHIR version the update goes to.
    version: Signal<FhirVersion>,
    /// The session, so a spent token is dropped.
    session: Session,
    /// The counter a taken restore raises, so the versions are read again.
    reloads: RwSignal<u32>,
    /// The live region.
    report: RwSignal<String>,
    /// The refusal the screen renders whole.
    refusal: RwSignal<Option<FhirError>>,
    /// Whether an update is in flight.
    in_flight: RwSignal<bool>,
}

impl Restoring {
    /// What one row's restore control does when it is pressed.
    ///
    /// `chosen` is the resource that version holds, `at` its identifier, and
    /// `current` the version the server holds now, which is the one `If-Match`
    /// states (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    fn presses(
        self,
        subject: Subject,
        chosen: Option<serde_json::Value>,
        at: String,
        current: String,
    ) -> impl Fn(MouseEvent) + Clone {
        move |_press| {
            if self.in_flight.get_untracked() {
                return;
            }
            let Some(chosen) = chosen.clone() else {
                self.report.set(String::from(
                    "This version carries no resource to put back.",
                ));
                return;
            };
            self.in_flight.set(true);
            self.refusal.set(None);
            self.report.set(String::from("Restoring"));
            let sent = restore(&chosen, &subject.id, &current);
            let client = self.client.get_value();
            let version = self.version.get_untracked();
            let resource_type = subject.resource_type.clone();
            let logical = subject.id.clone();
            let chose = at.clone();
            let held = self;
            spawn_local(async move {
                let written = Box::pin(client.update(
                    version,
                    &resource_type,
                    &logical,
                    &sent.body,
                    Some(sent.version_id.as_str()).filter(|stated| !stated.is_empty()),
                ))
                .await;
                held.wrote(&chose, written);
                held.in_flight.set(false);
            });
        }
    }

    /// Writes down what the server answered a restore with.
    fn wrote(self, chosen: &str, written: Result<Written, FhirError>) {
        match written {
            Ok(written) => {
                let now = written.version_id().unwrap_or_default().to_owned();
                self.report.set(restored_text(chosen, &now));
                self.reloads
                    .update(|count| *count = count.saturating_add(1));
            }
            Err(error) => {
                let refused = Refusal::of(&error);
                if refused.drops_the_token() {
                    self.session.release();
                }
                self.report.set(announced(&error));
                self.refusal.set(Some(error));
            }
        }
    }
}

/// The live region and the refusal a restore met.
///
/// It stands above the version list rather than inside it, because a restore
/// the server took raises `reloads` and the list is rebuilt around what comes
/// back: an announcement made inside it would be destroyed by the reload it
/// asked for.
fn outcome_section(restoring: Restoring) -> AnyView {
    let Restoring {
        reloads,
        report,
        refusal,
        ..
    } = restoring;
    // The region is in the document whether or not it has anything to say,
    // because a region inserted along with its first message is not announced
    // (<https://www.w3.org/TR/wai-aria-1.2/#aria-live>).
    let region = view! {
        <p
            id=REPORT_ID
            tabindex="-1"
            aria-live="polite"
            class=format!("mt-default {}", styles::MUTED)
        >
            {move || report.get()}
        </p>
    }
    .into_any();

    // The region stands outside the list a reload rebuilds, so the keyboard
    // moves to it as soon as the reading starts.
    let press: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        report.set(String::from(READING_AGAIN));
        reloads.update(|count| *count = count.saturating_add(1));
        focus_report(REPORT_ID);
    });
    let press = StoredValue::new(press);
    let refused = view! {
        <Show when=move || refusal.with(Option::is_some) fallback=|| ()>
            <div class="mt-default grid gap-default">
                <p class=styles::MUTED>
                    {move || {
                        refusal
                            .with(|held| {
                                held.as_ref().map(|error| Refusal::of(error).what_to_do())
                            })
                    }}
                </p>
                {move || reload_offer(refusal, press.get_value())}
                {move || {
                    refusal
                        .with(|held| {
                            held.as_ref()
                                .map(|error| {
                                    view! { <Failure error=error.clone() /> }.into_any()
                                })
                        })
                }}
            </div>
        </Show>
    }
    .into_any();

    view! {
        {region}
        {refused}
    }
    .into_any()
}

/// The table of versions, with a restore control on each earlier one.
fn list_section(restoring: Restoring, opened: &Opened, writable: bool) -> AnyView {
    let current = opened.current_version_id();
    let rows: Vec<AnyView> = opened
        .versions
        .iter()
        .map(|version| {
            let restorable = writable && version.id != current;
            version_row(
                restoring,
                opened.subject.clone(),
                version,
                &current,
                restorable,
            )
        })
        .collect();
    // This section is rebuilt whenever the versions are read again, so what
    // it draws is decided here rather than by a reactive branch over values
    // that cannot change while it stands.
    let listed = if rows.is_empty() {
        let why = opened.support.why_empty();
        view! { <p class=format!("mt-default {}", styles::MUTED)>{why}</p> }.into_any()
    } else {
        view! {
            <div class="mt-default overflow-x-auto">
                <table class=styles::TABLE>
                    <thead>
                        <tr>
                            <th scope="col" class=styles::TH>
                                "Version"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Changed"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Interaction"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Source system"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Restore"
                            </th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        }
        .into_any()
    };
    let composed = if opened.support == Support::VersionRead {
        view! {
            <p class=format!(
                "mt-default {}",
                styles::HINT,
            )>
                "This server declares the version read and not the history interaction, so the list is built by reading each earlier version back. A version identifier is opaque in FHIR, so a server that does not count its versions shows only the one it states."
            </p>
        }
        .into_any()
    } else {
        ().into_any()
    };
    let bounded = if opened.truncated {
        view! {
            <p class=format!(
                "mt-default {}",
                styles::HINT,
            )>"Only the most recent versions are listed."</p>
        }
        .into_any()
    } else {
        ().into_any()
    };
    view! {
        <section class="fui-card" aria-labelledby="history-versions-heading">
            <h2 id="history-versions-heading" class="fui-card-title">
                "Versions"
            </h2>
            {listed}
            {composed}
            {bounded}
        </section>
    }
    .into_any()
}

/// One version's row.
fn version_row(
    restoring: Restoring,
    subject: Subject,
    version: &Version,
    current: &str,
    restorable: bool,
) -> AnyView {
    let at = version.id.clone();
    let labelled = at.clone();
    let when = version.last_updated.clone();
    let method = version.method.clone();
    let source = version.source.clone();
    let is_current = version.id == current;
    let held = version.resource.clone();
    let restore_it = restoring.presses(subject, held, at.clone(), current.to_owned());
    view! {
        <tr>
            <th scope="row" class=format!("{} font-normal", styles::TD_TIGHT)>
                <span class=styles::CODE>{at.clone()}</span>
                {marked(is_current)}
            </th>
            <td class=styles::TD>{absent_or(when)}</td>
            <td class=styles::TD>{absent_or(method)}</td>
            <td class=styles::TD>{absent_or(source)}</td>
            <td class=styles::TD>
                {if restorable {
                    view! {
                        <button
                            type="button"
                            name="restore-version"
                            value=at
                            class=styles::BUTTON
                            aria-describedby=REPORT_ID
                            disabled=move || restoring.in_flight.get()
                            on:click=restore_it
                        >
                            {format!("Restore version {labelled}")}
                        </button>
                    }
                        .into_any()
                } else {
                    unrestorable()
                }}
            </td>
        </tr>
    }
    .into_any()
}

/// The mark on the version the server holds now, where this row is it.
fn marked(is_current: bool) -> AnyView {
    if is_current {
        view! { <span class=format!("ml-tight {}", styles::BADGE)>"current"</span> }.into_any()
    } else {
        ().into_any()
    }
}

/// What stands where a restore control would, on a version that has none.
///
/// The reason is `sr-only` rather than a `title`, which a keyboard reader
/// never reaches (<https://www.w3.org/WAI/WCAG22/Techniques/css/C7>).
fn unrestorable() -> AnyView {
    view! {
        <span class=styles::ABSENT aria-hidden="true">
            {styles::ABSENT_MARK}
        </span>
        <span class="sr-only">
            "This version is the one the server holds now, or this account cannot write this resource."
        </span>
    }
    .into_any()
}

/// One cell's text, or the mark for a fact the resource does not state.
///
/// An element the resource states as an empty string is drawn as one, in
/// quotes, because "the server stated nothing" and "the server stated nothing
/// in it" are different answers and a comparison turns on which it was.
fn absent_or(stated: Option<String>) -> AnyView {
    match stated {
        Some(text) if text.is_empty() => {
            view! { <span class=styles::CODE_MUTED>"\u{201c}\u{201d}"</span> }.into_any()
        }
        Some(text) => {
            view! { <span class=styles::CODE_MUTED>{crate::components::stated(text)}</span> }
                .into_any()
        }
        None => view! {
            <span class=styles::ABSENT title="The server states nothing here.">
                {styles::ABSENT_MARK}
            </span>
        }
        .into_any(),
    }
}

/// What the live region says after a restore the server took.
fn restored_text(chosen: &str, now: &str) -> String {
    if now.is_empty() {
        format!("Restored version {chosen}. The server stated no version for the write.")
    } else {
        format!("Restored version {chosen}. The server now holds version {now}.")
    }
}

/// The two controls that choose which versions to compare.
///
/// The choice lands in the address rather than in a private signal, so a
/// comparison is a link a reader can share and the browser's back button
/// walks. The comparison runs on a press rather than on each option a reader
/// moves through (<https://www.w3.org/TR/WCAG22/#on-input>).
fn compare_section(
    opened: &Opened,
    compared: Memo<Compared>,
    version: Signal<FhirVersion>,
) -> AnyView {
    if opened.versions.len() < 2 {
        return ().into_any();
    }
    let navigate = StoredValue::new(use_navigate());
    let ids: Vec<String> = opened
        .versions
        .iter()
        .map(|version| version.id.clone())
        .collect();
    let subject = StoredValue::new(opened.subject.clone());
    // The two pickers are seeded from the address and rebuilt whenever it
    // changes, so the browser's back button and a link another reader sent
    // both leave them showing the pair the difference table is drawn from.
    let form = move || {
        let chosen = compared.get();
        let earlier = RwSignal::new(chosen.from.clone());
        let later = RwSignal::new(chosen.to.clone());
        let submitted = move |event: SubmitEvent| {
            event.prevent_default();
            let target = subject.with_value(|subject| {
                address(
                    subject,
                    &Compared {
                        from: earlier.get_untracked(),
                        to: later.get_untracked(),
                    },
                    version.get_untracked(),
                )
            });
            navigate.with_value(|navigate| {
                navigate(
                    &target,
                    NavigateOptions {
                        resolve: false,
                        ..NavigateOptions::default()
                    },
                );
            });
        };
        let from_options = options(&ids, &chosen.from);
        let to_options = options(&ids, &chosen.to);
        view! {
            <form class="mt-default flex flex-wrap items-end gap-default" on:submit=submitted>
                <div class="grid gap-tight">
                    <label for="history-from" class=styles::LABEL>
                        "Earlier version"
                    </label>
                    <select
                        id="history-from"
                        name="from"
                        class=ferrohealth_ui::view::form::SELECT
                        prop:value=move || earlier.get()
                        on:change:target=move |event| earlier.set(event.target().value())
                    >
                        {from_options}
                    </select>
                </div>
                <div class="grid gap-tight">
                    <label for="history-to" class=styles::LABEL>
                        "Later version"
                    </label>
                    <select
                        id="history-to"
                        name="to"
                        class=ferrohealth_ui::view::form::SELECT
                        prop:value=move || later.get()
                        on:change:target=move |event| later.set(event.target().value())
                    >
                        {to_options}
                    </select>
                </div>
                <button type="submit" class=styles::SUBMIT>
                    "Compare these versions"
                </button>
            </form>
        }
        .into_any()
    };
    view! {
        <section class="fui-card" aria-labelledby="history-compare-heading">
            <h2 id="history-compare-heading" class="fui-card-title">
                "Compare two versions"
            </h2>
            {form}
        </section>
    }
    .into_any()
}

/// The options of one version picker, with the unchosen state first.
///
/// Each option states its own selectedness as well as the select's own
/// `prop:value`. Both are needed: attributes land before children, so a select
/// whose `value` names an option it does not have yet falls back to its first
/// one (<https://html.spec.whatwg.org/multipage/form-elements.html#the-select-element>).
fn options(ids: &[String], chosen: &str) -> Vec<AnyView> {
    let mut drawn = vec![
        view! {
            <option value="" selected=chosen.is_empty()>
                "Choose a version"
            </option>
        }
        .into_any(),
    ];
    drawn.extend(ids.iter().map(|id| {
        let selected = id == chosen;
        let id = id.clone();
        let label = format!("Version {id}");
        view! {
            <option value=id selected=selected>
                {label}
            </option>
        }
        .into_any()
    }));
    drawn
}

/// What the two chosen versions differ by.
///
/// It stands beside the pickers whether or not a comparison has been asked
/// for, because a live region inserted along with its first message is not
/// announced (<https://www.w3.org/TR/wai-aria-1.2/#aria-live>).
fn difference_section(opened: &Opened, compared: Memo<Compared>) -> AnyView {
    if opened.versions.len() < 2 {
        return ().into_any();
    }
    let opened = opened.clone();
    let found: Memo<Comparison> = Memo::new(move |_| {
        let chosen = compared.get();
        if !chosen.chosen() {
            return Comparison::Unasked;
        }
        Comparison::of(
            opened.resource_at(&chosen.from),
            opened.resource_at(&chosen.to),
        )
    });
    let counted = move || found.read().sentence();
    let table = move || {
        let held = found.read();
        if held.rows().is_empty() {
            return ().into_any();
        }
        let rows: Vec<AnyView> = held.rows().iter().map(difference_row).collect();
        view! {
            <div class="mt-default overflow-x-auto">
                <table class=styles::TABLE>
                    <thead>
                        <tr>
                            <th scope="col" class=styles::TH>
                                "Element"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Change"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Earlier"
                            </th>
                            <th scope="col" class=styles::TH>
                                "Later"
                            </th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        }
        .into_any()
    };
    view! {
        <section class="fui-card" aria-labelledby="history-difference-heading">
            <h2 id="history-difference-heading" class="fui-card-title">
                "What changed"
            </h2>
            // The region is in the document before the first comparison runs,
            // because a region inserted along with its first message is not
            // announced (<https://www.w3.org/TR/wai-aria-1.2/#aria-live>).
            <p id="history-difference-count" aria-live="polite" class="fui-muted">
                {counted}
            </p>
            {table}
        </section>
    }
    .into_any()
}

/// One row of the difference table.
fn difference_row(row: &Difference) -> AnyView {
    let mark = match row.change {
        Change::Added => styles::BADGE_OK,
        Change::Removed => styles::BADGE_DANGER,
        Change::Changed => styles::BADGE,
    };
    let path = row.path.clone();
    let word = row.change.word();
    // A side the change says the element was not on states nothing, which is
    // a different cell from one that states an empty value.
    let before = (row.change != Change::Added).then(|| row.before.clone());
    let after = (row.change != Change::Removed).then(|| row.after.clone());
    view! {
        <tr>
            <th scope="row" class=format!("{} font-normal", styles::TD)>
                <span class=styles::CODE>{path}</span>
            </th>
            <td class=styles::TD_TIGHT>
                <span class=mark>{word}</span>
            </td>
            <td class=styles::TD>{absent_or(before)}</td>
            <td class=styles::TD>{absent_or(after)}</td>
        </tr>
    }
    .into_any()
}

/// What the newest synchronisation run found about the resource on screen.
///
/// The console's server reads the sync service's admin listener named in its
/// configuration. Where the run list cannot be read or names nothing that
/// concerns this resource, the section is not in the document at all.
fn findings_section(client: &FhirClient, read: Resource<Result<Opened, FhirError>>) -> AnyView {
    let client = client.clone();
    let revalidated = LocalResource::new(move || {
        let client = client.clone();
        async move { client.latest_findings().await }
    });
    let concerning: Memo<Vec<sync::Finding>> = Memo::new(move |_| {
        let findings = revalidated.with(|answered| {
            answered
                .as_ref()
                .and_then(|answer| answer.as_ref().ok())
                .cloned()
                .unwrap_or_default()
        });
        if findings.is_empty() {
            return Vec::new();
        }
        // The resource these findings are matched against is the one the
        // answer was read for, never the one the address names now.
        let names = read.with(|answered| {
            answered
                .as_ref()
                .and_then(|answer| answer.as_ref().ok())
                .map(|opened| {
                    sync::names_of(
                        &opened.subject.resource_type,
                        &opened.subject.id,
                        &opened.canonical(),
                    )
                })
                .unwrap_or_default()
        });
        sync::concerning(&findings, &names)
    });
    let rows = move || {
        concerning
            .read()
            .iter()
            .map(finding_row)
            .collect::<Vec<AnyView>>()
    };
    view! {
        <Transition fallback=|| ()>
            <Show when=move || !concerning.read().is_empty() fallback=|| ()>
                <section class="fui-card" aria-labelledby="history-findings-heading">
                    <h2 id="history-findings-heading" class="fui-card-title">
                        "What the last synchronisation found"
                    </h2>
                    <p class="fui-muted">
                        "Codes this resource names that the release the sync service last activated changed."
                    </p>
                    <div class="mt-default overflow-x-auto">
                        <table class=styles::TABLE>
                            <thead>
                                <tr>
                                    <th scope="col" class=styles::TH>
                                        "Code"
                                    </th>
                                    <th scope="col" class=styles::TH>
                                        "System"
                                    </th>
                                    <th scope="col" class=styles::TH>
                                        "What the release did"
                                    </th>
                                    <th scope="col" class=styles::TH>
                                        "Release"
                                    </th>
                                </tr>
                            </thead>
                            <tbody>{rows}</tbody>
                        </table>
                    </div>
                </section>
            </Show>
        </Transition>
    }
    .into_any()
}

/// One finding's row.
fn finding_row(finding: &sync::Finding) -> AnyView {
    let code = finding.code.clone();
    let system = finding.system.clone();
    let happened = finding.what_happened();
    let release = finding.release.clone();
    view! {
        <tr>
            <th scope="row" class=format!("{} font-normal", styles::TD_TIGHT)>
                <span class=styles::CODE>{code}</span>
            </th>
            <td class=styles::TD>
                <span class=styles::CODE_MUTED>{system}</span>
            </td>
            <td class=styles::TD>{happened}</td>
            <td class=styles::TD>{absent_or(release)}</td>
        </tr>
    }
    .into_any()
}
