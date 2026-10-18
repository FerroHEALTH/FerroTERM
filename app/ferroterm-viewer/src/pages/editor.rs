//! Authoring one local `CodeSystem` in the browser.
//!
//! The screen is the same for a code system this deployment built from a
//! release and one a person wrote here: the built one opens read-only, because
//! the capability statement marks it as served from an artifact and an
//! artifact has no write path. What makes the other editable is three facts
//! off the wire, and nothing the bundle assumes: the server states a
//! `meta.versionId` for it, the capability statement marks no artifact behind
//! it, and the token in hand carries the scope for the change.
//!
//! Every write is a create or an update of the whole resource with `If-Match`
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>), so an edit made
//! somewhere else since the form was opened is refused with `412` rather than
//! overwritten. Nothing here deletes a concept: a published code keeps meaning
//! what it meant, and retiring it is the change that says so
//! (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).

use std::sync::Arc;

use leptos::ev::Event;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::use_query;
use leptos_router::params::Params;

use crate::auth::Session;
use crate::auth::scopes::Letter;
use crate::authoring::code_system::Concept;
use crate::authoring::code_system::Declared;
use crate::authoring::code_system::Designation;
use crate::authoring::code_system::Draft;
use crate::authoring::code_system::Key;
use crate::authoring::code_system::Lifecycle;
use crate::authoring::code_system::Valued;
use crate::components::coded::Codes;
use crate::components::coded::Control;
use crate::components::coded::coded_control;
use crate::components::coded::codes_of;
use crate::components::coded::fixed;
use crate::components::coded::gated;
use crate::components::failure::Failure;
use crate::components::history::history_offer;
use crate::components::reload::Reload;
use crate::components::reload::announced;
use crate::components::reload::focus_report;
use crate::components::reload::reload_offer;
use crate::components::reload::reloaded_text;
use crate::components::shell::SelectedVersion;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::FhirClient;
use crate::fhir::error::FhirError;
use crate::fhir::terminology::SystemCard;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::validation::ValidateRequest;
use crate::fhir::validation::Validation;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::Refusal;
use crate::routes::editing_link;
use crate::styles;
use ferrohealth_ui::view::empty::{Busy, busy};

/// The value set whose codes `CodeSystem.status` is bound to.
///
/// The control offers what the served root expands this to rather than a list
/// compiled into the bundle (<https://hl7.org/fhir/R4B/codesystem.html>).
const PUBLICATION_STATUS: &str = "http://hl7.org/fhir/ValueSet/publication-status";

/// The value set whose codes `CodeSystem.content` is bound to.
const CONTENT_MODE: &str = "http://hl7.org/fhir/ValueSet/codesystem-content-mode";

/// The value set whose codes `CodeSystem.property.type` is bound to.
const PROPERTY_TYPE: &str = "http://hl7.org/fhir/ValueSet/concept-property-type";

/// The live region every outcome of this screen is announced in.
const REPORT_ID: &str = "editor-report";

/// The four lifecycle states a reader moves a concept between, in order.
///
/// The words are the reader's; the codes are the standard property's
/// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
const LIFECYCLE_STATES: [(&str, &str); 4] = [
    ("active", "Active"),
    ("experimental", "Experimental"),
    ("deprecated", "Deprecated"),
    ("retired", "Retired"),
];

/// The screen's own query parameters.
#[derive(Clone, Debug, Params, PartialEq)]
struct EditorQuery {
    /// The canonical of the code system being edited, absent for a new one.
    system: Option<String>,
}

/// Authors one local code system, or shows a built one read-only.
///
/// The canonical is read reactively: a link from one system to another matches
/// this same route, and `leptos_router` then updates the query without
/// re-running this body, so a read taken untracked at setup would go stale.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn EditorPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let query = use_query::<EditorQuery>();
    let system = Signal::derive(move || {
        query
            .read()
            .as_ref()
            .ok()
            .and_then(|query| query.system.clone())
            .unwrap_or_default()
    });

    // A reload reruns this read, and the form is rebuilt around what came
    // back, rather than an effect writing the form's own signals.
    let reload = Reload::new(system);
    let reader = client.clone();
    let stored = Resource::new(
        move || (version.get(), system.get(), reload.generation()),
        move |(version, system, _reloads)| {
            let client = reader.clone();
            async move {
                if system.trim().is_empty() {
                    return Ok(None);
                }
                client.authored(version, &system).await
            }
        },
    );

    let capability_client = client.clone();
    let capabilities = Resource::new(
        move || version.get(),
        move |version| {
            let client = capability_client.clone();
            async move { client.terminology_capabilities(version).await }
        },
    );
    let standing = standing_of(capabilities, system);

    let statuses = codes_of(&client, version, bound(PUBLICATION_STATUS));
    let contents = codes_of(&client, version, bound(CONTENT_MODE));
    let kinds = codes_of(&client, version, bound(PROPERTY_TYPE));

    let heading = view! {
        <Title text=move || title_of(&system.get()) />
        <div>
            {ferrohealth_ui::view::surface::page_heading(
                ferrohealth_ui::view::surface::Header {
                    title: String::from("Edit a code system"),
                    subtitle: Some(
                        String::from(
                            "Its metadata, the properties it declares, and its concepts with their designations and lifecycle.",
                        ),
                    ),
                    crumbs: vec![
                        ferrohealth_ui::view::surface::Crumb::new("Code systems", "/systems"),
                    ],
                    mono: false,
                },
                (),
            )}
        </div>
    }
    .into_any();

    let form = view! {
        <Transition fallback=|| {
            busy("Reading the code system", Busy::Section)
        }>
            {move || {
                let client = client.clone();
                stored
                    .with(|answered| {
                        answered
                            .as_ref()
                            .map(|read| match read.as_ref() {
                                Err(error) => view! { <Failure error=error.clone() /> }.into_any(),
                                Ok(held) => {
                                    let draft = held.as_ref().map_or_else(Draft::new, Draft::of);
                                    form_section(
                                        client,
                                        version,
                                        draft,
                                        standing,
                                        reload,
                                        Options {
                                            statuses,
                                            contents,
                                            kinds,
                                        },
                                    )
                                }
                            })
                    })
            }}
        </Transition>
    }
    .into_any();

    view! {
        {heading}
        {form}
    }
}

/// What the served root says about the code system, read from its statement.
///
/// A statement that has not answered, and one that did not read, are two
/// different standings and neither is "no artifact": the screen offers to
/// change nothing it cannot place.
fn standing_of(
    capabilities: Resource<Result<TerminologyCapabilities, FhirError>>,
    system: Signal<String>,
) -> Standing {
    Standing {
        backing: Signal::derive(move || {
            let system = system.get();
            capabilities.with(|answered| match answered.as_ref() {
                None => Backing::Reading,
                Some(Err(_unread)) => Backing::Unreadable,
                Some(Ok(capabilities)) => {
                    let artifact = capabilities.card(&system).is_some_and(|card: SystemCard| {
                        card.versions
                            .iter()
                            .any(|version| version.artifact.is_some())
                    });
                    if artifact {
                        Backing::Artifact
                    } else {
                        Backing::Local
                    }
                }
            })
        }),
        capability: Signal::derive(move || {
            capabilities.with(|answered| {
                answered
                    .as_ref()
                    .and_then(|read| read.as_ref().err().cloned())
            })
        }),
    }
}

/// The document title for the code system being edited.
fn title_of(system: &str) -> String {
    if system.trim().is_empty() {
        "New code system".to_owned()
    } else {
        format!("Edit {system}")
    }
}

/// The canonical of a value set one element is bound to, as a signal.
///
/// The bindings of `CodeSystem` are the same on every served version, so this
/// is a constant read through the signal the control takes.
fn bound(canonical: &'static str) -> Signal<String> {
    Signal::derive(move || canonical.to_owned())
}

/// The three coded controls' options, so one argument carries them together.
#[derive(Clone, Copy)]
struct Options {
    /// The codes `CodeSystem.status` admits.
    statuses: Codes,
    /// The codes `CodeSystem.content` admits.
    contents: Codes,
    /// The codes `CodeSystem.property.type` admits.
    kinds: Codes,
}

/// What the served root says is behind the code system being edited.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Backing {
    /// The capability statement has not answered yet.
    Reading,
    /// It did not read, so what is behind the code system is unknown.
    Unreadable,
    /// It declares an artifact, which is read at startup and never written.
    Artifact,
    /// It declares none, so the resource is one the REST API manages.
    Local,
}

/// What the served root says about the resource, for the form to rest on.
#[derive(Clone, Copy)]
struct Standing {
    /// What is behind the code system.
    backing: Signal<Backing>,
    /// Why the capability statement did not read, when it did not.
    capability: Signal<Option<FhirError>>,
}

/// The whole form over one draft, and everything a save reports.
///
/// The draft is created here rather than written into from an effect: this
/// function runs inside the closure that reads the resource, so a refetch
/// disposes this form and builds a new one over what came back.
fn form_section(
    client: FhirClient,
    version: Signal<FhirVersion>,
    initial: Draft,
    standing: Standing,
    reload: Reload,
    options: Options,
) -> AnyView {
    let session = expect_context::<Session>();
    let managed = initial.managed() || initial.id.is_empty();
    // The canonical the server holds the resource under, which is what a
    // reload reads by even when the reader has since typed another.
    let canonical = StoredValue::new(if initial.id.is_empty() {
        String::new()
    } else {
        initial.url.clone()
    });
    let reloaded = reload.answered();
    let report = RwSignal::new(if reloaded {
        reloaded_text(&initial.version_id)
    } else {
        String::new()
    });
    if reloaded {
        // NOTE: the region is new with this form, so the keyboard is moved to
        // it once it is in the document, which is an effect on the page alone.
        Effect::new(move |_| focus_report(REPORT_ID));
    }
    let draft = RwSignal::new(initial);
    let refusal = RwSignal::new(None::<FhirError>);
    let checked = RwSignal::new(None::<Result<Validation, FhirError>>);
    let saving = RwSignal::new(false);
    // The letter is read from the draft rather than fixed at build time: a
    // create gives the resource an id, and the press after it is an update.
    let letter = Signal::derive(move || {
        if draft.with(|draft| draft.id.is_empty()) {
            Letter::Create
        } else {
            Letter::Update
        }
    });
    let readonly = Signal::derive(move || {
        standing.backing.get() != Backing::Local
            || !managed
            || !session.can(CODE_SYSTEM, letter.get())
    });

    let notice = standing_section(standing, managed, session, letter, readonly);
    let metadata = metadata_section(draft, readonly, options);
    let properties = properties_section(draft, readonly, options.kinds);
    let concepts = concepts_section(draft, readonly, options.kinds);
    let save = save_section(
        StoredValue::new(client),
        version,
        draft,
        readonly,
        saving,
        report,
        refusal,
        checked,
        session,
        canonical,
    );
    let press = Arc::new(move || {
        let named = canonical.get_value();
        let address = editing_link(CODE_SYSTEM, &named, "", version.get_untracked());
        reload.press(&named, address);
    });
    let outcome = outcome_section(press, report, refusal, checked);
    let versions = history_offer(
        CODE_SYSTEM,
        gated(Box::new(move || draft.read().id.clone())),
        version,
    );

    ferrohealth_ui::view::surface::stack(view! {
        {notice}
        {versions}
        {outcome}
        {metadata}
        {properties}
        {concepts}
        {save}
    })
    .into_any()
}

/// Why the form is read-only, when it is.
///
/// A control that is not there needs a reason beside it, or a reader is left
/// guessing whether the screen is broken.
fn standing_section(
    standing: Standing,
    managed: bool,
    session: Session,
    letter: Signal<Letter>,
    readonly: Signal<bool>,
) -> AnyView {
    let because = move || match standing.backing.get() {
        Backing::Reading => "Reading what this server can do with this code system.",
        Backing::Unreadable => {
            "This server's terminology capabilities did not read, so whether an artifact backs this code system is unknown, and the screen does not offer to change what it cannot place."
        }
        Backing::Artifact => {
            "This code system is served from a built index, which has no write path. It is shown as the server holds it."
        }
        Backing::Local if !managed => {
            "The server states no version for this resource, so there is nothing an update could state in If-Match."
        }
        Backing::Local if session.can(CODE_SYSTEM, letter.get()) => "",
        Backing::Local => {
            "This account carries no permission to change a code system. Sign in with one that does."
        }
    };
    view! {
        <Show when=move || readonly.get() fallback=|| ()>
            <p class=format!(
                "mt-default rounded-md p-default {}",
                styles::NOTICE,
            )>"Read only. " {because}</p>
            {move || {
                standing
                    .capability
                    .with(|held| {
                        held.as_ref()
                            .map(|error| view! { <Failure error=error.clone() /> }.into_any())
                    })
            }}
        </Show>
    }
    .into_any()
}

/// The live region, the refusal, and the check a save ran.
fn outcome_section(
    press: Arc<dyn Fn() + Send + Sync>,
    report: RwSignal<String>,
    refusal: RwSignal<Option<FhirError>>,
    checked: RwSignal<Option<Result<Validation, FhirError>>>,
) -> AnyView {
    // The region is in the document whether or not it has anything to say,
    // because a region inserted along with its first message is not announced
    // (<https://www.w3.org/TR/wai-aria-1.2/#aria-live>).
    let region = view! {
        <p
            id=REPORT_ID
            tabindex="-1"
            aria-live="polite"
            // An empty region keeps its place in the accessibility tree and
            // takes none on the page, so it still announces what arrives.
            class=move || {
                if report.with(String::is_empty) {
                    "fui-sr-only".to_owned()
                } else {
                    styles::MUTED.to_owned()
                }
            }
        >
            {move || report.get()}
        </p>
    }
    .into_any();

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

    let check = view! {
        {move || {
            checked
                .with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer {
                            Ok(validation) => validation_view(validation),
                            Err(error) => view! { <Failure error=error.clone() /> }.into_any(),
                        })
                })
        }}
    }
    .into_any();

    view! {
        {region}
        {refused}
        {check}
    }
    .into_any()
}

/// What `$validate-code` said about the concept a save retired.
fn validation_view(validation: &Validation) -> AnyView {
    let code = validation.code.clone().unwrap_or_default();
    let inactive = validation.inactive;
    let status = validation.status.clone().unwrap_or_default();
    let message = validation.message.clone().unwrap_or_default();
    view! {
        <section class="fui-card" aria-labelledby="editor-check-heading">
            <h2 id="editor-check-heading" class="fui-card-title">
                "What the server says about the retired concept"
            </h2>
            <p class="fui-muted">
                "CodeSystem/$validate-code, run against the version that was just saved."
            </p>
            <table class=format!("mt-default {}", styles::TABLE)>
                <thead>
                    <tr>
                        <th scope="col" class=styles::TH>
                            "Parameter"
                        </th>
                        <th scope="col" class=styles::TH>
                            "Value"
                        </th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <th scope="row" class=styles::TH>
                            "code"
                        </th>
                        <td class=styles::TD>
                            <span class=styles::CODE>{code}</span>
                        </td>
                    </tr>
                    <tr>
                        <th scope="row" class=styles::TH>
                            "inactive"
                        </th>
                        <td class=styles::TD>
                            {match inactive {
                                Some(true) => "true",
                                Some(false) => "false",
                                None => "the server stated none",
                            }}
                        </td>
                    </tr>
                    <tr>
                        <th scope="row" class=styles::TH>
                            "status"
                        </th>
                        <td class=styles::TD>{status}</td>
                    </tr>
                    <tr>
                        <th scope="row" class=styles::TH>
                            "message"
                        </th>
                        <td class=styles::TD>{message}</td>
                    </tr>
                </tbody>
            </table>
        </section>
    }
    .into_any()
}

/// The metadata every request about this code system names it by.
fn metadata_section(draft: RwSignal<Draft>, readonly: Signal<bool>, options: Options) -> AnyView {
    let seed = draft.read_untracked().clone();
    let url = seed.url.clone();
    let business = seed.version.clone();
    view! {
        <section class="fui-card" aria-labelledby="editor-metadata-heading">
            <h2 id="editor-metadata-heading" class="fui-card-title">
                "Metadata"
            </h2>
            <div class="mt-default grid gap-default sm:grid-cols-2">
                <div class="grid gap-tight">
                    <label for="editor-url" class=styles::LABEL>
                        "Canonical (url)"
                    </label>
                    <input
                        id="editor-url"
                        name="url"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=url
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            draft.update(|draft| draft.url = typed);
                        }
                    />
                </div>
                <div class="grid gap-tight">
                    <label for="editor-version" class=styles::LABEL>
                        "Business version"
                    </label>
                    <input
                        id="editor-version"
                        name="version"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=business
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            draft.update(|draft| draft.version = typed);
                        }
                    />
                </div>
                {coded_control(
                    Control {
                        id: String::from("editor-status"),
                        name: "status",
                        label: fixed("Publication status"),
                        sr_only: false,
                    },
                    options.statuses,
                    readonly,
                    gated(Box::new(move || draft.read().status.clone())),
                    Box::new(move |chosen| draft.update(|draft| draft.status = chosen)),
                )}
                {coded_control(
                    Control {
                        id: String::from("editor-content"),
                        name: "content",
                        label: fixed("Content mode"),
                        sr_only: false,
                    },
                    options.contents,
                    readonly,
                    gated(Box::new(move || draft.read().content.clone())),
                    Box::new(move |chosen| draft.update(|draft| draft.content = chosen)),
                )}
                <div class="flex items-center gap-default">
                    <input
                        id="editor-case-sensitive"
                        name="caseSensitive"
                        type="checkbox"
                        class="h-4 w-4 accent-accent"
                        disabled=move || readonly.get()
                        prop:checked=move || draft.read().case_sensitive
                        on:change:target=move |event| {
                            let ticked = event.target().checked();
                            draft.update(|draft| draft.case_sensitive = ticked);
                        }
                    />
                    <label for="editor-case-sensitive" class=styles::LABEL>
                        "Codes are case sensitive"
                    </label>
                </div>
            </div>
        </section>
    }
    .into_any()
}

/// The properties this code system declares.
fn properties_section(draft: RwSignal<Draft>, readonly: Signal<bool>, kinds: Codes) -> AnyView {
    let keys = Memo::new(move |_| {
        draft
            .read()
            .properties
            .iter()
            .map(|property| property.key)
            .collect::<Vec<Key>>()
    });
    view! {
        <section class="fui-card" aria-labelledby="editor-properties-heading">
            <h2 id="editor-properties-heading" class="fui-card-title">
                "Declared properties"
            </h2>
            <p class="fui-muted">
                "The lifecycle properties are declared by the editor on every save, so they are not listed here."
            </p>
            <table class=format!("mt-default {}", styles::TABLE)>
                <thead>
                    <tr>
                        <th scope="col" class=styles::TH>
                            "Code"
                        </th>
                        <th scope="col" class=styles::TH>
                            "Identifier (uri)"
                        </th>
                        <th scope="col" class=styles::TH>
                            "Type"
                        </th>
                        <th scope="col" class=styles::TH>
                            "Remove"
                        </th>
                    </tr>
                </thead>
                <tbody>
                    <For each=move || keys.get() key=|key| *key let:key>
                        {property_row(draft, key, readonly, kinds)}
                    </For>
                </tbody>
            </table>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="mt-default">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    let key = draft.keys.next();
                                    draft
                                        .properties
                                        .push(Declared {
                                            key,
                                            ..Declared::default()
                                        });
                                });
                        }
                    >
                        "Declare a property"
                    </button>
                </p>
            </Show>
        </section>
    }
    .into_any()
}

/// One declared property.
///
/// The text controls are seeded with the value the row was built from and keep
/// the model in step on every keystroke. They are seeded rather than driven,
/// because the row is the only writer of its own fields and a DOM property set
/// from the same signal on every keystroke moves the caret to the end of the
/// text (<https://html.spec.whatwg.org/multipage/input.html#dom-input-value>).
fn property_row(draft: RwSignal<Draft>, key: Key, readonly: Signal<bool>, kinds: Codes) -> AnyView {
    let seed = property_seed(draft, key);
    let id = key.0;
    view! {
        <tr>
            <td class=styles::TD>
                <label for=format!("property-{id}-code") class="sr-only">
                    "Property code"
                </label>
                <input
                    id=format!("property-{id}-code")
                    name="property-code"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.code
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_property(draft, key, |property| property.code = typed);
                    }
                />
            </td>
            <td class=styles::TD>
                <label for=format!("property-{id}-uri") class="sr-only">
                    "Property identifier"
                </label>
                <input
                    id=format!("property-{id}-uri")
                    name="property-uri"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.uri
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_property(draft, key, |property| property.uri = typed);
                    }
                />
            </td>
            <td class=styles::TD>
                {coded_control(
                    Control {
                        id: format!("property-{id}-type"),
                        name: "property-type",
                        label: fixed("Property type"),
                        sr_only: true,
                    },
                    kinds,
                    readonly,
                    gated(
                        Box::new(move || property_of(draft, key, |property| property.kind.clone())),
                    ),
                    Box::new(move |chosen| with_property(
                        draft,
                        key,
                        |property| property.kind = chosen,
                    )),
                )}
            </td>
            <td class=styles::TD>
                <Show when=move || !readonly.get() fallback=|| ()>
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    draft.properties.retain(|property| property.key != key);
                                });
                        }
                    >
                        "Remove"
                    </button>
                </Show>
            </td>
        </tr>
    }
    .into_any()
}

/// Reads one declared property inside the guard, without cloning the rest.
///
/// The read is tracked, so a control drawn from it redraws when the row
/// changes; `project` takes only what the control needs, so a keystroke
/// anywhere in the form does not clone a row per control. A row's own text
/// control is seeded from [`property_seed`] instead, which is untracked: a
/// seed that tracked would re-run the row's view on every keystroke.
fn property_of<T: Default>(
    draft: RwSignal<Draft>,
    key: Key,
    project: impl Fn(&Declared) -> T,
) -> T {
    draft.with(|draft| {
        draft
            .properties
            .iter()
            .find(|property| property.key == key)
            .map_or_else(T::default, project)
    })
}

/// The declared property `key` names, read without subscribing to the draft.
fn property_seed(draft: RwSignal<Draft>, key: Key) -> Declared {
    draft.with_untracked(|draft| {
        draft
            .properties
            .iter()
            .find(|property| property.key == key)
            .cloned()
            .unwrap_or_default()
    })
}

/// Applies `change` to the declared property `key` names.
fn with_property(draft: RwSignal<Draft>, key: Key, change: impl FnOnce(&mut Declared)) {
    draft.update(|draft| {
        if let Some(property) = draft
            .properties
            .iter_mut()
            .find(|property| property.key == key)
        {
            change(property);
        }
    });
}

/// The concepts this code system defines.
fn concepts_section(draft: RwSignal<Draft>, readonly: Signal<bool>, kinds: Codes) -> AnyView {
    let keys = Memo::new(move |_| {
        draft
            .read()
            .concepts
            .iter()
            .map(|concept| concept.key)
            .collect::<Vec<Key>>()
    });
    view! {
        <section class="fui-card" aria-labelledby="editor-concepts-heading">
            <h2 id="editor-concepts-heading" class="fui-card-title">
                "Concepts"
            </h2>
            <p class="fui-muted">
                "A concept is retired rather than removed, so a code that has been published keeps meaning what it meant."
            </p>
            <div class="mt-default grid gap-default">
                <For each=move || keys.get() key=|key| *key let:key>
                    {concept_row(draft, key, readonly, kinds)}
                </For>
            </div>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="mt-default">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    let key = draft.keys.next();
                                    draft
                                        .concepts
                                        .push(Concept {
                                            key,
                                            ..Concept::default()
                                        });
                                });
                        }
                    >
                        "Add a concept"
                    </button>
                </p>
            </Show>
        </section>
    }
    .into_any()
}

/// One concept: what it is, what it is called, and where it is in its life.
fn concept_row(draft: RwSignal<Draft>, key: Key, readonly: Signal<bool>, kinds: Codes) -> AnyView {
    let seed = concept_seed(draft, key);
    let id = key.0;
    let named = move || {
        let code = concept_of(draft, key, |concept| concept.code.clone());
        if code.trim().is_empty() {
            "A concept with no code yet".to_owned()
        } else {
            code
        }
    };
    let designations = designations_view(draft, key, readonly);
    let values = values_view(draft, key, readonly, kinds);
    let lifecycle = lifecycle_view(draft, key, readonly);
    view! {
        <fieldset class="fui-fieldset fui-well">
            <legend>{named}</legend>
            <div class="grid gap-default sm:grid-cols-2">
                <div class="grid gap-tight">
                    <label for=format!("concept-{id}-code") class=styles::LABEL>
                        "Code"
                    </label>
                    <input
                        id=format!("concept-{id}-code")
                        name="concept-code"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=seed.code
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            with_concept(draft, key, |concept| concept.code = typed);
                        }
                    />
                </div>
                <div class="grid gap-tight">
                    <label for=format!("concept-{id}-display") class=styles::LABEL>
                        "Display"
                    </label>
                    <input
                        id=format!("concept-{id}-display")
                        name="concept-display"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=seed.display
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            with_concept(draft, key, |concept| concept.display = typed);
                        }
                    />
                </div>
            </div>
            <div class="mt-default grid gap-tight">
                <label for=format!("concept-{id}-definition") class=styles::LABEL>
                    "Definition"
                </label>
                <textarea
                    id=format!("concept-{id}-definition")
                    name="concept-definition"
                    rows="2"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_concept(draft, key, |concept| concept.definition = typed);
                    }
                >
                    {seed.definition}
                </textarea>
            </div>
            {lifecycle}
            {designations}
            {values}
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="mt-default">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            with_concept(
                                draft,
                                key,
                                |concept| {
                                    concept.lifecycle = Lifecycle::Retired {
                                        on: concept.lifecycle.on().to_owned(),
                                    };
                                },
                            );
                        }
                    >
                        "Retire "
                        {named}
                    </button>
                </p>
            </Show>
        </fieldset>
    }
    .into_any()
}

/// The lifecycle control of one concept, and the date the state takes.
fn lifecycle_view(draft: RwSignal<Draft>, key: Key, readonly: Signal<bool>) -> AnyView {
    let id = key.0;
    // Seeded rather than driven, for the reason `property_row` records: this
    // control is the only writer of its own text, and a DOM property set from
    // the same signal on every keystroke moves the caret to the end of it.
    let seed = concept_seed(draft, key).lifecycle.on().to_owned();
    let state = Signal::derive(move || concept_of(draft, key, |concept| concept.lifecycle.clone()));
    let dated = move || state.get().dated();
    let options: Vec<AnyView> = LIFECYCLE_STATES
        .into_iter()
        .map(|(code, label)| view! { <option value=code>{label}</option> }.into_any())
        .collect();
    view! {
        <div class="mt-default grid gap-default sm:grid-cols-2">
            <div class="grid gap-tight">
                <label for=format!("concept-{id}-lifecycle") class=styles::LABEL>
                    "Lifecycle"
                </label>
                <select
                    id=format!("concept-{id}-lifecycle")
                    name="concept-lifecycle"
                    class=ferrohealth_ui::view::form::SELECT
                    disabled=move || readonly.get()
                    prop:value=move || state.get().status().to_owned()
                    on:change=move |event: Event| {
                        let chosen = event_target_value(&event);
                        with_concept(
                            draft,
                            key,
                            |concept| {
                                let on = concept.lifecycle.on().to_owned();
                                concept.lifecycle = Lifecycle::of(&chosen, &on);
                            },
                        );
                    }
                >
                    {options}
                </select>
            </div>
            <div class="grid gap-tight" hidden=move || !dated()>
                <label for=format!("concept-{id}-lifecycle-date") class=styles::LABEL>
                    "On"
                </label>
                <input
                    id=format!("concept-{id}-lifecycle-date")
                    name="concept-lifecycle-date"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_concept(
                            draft,
                            key,
                            |concept| {
                                concept.lifecycle = Lifecycle::of(
                                    concept.lifecycle.status(),
                                    &typed,
                                );
                            },
                        );
                    }
                />
            </div>
        </div>
    }
    .into_any()
}

/// The designations of one concept.
fn designations_view(draft: RwSignal<Draft>, concept: Key, readonly: Signal<bool>) -> AnyView {
    let keys = Memo::new(move |_| {
        concept_of(draft, concept, |held| {
            held.designations
                .iter()
                .map(|designation| designation.key)
                .collect::<Vec<Key>>()
        })
    });
    view! {
        <div class="mt-default grid gap-tight">
            <p class=styles::EYEBROW>"Designations"</p>
            <For each=move || keys.get() key=|key| *key let:key>
                {designation_row(draft, concept, key, readonly)}
            </For>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p>
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    let key = draft.keys.next();
                                    if let Some(held) = draft
                                        .concepts
                                        .iter_mut()
                                        .find(|held| held.key == concept)
                                    {
                                        held.designations
                                            .push(Designation {
                                                key,
                                                ..Designation::default()
                                            });
                                    }
                                });
                        }
                    >
                        "Add a designation"
                    </button>
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// One designation: its language, what kind of term it is, and the term.
fn designation_row(
    draft: RwSignal<Draft>,
    concept: Key,
    key: Key,
    readonly: Signal<bool>,
) -> AnyView {
    let seed = concept_seed(draft, concept)
        .designations
        .into_iter()
        .find(|designation| designation.key == key)
        .unwrap_or_default();
    let id = key.0;
    view! {
        <div class="grid gap-default sm:grid-cols-[1fr_1fr_2fr_auto]">
            <div class="grid gap-tight">
                <label for=format!("designation-{id}-language") class=styles::LABEL>
                    "Language"
                </label>
                <input
                    id=format!("designation-{id}-language")
                    name="designation-language"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.language
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_designation(
                            draft,
                            concept,
                            key,
                            |designation| designation.language = typed,
                        );
                    }
                />
            </div>
            <div class="grid gap-tight">
                <label for=format!("designation-{id}-use") class=styles::LABEL>
                    "Use (system|code)"
                </label>
                <input
                    id=format!("designation-{id}-use")
                    name="designation-use"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.usage
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_designation(
                            draft,
                            concept,
                            key,
                            |designation| designation.usage = typed,
                        );
                    }
                />
            </div>
            <div class="grid gap-tight">
                <label for=format!("designation-{id}-value") class=styles::LABEL>
                    "Term"
                </label>
                <input
                    id=format!("designation-{id}-value")
                    name="designation-value"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.value
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_designation(
                            draft,
                            concept,
                            key,
                            |designation| designation.value = typed,
                        );
                    }
                />
            </div>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="self-end">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            with_concept(
                                draft,
                                concept,
                                |held| {
                                    held.designations.retain(|designation| designation.key != key);
                                },
                            );
                        }
                    >
                        "Remove"
                    </button>
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// Applies `change` to one designation of one concept.
fn with_designation(
    draft: RwSignal<Draft>,
    concept: Key,
    key: Key,
    change: impl FnOnce(&mut Designation),
) {
    with_concept(draft, concept, |held| {
        if let Some(designation) = held
            .designations
            .iter_mut()
            .find(|designation| designation.key == key)
        {
            change(designation);
        }
    });
}

/// The property values of one concept.
fn values_view(
    draft: RwSignal<Draft>,
    concept: Key,
    readonly: Signal<bool>,
    kinds: Codes,
) -> AnyView {
    let keys = Memo::new(move |_| {
        concept_of(draft, concept, |held| {
            held.values
                .iter()
                .map(|valued| valued.key)
                .collect::<Vec<Key>>()
        })
    });
    view! {
        <div class="mt-default grid gap-tight">
            <p class=styles::EYEBROW>"Property values"</p>
            <For each=move || keys.get() key=|key| *key let:key>
                {value_row(draft, concept, key, readonly, kinds)}
            </For>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p>
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    let key = draft.keys.next();
                                    if let Some(held) = draft
                                        .concepts
                                        .iter_mut()
                                        .find(|held| held.key == concept)
                                    {
                                        held.values.push(Valued { key, ..Valued::default() });
                                    }
                                });
                        }
                    >
                        "Add a property value"
                    </button>
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// One property value: which declared property, and the value itself.
fn value_row(
    draft: RwSignal<Draft>,
    concept: Key,
    key: Key,
    readonly: Signal<bool>,
    kinds: Codes,
) -> AnyView {
    let seed = concept_seed(draft, concept)
        .values
        .into_iter()
        .find(|valued| valued.key == key)
        .unwrap_or_default();
    let id = key.0;
    let declared = gated(Box::new(move || {
        let code = concept_of(draft, concept, |held| {
            held.values
                .iter()
                .find(|valued| valued.key == key)
                .map(|valued| valued.code.clone())
                .unwrap_or_default()
        });
        draft.with(|draft| {
            draft
                .properties
                .iter()
                .find(|property| property.code == code)
                .map_or_else(|| "string".to_owned(), |property| property.kind.clone())
        })
    }));
    let kind_of = move || {
        let kind = declared.get();
        kinds.offered.with(|kinds| {
            kinds
                .iter()
                .find(|coded| coded.code == kind)
                .map_or(kind.clone(), |coded| {
                    if coded.display.is_empty() {
                        coded.code.clone()
                    } else {
                        coded.display.clone()
                    }
                })
        })
    };
    view! {
        <div class="grid gap-default sm:grid-cols-[1fr_2fr_auto_auto]">
            <div class="grid gap-tight">
                <label for=format!("value-{id}-code") class=styles::LABEL>
                    "Property"
                </label>
                <input
                    id=format!("value-{id}-code")
                    name="value-code"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.code
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_value(draft, concept, key, |valued| valued.code = typed);
                    }
                />
            </div>
            <div class="grid gap-tight">
                <label for=format!("value-{id}-text") class=styles::LABEL>
                    "Value"
                </label>
                <input
                    id=format!("value-{id}-text")
                    name="value-text"
                    type="text"
                    class=styles::INPUT
                    disabled=move || readonly.get()
                    value=seed.text
                    on:input:target=move |event| {
                        let typed = event.target().value();
                        with_value(draft, concept, key, |valued| valued.text = typed);
                    }
                />
            </div>
            <p class=format!("self-end {}", styles::HINT)>{kind_of}</p>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="self-end">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            with_concept(
                                draft,
                                concept,
                                |held| held.values.retain(|valued| valued.key != key),
                            );
                        }
                    >
                        "Remove"
                    </button>
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// Applies `change` to one property value of one concept.
fn with_value(draft: RwSignal<Draft>, concept: Key, key: Key, change: impl FnOnce(&mut Valued)) {
    with_concept(draft, concept, |held| {
        if let Some(valued) = held.values.iter_mut().find(|valued| valued.key == key) {
            change(valued);
        }
    });
}

/// Reads one concept inside the guard, without cloning the rest.
///
/// The read is tracked, so the legend, the lifecycle control and the nested
/// lists redraw when the concept changes; `project` takes only what the caller
/// draws, so a keystroke does not clone a concept per control on the screen.
/// A row's own text control is seeded from [`concept_seed`] instead.
fn concept_of<T: Default>(draft: RwSignal<Draft>, key: Key, project: impl Fn(&Concept) -> T) -> T {
    draft.with(|draft| {
        draft
            .concepts
            .iter()
            .find(|concept| concept.key == key)
            .map_or_else(T::default, project)
    })
}

/// The concept `key` names, read without subscribing to the draft.
fn concept_seed(draft: RwSignal<Draft>, key: Key) -> Concept {
    draft.with_untracked(|draft| {
        draft
            .concepts
            .iter()
            .find(|concept| concept.key == key)
            .cloned()
            .unwrap_or_default()
    })
}

/// Applies `change` to the concept `key` names.
fn with_concept(draft: RwSignal<Draft>, key: Key, change: impl FnOnce(&mut Concept)) {
    draft.update(|draft| {
        if let Some(concept) = draft.concepts.iter_mut().find(|concept| concept.key == key) {
            change(concept);
        }
    });
}

/// The control that sends the whole resource, and what it reports.
#[expect(
    clippy::too_many_arguments,
    reason = "the control reports into five signals and one stored value of the form it belongs to, and threading them through a struct would only rename the same arguments"
)]
fn save_section(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<Draft>,
    readonly: Signal<bool>,
    saving: RwSignal<bool>,
    report: RwSignal<String>,
    refusal: RwSignal<Option<FhirError>>,
    checked: RwSignal<Option<Result<Validation, FhirError>>>,
    session: Session,
    canonical: StoredValue<String>,
) -> AnyView {
    let send = move |_| {
        if saving.get_untracked() || readonly.get_untracked() {
            return;
        }
        let held = draft.get_untracked();
        if !held.savable() {
            report.set(String::from(
                "A code system needs a canonical, a status, and a content mode before it can be saved.",
            ));
            return;
        }
        saving.set(true);
        refusal.set(None);
        checked.set(None);
        report.set(String::from("Saving"));
        let client = client.get_value();
        let version = version.get_untracked();
        spawn_local(async move {
            send_it(
                &client,
                version,
                &held,
                Reports {
                    draft,
                    report,
                    refusal,
                    checked,
                    session,
                    canonical,
                },
            )
            .await;
            saving.set(false);
        });
    };
    view! {
        <Show when=move || !readonly.get() fallback=|| ()>
            <section class="fui-card" aria-labelledby="editor-save-heading">
                <h2 id="editor-save-heading" class="fui-card-title">
                    "Save"
                </h2>
                <p class="fui-muted">
                    "An update states the version it replaces, so a change made elsewhere since this form was opened is refused rather than overwritten."
                </p>
                <p class="mt-default">
                    <button
                        type="button"
                        class=styles::SUBMIT
                        aria-describedby=REPORT_ID
                        disabled=move || saving.get()
                        on:click=send
                    >
                        {move || {
                            if draft.read().id.is_empty() {
                                "Create this code system"
                            } else {
                                "Save this code system"
                            }
                        }}
                    </button>
                </p>
            </section>
        </Show>
    }
    .into_any()
}

/// What the live region says after a save the server took.
fn saved_text(version_id: &str) -> String {
    if version_id.is_empty() {
        String::from("Saved. The server stated no version for it.")
    } else {
        format!("Saved. The server now holds version {version_id}.")
    }
}

/// Where a save writes what it did, so one argument carries them together.
#[derive(Clone, Copy)]
struct Reports {
    /// The draft, which a save that was taken gives an id and a version.
    draft: RwSignal<Draft>,
    /// The live region.
    report: RwSignal<String>,
    /// The refusal the screen renders whole.
    refusal: RwSignal<Option<FhirError>>,
    /// The check a save that retired something runs afterwards.
    checked: RwSignal<Option<Result<Validation, FhirError>>>,
    /// The session, so a spent token is dropped.
    session: Session,
    /// The canonical the server now holds the resource under.
    canonical: StoredValue<String>,
}

/// Sends one save and writes down what came back.
async fn send_it(client: &FhirClient, version: FhirVersion, held: &Draft, into: Reports) {
    let body = held.body();
    let written = if held.id.is_empty() {
        Box::pin(client.create(version, CODE_SYSTEM, &body)).await
    } else {
        Box::pin(client.update(
            version,
            CODE_SYSTEM,
            &held.id,
            &body,
            Some(held.version_id.as_str()).filter(|version| !version.is_empty()),
        ))
        .await
    };
    let written = match written {
        Ok(written) => written,
        Err(error) => {
            let refused = Refusal::of(&error);
            if refused.drops_the_token() {
                into.session.release();
            }
            into.report.set(announced(&error));
            into.refusal.set(Some(error));
            return;
        }
    };
    into.canonical.set_value(held.url.clone());
    let assigned = written
        .resource
        .as_ref()
        .and_then(|resource| resource.id.clone())
        .unwrap_or_else(|| held.id.clone());
    let at = written.version_id().unwrap_or_default().to_owned();
    into.draft.update(|draft| {
        draft.id = assigned;
        draft.version_id.clone_from(&at);
    });
    into.report.set(saved_text(&at));

    let request = into.draft.with_untracked(|draft| {
        draft.retired().map(|retired| ValidateRequest {
            url: draft.url.clone(),
            code: retired.code.clone(),
            ..ValidateRequest::default()
        })
    });
    let Some(request) = request else {
        return;
    };
    let answer = Box::pin(client.validate_code(version, &request))
        .await
        .map(|parameters| parameters.validation());
    // The check is the point of a save that retired something, so what it said
    // is announced rather than only drawn.
    into.report.update(|said| {
        said.push(' ');
        said.push_str(&checked_text(&request.code, answer.as_ref()));
    });
    into.checked.set(Some(answer));
}

/// What the live region says about the check a save ran.
fn checked_text(code: &str, answer: Result<&Validation, &FhirError>) -> String {
    match answer {
        Err(error) => format!("The check on `{code}` did not answer: {error}"),
        Ok(validation) => match validation.inactive {
            Some(true) => format!("The server reports `{code}` as inactive."),
            Some(false) => format!("The server reports `{code}` as still active."),
            None => format!("The server stated no inactive flag for `{code}`."),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::reload::diagnostics;
    use crate::fhir::outcome::OperationOutcome;

    #[test]
    fn a_new_code_system_and_an_edited_one_title_the_page_apart() {
        assert_eq!(title_of("  "), "New code system");
        assert_eq!(
            title_of("https://terminology.example/colours"),
            "Edit https://terminology.example/colours"
        );
    }

    #[test]
    fn a_save_says_which_version_the_server_now_holds() {
        assert_eq!(saved_text("3"), "Saved. The server now holds version 3.");
        assert_eq!(
            saved_text(""),
            "Saved. The server stated no version for it.",
            "a server that stated none is said to have stated none"
        );
    }

    #[test]
    fn the_check_a_save_ran_is_announced_as_well_as_drawn() {
        let retired = Validation {
            inactive: Some(true),
            ..Validation::default()
        };
        assert_eq!(
            checked_text("vermilion", Ok(&retired)),
            "The server reports `vermilion` as inactive."
        );
        let active = Validation::default();
        assert_eq!(
            checked_text("red", Ok(&active)),
            "The server stated no inactive flag for `red`.",
            "a server that stated none is said to have stated none"
        );
        let error = FhirError::Transport {
            url: String::from("https://tx.example.org/r4b/CodeSystem/$validate-code"),
            message: String::from("network error"),
        };
        assert!(
            checked_text("red", Err(&error)).contains("network error"),
            "a check that did not answer says what stopped it"
        );
    }

    #[test]
    fn the_announcement_carries_the_server_s_own_wording() {
        let outcome: OperationOutcome = serde_json::from_str(
            r#"{"resourceType":"OperationOutcome","issue":[
                 {"severity":"error","code":"invariant",
                  "details":{"text":"A CodeSystem must have a status"}}]}"#,
        )
        .expect("the server's own answer parses");
        let error = FhirError::Refused {
            url: String::from("https://tx.example.org/r4b/CodeSystem"),
            status: http::StatusCode::UNPROCESSABLE_ENTITY,
            outcome,
        };
        assert_eq!(diagnostics(&error), "A CodeSystem must have a status");
    }

    #[test]
    fn the_lifecycle_control_offers_every_standard_state() {
        let offered: Vec<&str> = LIFECYCLE_STATES.into_iter().map(|(code, _)| code).collect();
        for lifecycle in [
            Lifecycle::Active,
            Lifecycle::Experimental,
            Lifecycle::Deprecated { on: String::new() },
            Lifecycle::Retired { on: String::new() },
        ] {
            assert!(
                offered.contains(&lifecycle.status()),
                "{lifecycle:?} is a state the control cannot reach"
            );
        }
    }
}
