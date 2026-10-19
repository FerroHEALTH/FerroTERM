//! Authoring one local `ConceptMap` in the browser, with a translate preview.
//!
//! The screen is the same for a map this deployment loaded and one a person
//! wrote here. What makes the second editable is two facts off the wire and
//! nothing the bundle assumes: the server states a `meta.versionId` for it,
//! which only the REST API stamps
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>), and the token in hand
//! carries the scope for the change. A built or national map states no version,
//! so it opens read-only.
//!
//! Every code on both sides of a mapping is picked out of the system its group
//! names, through the same `ValueSet/$expand` search the concept browser runs,
//! so a target arrives with the display the server gave it rather than typed
//! blind. The preview is `ConceptMap/$translate` with the map itself carried in
//! the `conceptMap` parameter, which is how a map that has never been saved is
//! translated through; a server may refuse a map sent that way
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>), and the
//! screen then falls back to the saved map and says which it used.

use std::sync::Arc;

use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::use_query;
use leptos_router::params::Params;

use crate::auth::Session;
use crate::auth::scopes::Letter;
use crate::authoring::code_system::Key;
use crate::authoring::concept_map::MapDraft;
use crate::authoring::concept_map::MapElement;
use crate::authoring::concept_map::MapGroup;
use crate::authoring::concept_map::MapTarget;
use crate::authoring::concept_map::dialect;
use crate::components::coded::Codes;
use crate::components::coded::Control;
use crate::components::coded::coded_control;
use crate::components::coded::codes_of;
use crate::components::coded::fixed;
use crate::components::coded::gated;
use crate::components::editor_mode;
use crate::components::failure::Failure;
use crate::components::history::history_offer;
use crate::components::icon;
use crate::components::reload::Reload;
use crate::components::reload::announced;
use crate::components::reload::diagnostics;
use crate::components::reload::focus_report;
use crate::components::reload::reload_offer;
use crate::components::reload::reloaded_text;
use crate::components::shell::SelectedVersion;
use crate::fhir::CONCEPT_MAP;
use crate::fhir::FhirClient;
use crate::fhir::concept::ConceptQuery;
use crate::fhir::error::FhirError;
use crate::fhir::expansion::ConceptRow;
use crate::fhir::translate::TranslateAnswer;
use crate::fhir::translate::TranslateRequest;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::Refusal;
use crate::locale::{Msg, message, tr, tr_with};
use crate::routes::editing_link;
use crate::styles;
use ferrohealth_ui::view::empty::{Busy, busy, empty_quiet};
use ferrohealth_ui::view::surface::{Crumb, Header, editor_heading, editor_title};

/// The value set whose codes `ConceptMap.status` is bound to.
///
/// It is the same publication status value set every definitional resource's
/// `status` is bound to, on every served version
/// (<https://hl7.org/fhir/R4B/conceptmap.html>).
const PUBLICATION_STATUS: &str = "http://hl7.org/fhir/ValueSet/publication-status";

/// The live region every outcome of this screen is announced in.
const REPORT_ID: &str = "map-editor-report";

/// Where a picked code and its display go.
///
/// It is a shared function rather than a type parameter, so one picker
/// compiles once for every row that draws one: a generic one is monomorphized
/// per call site, and this screen draws a picker per code and per target.
type Chosen = Arc<dyn Fn(String, String) + Send + Sync>;

/// How many codes one search of a system asks for.
///
/// A picker is a list a person reads, so it asks for a page rather than an
/// expansion (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
const SEARCH_COUNT: u32 = 20;

/// The screen's own query parameters.
#[derive(Clone, Debug, Params, PartialEq)]
struct MapEditorQuery {
    /// The canonical of the concept map being edited, absent for a new one.
    map: Option<String>,
}

/// Authors one local concept map, or shows a loaded one read-only.
///
/// The canonical is read reactively: a link from one map to another matches
/// this same route, and `leptos_router` then updates the query without
/// re-running this body, so a read taken untracked at setup would go stale.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ConceptMapEditorPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let query = use_query::<MapEditorQuery>();
    let canonical = Signal::derive(move || {
        query
            .read()
            .as_ref()
            .ok()
            .and_then(|query| query.map.clone())
            .unwrap_or_default()
    });

    // A reload reruns this read, and the form is rebuilt around what came
    // back, rather than an effect writing the form's own signals.
    let reload = Reload::new(canonical);
    let reader = client.clone();
    let stored = Resource::new(
        move || (version.get(), canonical.get(), reload.generation()),
        move |(version, canonical, _reloads)| {
            let client = reader.clone();
            async move {
                if canonical.trim().is_empty() {
                    return Ok(None);
                }
                client.authored_map(version, &canonical).await
            }
        },
    );

    let statuses = codes_of(
        &client,
        version,
        Signal::derive(|| PUBLICATION_STATUS.to_owned()),
    );
    // The element a target's relationship goes in differs per version, and so
    // does the value set it is bound to, so the control's codes are read for
    // the version the reader is looking through.
    let relationships = codes_of(
        &client,
        version,
        Signal::derive(move || dialect(version.get()).relationship_value_set.to_owned()),
    );

    let mode = move || editor_mode(&canonical.get());
    let heading = view! {
        <Title text=move || editor_title(message(Msg::TitleConceptMapEditor), &mode()) />
        <div>
            {move || {
                editor_heading(
                    message(Msg::TitleConceptMapEditor),
                    &mode(),
                    Header {
                        subtitle: Some(tr(Msg::MetadataSystemsEachGroupMapsBetween)),
                        crumbs: vec![Crumb::new(tr(Msg::ConceptMaps), "/conceptmaps")],
                        ..Header::default()
                    },
                    (),
                )
            }}
        </div>
    }
    .into_any();

    let form = view! {
        <Transition fallback=|| {
            busy(tr(Msg::ReadingTheConceptMap), Busy::Section)
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
                                    let draft = held
                                        .as_ref()
                                        .map_or_else(MapDraft::new, MapDraft::of);
                                    form_section(
                                        client,
                                        version,
                                        draft,
                                        reload,
                                        Options { statuses, relationships },
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

/// The two coded controls' options, so one argument carries them together.
#[derive(Clone, Copy)]
struct Options {
    /// The codes `ConceptMap.status` admits.
    statuses: Codes,
    /// The codes a target's relationship element admits on this version.
    relationships: Codes,
}

/// What one preview ran, and what came back.
#[derive(Clone, Debug)]
struct Preview {
    /// The code the preview ran for, so the answer is drawn under its row.
    element: Key,
    /// Whether the map travelled inline, or the preview fell back to the
    /// saved map because the server would not take it inline.
    inline: bool,
    /// What the server answered.
    answer: Result<TranslateAnswer, FhirError>,
}

/// The whole form over one draft, and everything a save or a preview reports.
///
/// The draft is created here rather than written into from an effect: this
/// function runs inside the closure that reads the resource, so a refetch
/// disposes this form and builds a new one over what came back.
fn form_section(
    client: FhirClient,
    version: Signal<FhirVersion>,
    initial: MapDraft,
    reload: Reload,
    options: Options,
) -> AnyView {
    let session = expect_context::<Session>();
    let managed = initial.managed() || initial.id.is_empty();
    // The canonical the server holds the map under, which is what a reload
    // reads by even when the reader has since typed another.
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
    let preview = RwSignal::new(None::<Preview>);
    let saving = RwSignal::new(false);
    let letter = Signal::derive(move || {
        if draft.with(|draft| draft.id.is_empty()) {
            Letter::Create
        } else {
            Letter::Update
        }
    });
    let readonly = Signal::derive(move || !managed || !session.can(CONCEPT_MAP, letter.get()));
    let held = StoredValue::new(client);

    let notice = standing_section(managed, session, letter, readonly);
    let versions = history_offer(
        CONCEPT_MAP,
        gated(Box::new(move || draft.read().id.clone())),
        version,
    );
    let press = Arc::new(move || {
        let named = canonical.get_value();
        let address = editing_link(CONCEPT_MAP, &named, "", version.get_untracked());
        reload.press(&named, address);
    });
    let outcome = outcome_section(press, report, refusal);
    let metadata = metadata_section(draft, version, readonly, options.statuses);
    let groups = groups_section(held, version, draft, readonly, options, preview, report);
    let save = save_section(
        held, version, draft, readonly, saving, report, refusal, session, canonical,
    );

    ferrohealth_ui::view::surface::stack(view! {
        {notice}
        {versions}
        {outcome}
        {metadata}
        {groups}
        {save}
    })
    .into_any()
}

/// Why the form is read-only, when it is.
///
/// A control that is not there needs a reason beside it, or a reader is left
/// guessing whether the screen is broken.
fn standing_section(
    managed: bool,
    session: Session,
    letter: Signal<Letter>,
    readonly: Signal<bool>,
) -> AnyView {
    let because = move || {
        if !managed {
            tr(Msg::ServerStatesNoVersionResourceSo)
        } else if session.can(CONCEPT_MAP, letter.get()) {
            String::new()
        } else {
            tr(Msg::AccountCarriesNoPermissionChangeConcept)
        }
    };
    view! {
        <Show when=move || readonly.get() fallback=|| ()>
            <p class=format!("mt-default rounded-md p-default {}", styles::NOTICE)>{tr(Msg::ReadOnly)} {because}</p>
        </Show>
    }
    .into_any()
}

/// The live region and the refusal a save met.
fn outcome_section(
    press: Arc<dyn Fn() + Send + Sync>,
    report: RwSignal<String>,
    refusal: RwSignal<Option<FhirError>>,
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
                                held.as_ref().map(|error| tr(Refusal::of(error).what_to_do()))
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

/// The label of a scope control: `msg`, naming the `element` the version in
/// force writes the scope under.
fn scope_label(msg: Msg, element: &str) -> String {
    tr_with(msg, &[("element", element.into())])
}

/// The metadata every request about this concept map names it by.
fn metadata_section(
    draft: RwSignal<MapDraft>,
    version: Signal<FhirVersion>,
    readonly: Signal<bool>,
    statuses: Codes,
) -> AnyView {
    let seed = draft.read_untracked().clone();
    // The element each scope is written under differs per version, so the
    // label says which element this control fills on the root being read.
    let source_label = move || scope_label(Msg::SourceScope, dialect(version.get()).source_scope);
    let target_label = move || scope_label(Msg::TargetScope, dialect(version.get()).target_scope);
    view! {
        <section class="fui-card" aria-labelledby="map-metadata-heading">
            <h2 id="map-metadata-heading" class="fui-card-title">
                {tr(Msg::Metadata)}
            </h2>
            <div class="mt-default grid gap-default sm:grid-cols-2">
                <div class="grid gap-tight">
                    <label for="map-url" class=styles::LABEL>
                        {tr(Msg::CanonicalUrl)}
                    </label>
                    <input
                        id="map-url"
                        name="map-url"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=seed.url
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            draft.update(|draft| draft.url = typed);
                        }
                    />
                </div>
                <div class="grid gap-tight">
                    <label for="map-version" class=styles::LABEL>
                        {tr(Msg::BusinessVersion)}
                    </label>
                    <input
                        id="map-version"
                        name="map-version"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=seed.version
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            draft.update(|draft| draft.version = typed);
                        }
                    />
                </div>
                {coded_control(
                    Control {
                        id: String::from("map-status"),
                        name: "map-status",
                        label: fixed(tr(Msg::PublicationStatus)),
                        sr_only: false,
                    },
                    statuses,
                    readonly,
                    gated(Box::new(move || draft.read().status.clone())),
                    Box::new(move |chosen| draft.update(|draft| draft.status = chosen)),
                )}
                <div class="grid gap-tight">
                    <label for="map-source-scope" class=styles::LABEL>
                        {source_label}
                    </label>
                    <input
                        id="map-source-scope"
                        name="map-source-scope"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=seed.source_scope
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            draft.update(|draft| draft.source_scope = typed);
                        }
                    />
                </div>
                <div class="grid gap-tight">
                    <label for="map-target-scope" class=styles::LABEL>
                        {target_label}
                    </label>
                    <input
                        id="map-target-scope"
                        name="map-target-scope"
                        type="text"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        value=seed.target_scope
                        on:input:target=move |event| {
                            let typed = event.target().value();
                            draft.update(|draft| draft.target_scope = typed);
                        }
                    />
                </div>
            </div>
        </section>
    }
    .into_any()
}

/// The groups this map maps between.
fn groups_section(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    readonly: Signal<bool>,
    options: Options,
    preview: RwSignal<Option<Preview>>,
    report: RwSignal<String>,
) -> AnyView {
    let keys = Memo::new(move |_| {
        draft
            .read()
            .groups
            .iter()
            .map(|group| group.key)
            .collect::<Vec<Key>>()
    });
    view! {
        <section class="fui-card" aria-labelledby="map-groups-heading">
            <h2 id="map-groups-heading" class="fui-card-title">
                {tr(Msg::Groups)}
            </h2>
            <p class="fui-muted">
                {tr(Msg::GroupMapsCodesOneSystemCodes)}
            </p>
            <Show
                when=move || keys.with(|keys| !keys.is_empty())
                fallback=|| empty_quiet(icon::CONCEPT_MAPS, tr(Msg::NoGroupsTitle), tr(Msg::NoGroupsLine))
            >
            <div class="mt-default grid gap-default">
                <For each=move || keys.get() key=|key| *key let:key>
                    {group_panel(client, version, draft, key, readonly, options, preview, report)}
                </For>
            </div>
            </Show>
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
                                        .groups
                                        .push(MapGroup {
                                            key,
                                            ..MapGroup::default()
                                        });
                                });
                        }
                    >
                        {tr(Msg::AddAGroup)}
                    </button>
                </p>
            </Show>
        </section>
    }
    .into_any()
}

/// One group: the two systems, and the codes mapped between them.
#[expect(
    clippy::too_many_arguments,
    reason = "a group draws over the form's own signals, and bundling them into a struct would only rename the same arguments"
)]
fn group_panel(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    key: Key,
    readonly: Signal<bool>,
    options: Options,
    preview: RwSignal<Option<Preview>>,
    report: RwSignal<String>,
) -> AnyView {
    let named = move || {
        let source = group_of(draft, key, |group| group.source.clone());
        let target = group_of(draft, key, |group| group.target.clone());
        if source.trim().is_empty() && target.trim().is_empty() {
            tr(Msg::GroupNoSystemsYet).clone()
        } else {
            tr_with(
                Msg::To2,
                &[
                    ("source", (source).clone().into()),
                    ("target", (target).clone().into()),
                ],
            )
        }
    };
    let elements = elements_view(
        client, version, draft, key, readonly, options, preview, report,
    );
    let systems = systems_view(draft, key, readonly);
    view! {
        <fieldset class="fui-fieldset fui-well">
            <legend>{named}</legend>
            {systems}
            {elements}
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="mt-default flex flex-wrap gap-default">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    let minted = draft.keys.next();
                                    if let Some(group) = draft
                                        .groups
                                        .iter_mut()
                                        .find(|group| group.key == key)
                                    {
                                        group
                                            .elements
                                            .push(MapElement {
                                                key: minted,
                                                ..MapElement::default()
                                            });
                                    }
                                });
                        }
                    >
                        {tr(Msg::AddACode)}
                    </button>
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft.update(|draft| draft.groups.retain(|group| group.key != key));
                        }
                    >
                        {tr(Msg::RemoveThisGroup)}
                    </button>
                </p>
            </Show>
        </fieldset>
    }
    .into_any()
}

/// The two systems one group maps between, and the version of each.
///
/// A group states its systems as a `uri` with a version beside it on R4 and
/// R4B, and as a `canonical` carrying its own version on R5 and R6
/// (<https://hl7.org/fhir/R5/conceptmap.html>), so the form keeps the two
/// apart and the save joins them the way the served version writes them.
fn systems_view(draft: RwSignal<MapDraft>, key: Key, readonly: Signal<bool>) -> AnyView {
    let id = key.0;
    view! {
        <div class="grid gap-default sm:grid-cols-2">
            {driven_control(
                format!("group-{id}-source"),
                "group-source",
                tr(Msg::SourceSystem),
                readonly,
                gated(Box::new(move || group_of(draft, key, |group| group.source.clone()))),
                Box::new(move |typed| with_group(draft, key, |group| group.source = typed)),
            )}
            {driven_control(
                format!("group-{id}-source-version"),
                "group-source-version",
                tr(Msg::SourceSystemVersion),
                readonly,
                gated(Box::new(move || group_of(draft, key, |group| group.source_version.clone()))),
                Box::new(move |typed| with_group(draft, key, |group| group.source_version = typed)),
            )}
            {driven_control(
                format!("group-{id}-target"),
                "group-target",
                tr(Msg::TargetSystem),
                readonly,
                gated(Box::new(move || group_of(draft, key, |group| group.target.clone()))),
                Box::new(move |typed| with_group(draft, key, |group| group.target = typed)),
            )}
            {driven_control(
                format!("group-{id}-target-version"),
                "group-target-version",
                tr(Msg::TargetSystemVersion),
                readonly,
                gated(Box::new(move || group_of(draft, key, |group| group.target_version.clone()))),
                Box::new(move |typed| with_group(draft, key, |group| group.target_version = typed)),
            )}
        </div>
    }
    .into_any()
}

/// The codes one group maps.
#[expect(
    clippy::too_many_arguments,
    reason = "the list draws over the form's own signals, and bundling them into a struct would only rename the same arguments"
)]
fn elements_view(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    group: Key,
    readonly: Signal<bool>,
    options: Options,
    preview: RwSignal<Option<Preview>>,
    report: RwSignal<String>,
) -> AnyView {
    let keys = Memo::new(move |_| {
        group_of(draft, group, |held| {
            held.elements
                .iter()
                .map(|element| element.key)
                .collect::<Vec<Key>>()
        })
    });
    view! {
        <div class="mt-default grid gap-default">
            <p class=styles::EYEBROW>{tr(Msg::Codes)}</p>
            <Show
                when=move || keys.with(Vec::is_empty)
                fallback=|| ()
            >
                {empty_quiet(icon::CONCEPT_MAPS, tr(Msg::NoElementsTitle), tr(Msg::NoElementsLine))}
            </Show>
            <For each=move || keys.get() key=|key| *key let:key>
                {element_panel(
                    client,
                    version,
                    draft,
                    group,
                    key,
                    readonly,
                    options,
                    preview,
                    report,
                )}
            </For>
        </div>
    }
    .into_any()
}

/// One text control whose value the model owns.
///
/// It is driven rather than seeded, because two things write it: the reader,
/// and the picker beside it. The value setter moves the caret to the end only
/// when the new value differs from the old
/// (<https://html.spec.whatwg.org/multipage/input.html#dom-input-value>), so
/// typing is unaffected and only a search result filling the field moves it.
fn driven_control(
    id: String,
    name: &'static str,
    label: String,
    readonly: Signal<bool>,
    held: Signal<String>,
    mut typed: Box<dyn FnMut(String)>,
) -> AnyView {
    let named = StoredValue::new(id);
    view! {
        <div class="grid gap-tight">
            <label for=move || named.with_value(Clone::clone) class=styles::LABEL>
                {label}
            </label>
            <input
                id=move || named.with_value(Clone::clone)
                name=name
                type="text"
                class=styles::INPUT
                disabled=move || readonly.get()
                prop:value=move || held.get()
                on:input:target=move |event| typed(event.target().value())
            />
        </div>
    }
    .into_any()
}

/// One source code: what it is, what it maps to, and what `$translate` says.
#[expect(
    clippy::too_many_arguments,
    reason = "the row draws over the form's own signals, and bundling them into a struct would only rename the same arguments"
)]
fn element_panel(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    group: Key,
    key: Key,
    readonly: Signal<bool>,
    options: Options,
    preview: RwSignal<Option<Preview>>,
    report: RwSignal<String>,
) -> AnyView {
    let id = key.0;
    let source_system = gated(Box::new(move || {
        group_of(draft, group, |held| held.source.clone())
    }));
    // The flag is the version's as well as the concept's: a map authored on R5
    // and read through an R4B root still carries `noMap`, and a version that
    // defines no such element must not hide the targets it does define.
    let unmapped = move || {
        dialect(version.get()).no_map && element_of(draft, group, key, |element| element.no_map)
    };
    let targets = targets_view(
        client,
        version,
        draft,
        group,
        key,
        readonly,
        options.relationships,
    );
    let picked = picker(
        client,
        version,
        format!("element-{id}"),
        &tr(Msg::SourceCode2),
        source_system,
        readonly,
        Arc::new(move |code, display| {
            with_element(draft, group, key, |element| {
                element.code = code;
                element.display = display;
            });
        }),
    );
    let no_map = no_map_control(draft, version, group, key, readonly);
    let fields = element_fields(draft, group, key, readonly);
    view! {
        <fieldset class="fui-fieldset fui-well">
            <legend>
                {move || {
                    let code = element_of(draft, group, key, |element| element.code.clone());
                    if code.trim().is_empty() { tr(Msg::CodeNotChosenYet).clone() } else { code }
                }}
            </legend>
            {fields}
            {picked}
            {no_map}
            <div hidden=unmapped>{targets}</div>
            <p class="mt-default">
                <button
                    type="button"
                    class=styles::BUTTON
                    aria-describedby=REPORT_ID
                    on:click=move |_| {
                        run_preview(client, version, draft, group, key, preview, report);
                    }
                >
                    {tr(Msg::PreviewCodeThroughTranslate)}
                </button>
            </p>
            {preview_view(preview, key)}
            <Show when=move || !readonly.get() fallback=|| ()>
                <p class="mt-default">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            with_group(
                                draft,
                                group,
                                |held| held.elements.retain(|element| element.key != key),
                            );
                        }
                    >
                        {tr(Msg::RemoveThisCode)}
                    </button>
                </p>
            </Show>
        </fieldset>
    }
    .into_any()
}

/// One code's own two fields.
fn element_fields(
    draft: RwSignal<MapDraft>,
    group: Key,
    key: Key,
    readonly: Signal<bool>,
) -> AnyView {
    let id = key.0;
    view! {
        <div class="grid gap-default sm:grid-cols-2">
            {driven_control(
                format!("element-{id}-code"),
                "element-code",
                tr(Msg::Code),
                readonly,
                gated(Box::new(move || element_of(draft, group, key, |held| held.code.clone()))),
                Box::new(move |code| with_element(draft, group, key, |held| held.code = code)),
            )}
            {driven_control(
                format!("element-{id}-display"),
                "element-display",
                tr(Msg::Display),
                readonly,
                gated(Box::new(move || element_of(draft, group, key, |held| held.display.clone()))),
                Box::new(move |display| with_element(
                    draft,
                    group,
                    key,
                    |held| held.display = display,
                )),
            )}
        </div>
    }
    .into_any()
}

/// The control that says a code maps to nothing, where the version has one.
///
/// `element.noMap` arrived in R5, and `cmd-4` makes it an error to carry both
/// it and a target (<https://hl7.org/fhir/R5/conceptmap.html>), so the targets
/// beside it are hidden while it is ticked.
fn no_map_control(
    draft: RwSignal<MapDraft>,
    version: Signal<FhirVersion>,
    group: Key,
    key: Key,
    readonly: Signal<bool>,
) -> AnyView {
    let id = key.0;
    let unmapped = move || element_of(draft, group, key, |element| element.no_map);
    view! {
        <Show when=move || dialect(version.get()).no_map fallback=|| ()>
            <div class="mt-default flex items-center gap-default">
                <input
                    id=format!("element-{id}-nomap")
                    name="element-nomap"
                    type="checkbox"
                    class="h-4 w-4 accent-accent"
                    disabled=move || readonly.get()
                    prop:checked=unmapped
                    on:change:target=move |event| {
                        let ticked = event.target().checked();
                        with_element(draft, group, key, |element| element.no_map = ticked);
                    }
                />
                <label for=format!("element-{id}-nomap") class=styles::LABEL>
                    {tr(Msg::CodeMapsNothingNomap)}
                </label>
            </div>
        </Show>
    }
    .into_any()
}

/// The targets one source code maps to.
fn targets_view(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    group: Key,
    element: Key,
    readonly: Signal<bool>,
    relationships: Codes,
) -> AnyView {
    let keys = Memo::new(move |_| {
        element_of(draft, group, element, |held| {
            held.targets
                .iter()
                .map(|target| target.key)
                .collect::<Vec<Key>>()
        })
    });
    view! {
        <div class="mt-default grid gap-default">
            <p class=styles::EYEBROW>{tr(Msg::MapsTo)}</p>
            <For each=move || keys.get() key=|key| *key let:key>
                {target_row(client, version, draft, group, element, key, readonly, relationships)}
            </For>
            <Show when=move || !readonly.get() fallback=|| ()>
                <p>
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            draft
                                .update(|draft| {
                                    let minted = draft.keys.next();
                                    if let Some(held) = draft
                                        .groups
                                        .iter_mut()
                                        .find(|held| held.key == group)
                                        .and_then(|held| {
                                            held.elements.iter_mut().find(|held| held.key == element)
                                        })
                                    {
                                        held.targets
                                            .push(MapTarget {
                                                key: minted,
                                                ..MapTarget::default()
                                            });
                                    }
                                });
                        }
                    >
                        {tr(Msg::AddATarget)}
                    </button>
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// One target: the code it maps to, how it relates, and why.
#[expect(
    clippy::too_many_arguments,
    reason = "the row draws over the form's own signals, and bundling them into a struct would only rename the same arguments"
)]
fn target_row(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    group: Key,
    element: Key,
    key: Key,
    readonly: Signal<bool>,
    relationships: Codes,
) -> AnyView {
    let id = key.0;
    let target_system = gated(Box::new(move || {
        group_of(draft, group, |held| held.target.clone())
    }));
    let picked = picker(
        client,
        version,
        format!("target-{id}"),
        &tr(Msg::TargetCode2),
        target_system,
        readonly,
        Arc::new(move |code, display| {
            with_target(draft, group, element, key, |target| {
                target.code = code;
                target.display = display;
            });
        }),
    );
    let fields = target_fields(draft, group, element, key, readonly, relationships, version);
    view! {
        <div class=format!("p-default {}", styles::PANEL)>
            {fields} {picked} <Show when=move || !readonly.get() fallback=|| ()>
                <p class="mt-default">
                    <button
                        type="button"
                        class=styles::BUTTON
                        on:click=move |_| {
                            with_element(
                                draft,
                                group,
                                element,
                                |held| held.targets.retain(|target| target.key != key),
                            );
                        }
                    >
                        {tr(Msg::RemoveThisTarget)}
                    </button>
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// One target's own fields: the code it maps to, how it relates, and why.
fn target_fields(
    draft: RwSignal<MapDraft>,
    group: Key,
    element: Key,
    key: Key,
    readonly: Signal<bool>,
    relationships: Codes,
    version: Signal<FhirVersion>,
) -> AnyView {
    let id = key.0;
    view! {
        <div class="grid gap-default sm:grid-cols-2">
            {driven_control(
                format!("target-{id}-code"),
                "target-code",
                tr(Msg::TargetCode),
                readonly,
                gated(
                    Box::new(move || target_of(
                        draft,
                        group,
                        element,
                        key,
                        |held| held.code.clone(),
                    )),
                ),
                Box::new(move |code| with_target(
                    draft,
                    group,
                    element,
                    key,
                    |held| held.code = code,
                )),
            )}
            {driven_control(
                format!("target-{id}-display"),
                "target-display",
                tr(Msg::TargetDisplay),
                readonly,
                gated(
                    Box::new(move || target_of(
                        draft,
                        group,
                        element,
                        key,
                        |held| held.display.clone(),
                    )),
                ),
                Box::new(move |display| {
                    with_target(draft, group, element, key, |held| held.display = display);
                }),
            )} {relationship_control(draft, version, group, element, key, readonly, relationships)}
            {driven_control(
                format!("target-{id}-comment"),
                "target-comment",
                tr(Msg::Comment),
                readonly,
                gated(
                    Box::new(move || target_of(
                        draft,
                        group,
                        element,
                        key,
                        |held| held.comment.clone(),
                    )),
                ),
                Box::new(move |comment| {
                    with_target(draft, group, element, key, |held| held.comment = comment);
                }),
            )}
        </div>
    }
    .into_any()
}

/// The control that says how one target relates to its source code.
///
/// The element it fills and the value set it offers are both the served
/// version's, so the label names the element the save will write
/// (`equivalence` on R4 and R4B, `relationship` on R5 and R6).
fn relationship_control(
    draft: RwSignal<MapDraft>,
    version: Signal<FhirVersion>,
    group: Key,
    element: Key,
    key: Key,
    readonly: Signal<bool>,
    relationships: Codes,
) -> AnyView {
    let id = key.0;
    // The one label carries the element the save will write, so what a
    // sighted reader sees is also the control's accessible name
    // (<https://www.w3.org/TR/WCAG22/#label-in-name>).
    let label = gated(Box::new(move || {
        tr_with(
            Msg::Relationship2,
            &[(
                "value0",
                (dialect(version.get()).relationship).to_string().into(),
            )],
        )
    }));
    coded_control(
        Control {
            id: format!("target-{id}-relationship"),
            name: "target-relationship",
            label,
            sr_only: false,
        },
        relationships,
        readonly,
        gated(Box::new(move || {
            target_of(draft, group, element, key, |held| held.relationship.clone())
        })),
        Box::new(move |chosen| {
            with_target(draft, group, element, key, |held| {
                held.relationship = chosen;
            });
        }),
    )
}

/// Picks one code out of a system, through the search the browser screen runs.
///
/// The search is `ValueSet/$expand` over a value set that includes the one
/// system, with the reader's text as `filter`
/// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), so a code
/// arrives with the display the server gave it rather than typed blind.
// NOTE: no FHIR spec governs this, our own design: the phrase is a control of
// one row of this form rather than the screen's own filter, so it is local
// state and the address carries the map being edited.
fn picker(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    id: String,
    what: &str,
    system: Signal<String>,
    readonly: Signal<bool>,
    chose: Chosen,
) -> AnyView {
    let typed = RwSignal::new(String::new());
    let asked = RwSignal::new(String::new());
    let found = Resource::new(
        move || (version.get(), system.get(), asked.get()),
        move |(version, system, filter)| {
            let client = client.get_value();
            async move {
                if system.trim().is_empty() || filter.trim().is_empty() {
                    return Ok(Vec::new());
                }
                let query = ConceptQuery {
                    system,
                    filter: Some(filter),
                    count: SEARCH_COUNT,
                    ..ConceptQuery::default()
                };
                client.expand_inline(version, &query).await.map(|expanded| {
                    expanded
                        .expansion()
                        .map(|expansion| expansion.concepts)
                        .unwrap_or_default()
                })
            }
        },
    );
    let named = StoredValue::new(id);
    let field = move || format!("{}-search", named.with_value(Clone::clone));
    let region = move || format!("{}-found", named.with_value(Clone::clone));
    let search = move |event: SubmitEvent| {
        event.prevent_default();
        asked.set(typed.get_untracked());
    };
    let results = move || {
        let chose = Chosen::clone(&chose);
        found
            .with(|answered| {
                answered.as_ref().map(|read| match read.as_ref() {
                    Err(error) => view! { <Failure error=error.clone() /> }.into_any(),
                    Ok(rows) => offers(rows, readonly, &chose),
                })
            })
            .into_any()
    };
    let counted = move || {
        let asked = asked.get();
        if asked.trim().is_empty() {
            return String::new();
        }
        found.with(|answered| match answered.as_ref() {
            None => tr_with(Msg::SearchingFor, &[("asked", (asked).clone().into())]),
            Some(Err(_refused)) => tr_with(
                Msg::SearchDidNotAnswer,
                &[("asked", (asked).clone().into())],
            ),
            Some(Ok(rows)) => tr_with(
                Msg::CodesMatch,
                &[
                    ("rows", (rows.len()).to_string().into()),
                    ("asked", (asked).clone().into()),
                ],
            ),
        })
    };
    view! {
        <div class="mt-default grid gap-tight">
            <form class="flex flex-wrap items-end gap-default" on:submit=search>
                <div class="grid grow gap-tight">
                    <label for=field class=styles::LABEL>
                        {tr_with(Msg::FindSystem, &[("what", what.into())])}
                    </label>
                    <input
                        id=field
                        name=field
                        type="search"
                        class=styles::INPUT
                        disabled=move || readonly.get()
                        prop:value=move || typed.get()
                        on:input:target=move |event| typed.set(event.target().value())
                    />
                </div>
                <button type="submit" class=styles::BUTTON disabled=move || readonly.get()>
                    {tr(Msg::Search)}
                </button>
            </form>
            <p id=region aria-live="polite" class=styles::HINT>
                {counted}
            </p>
            // A picker that has not been searched has nothing to wait for, so a
            // saved map with twenty codes does not paint twenty spinners.
            <Transition fallback=move || {
                (!asked.read().trim().is_empty()).then(|| busy(tr(Msg::Searching), Busy::Section))
            }>{results}</Transition>
        </div>
    }
    .into_any()
}

/// The codes one search found, each a control that fills the row.
fn offers(rows: &[ConceptRow], readonly: Signal<bool>, chose: &Chosen) -> AnyView {
    let drawn: Vec<AnyView> = rows
        .iter()
        .map(|row| {
            let code = row.code.clone();
            let display = row.display.clone().unwrap_or_default();
            let shown = if display.is_empty() {
                code.clone()
            } else {
                format!("{display} ({code})")
            };
            let chose = Chosen::clone(chose);
            view! {
                <li>
                    <button
                        type="button"
                        class=styles::BUTTON
                        disabled=move || readonly.get()
                        on:click=move |_| chose(code.clone(), display.clone())
                    >
                        {shown}
                    </button>
                </li>
            }
            .into_any()
        })
        .collect();
    view! { <ul class="mt-tight flex flex-wrap gap-tight">{drawn}</ul> }.into_any()
}

/// What the preview of one code answered.
fn preview_view(preview: RwSignal<Option<Preview>>, element: Key) -> AnyView {
    view! {
        {move || {
            preview
                .with(|held| {
                    held.as_ref()
                        .filter(|held| held.element == element)
                        .map(|held| match &held.answer {
                            Err(error) => view! { <Failure error=error.clone() /> }.into_any(),
                            Ok(answer) => matches_view(answer, held.inline),
                        })
                })
        }}
    }
    .into_any()
}

/// The matches one preview reported, as the table a reader reads.
fn matches_view(answer: &TranslateAnswer, inline: bool) -> AnyView {
    let source = if inline {
        tr(Msg::TranslatedThroughMapScreenSentRequest)
    } else {
        tr(Msg::ServerDidNotTakeMapInline)
    };
    let result = match answer.result() {
        Some(true) => tr(Msg::ServerTranslatedCode),
        Some(false) => tr(Msg::ServerTranslatedNothingCode),
        None => tr(Msg::ServerStatedNoResult),
    };
    let message = answer.message().unwrap_or_default();
    let rows: Vec<AnyView> = answer
        .matches()
        .iter()
        .map(|matched| {
            let relation = matched
                .relationship
                .clone()
                .or_else(|| matched.equivalence.clone())
                .unwrap_or_default();
            let target = matched.target.clone().unwrap_or_default();
            let code = target.code.clone().unwrap_or_default();
            let display = target.display.clone().unwrap_or_default();
            let system = target.system.clone().unwrap_or_default();
            view! {
                <tr>
                    <th scope="row" class=styles::TH>
                        <span class=styles::CODE>{code}</span>
                    </th>
                    <td class=styles::TD>{display}</td>
                    <td class=styles::TD>{relation}</td>
                    <td class=styles::TD>
                        <span class=styles::CODE_MUTED>{system}</span>
                    </td>
                </tr>
            }
            .into_any()
        })
        .collect();
    let table = if rows.is_empty() {
        ().into_any()
    } else {
        view! {
            <table class=styles::TABLE>
                <thead>
                    <tr>
                        <th scope="col" class=styles::TH>
                            {tr(Msg::Code)}
                        </th>
                        <th scope="col" class=styles::TH>
                            {tr(Msg::Display)}
                        </th>
                        <th scope="col" class=styles::TH>
                            {tr(Msg::Relationship)}
                        </th>
                        <th scope="col" class=styles::TH>
                            {tr(Msg::System)}
                        </th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        }
        .into_any()
    };
    view! {
        <div class="mt-default grid gap-tight">
            <p class=styles::MUTED>{source}</p>
            <p class=styles::MUTED>{result} " " {message}</p>
            {table}
        </div>
    }
    .into_any()
}

/// Runs one `$translate` preview for the code `element` names.
fn run_preview(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    group: Key,
    element: Key,
    preview: RwSignal<Option<Preview>>,
    report: RwSignal<String>,
) {
    let version = version.get_untracked();
    let held = draft.get_untracked();
    let problems = held.problems(version);
    if let Some(first) = problems.first() {
        preview.set(None);
        report.set(tr_with(
            Msg::MapHasCompleteBeforeCanTranslated,
            &[("first", (first.text()).clone().into())],
        ));
        return;
    }
    let Some(request) = request_for(&held, group, element) else {
        preview.set(None);
        report.set(tr(Msg::CodeHasNoSystemTranslateFrom));
        return;
    };
    let resource = held.resource(version);
    let saved = Some(held.url.trim().to_owned())
        .filter(|url| !url.is_empty() && !held.id.is_empty())
        .map(|url| TranslateRequest {
            concept_map: url,
            concept_map_version: held.version.trim().to_owned(),
            ..request.clone()
        });
    let client = client.get_value();
    report.set(tr(Msg::Translating));
    spawn_local(async move {
        let inline = Box::pin(client.translate_inline(version, &request, &resource)).await;
        let (inline_used, answer) = match (inline, saved) {
            (Ok(answer), _) => (true, Ok(answer)),
            // Only the refusal the operation anticipates falls back: a server
            // "may choose not to accept concept maps in this fashion"
            // (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>)
            // and says so with `not-supported`
            // (<https://hl7.org/fhir/R4B/valueset-issue-type.html>). Every
            // other refusal is about the map itself, and is shown whole
            // rather than reported as something the server cannot do.
            (Err(error), Some(saved)) if unsupported(&error) => {
                (false, Box::pin(client.translate(version, &saved)).await)
            }
            (Err(error), _elsewhere) => (true, Err(error)),
        };
        report.set(previewed_text(inline_used, answer.as_ref()));
        preview.set(Some(Preview {
            element,
            inline: inline_used,
            answer,
        }));
    });
}

/// Whether a refusal says the server does not take a map sent inline.
///
/// `not-supported` is the issue type for "the interaction, operation, resource
/// or profile is not supported"
/// (<https://hl7.org/fhir/R4B/valueset-issue-type.html>), which is what a
/// server that declines the `conceptMap` parameter answers.
fn unsupported(error: &FhirError) -> bool {
    error
        .outcome()
        .is_some_and(|outcome| outcome.carries_code("not-supported"))
}

/// What the live region says after a preview.
fn previewed_text(inline: bool, answer: Result<&TranslateAnswer, &FhirError>) -> String {
    match answer {
        Err(error) => tr_with(
            Msg::PreviewDidNotAnswer,
            &[("value0", (diagnostics(error)).clone().into())],
        ),
        Ok(answer) => {
            let through = if inline {
                tr(Msg::MapScreen)
            } else {
                tr(Msg::SavedMapBecauseServerDidNot)
            };
            tr_with(
                Msg::TranslatedThroughMatches,
                &[
                    ("through", (through).clone().into()),
                    ("answer", (answer.matches().len()).to_string().into()),
                ],
            )
        }
    }
}

/// The `$translate` request one code of one group sends.
fn request_for(draft: &MapDraft, group: Key, element: Key) -> Option<TranslateRequest> {
    let group = draft.groups.iter().find(|held| held.key == group)?;
    let element = group.elements.iter().find(|held| held.key == element)?;
    if group.source.trim().is_empty() || element.code.trim().is_empty() {
        return None;
    }
    Some(TranslateRequest {
        system: group.source.trim().to_owned(),
        system_version: group.source_version.trim().to_owned(),
        code: element.code.trim().to_owned(),
        target_system: group.target.trim().to_owned(),
        ..TranslateRequest::default()
    })
}

/// The control that sends the whole resource, and what it reports.
#[expect(
    clippy::too_many_arguments,
    reason = "the control reports into four signals and one stored value of the form it belongs to, and threading them through a struct would only rename the same arguments"
)]
fn save_section(
    client: StoredValue<FhirClient>,
    version: Signal<FhirVersion>,
    draft: RwSignal<MapDraft>,
    readonly: Signal<bool>,
    saving: RwSignal<bool>,
    report: RwSignal<String>,
    refusal: RwSignal<Option<FhirError>>,
    session: Session,
    canonical: StoredValue<String>,
) -> AnyView {
    let problems = Memo::new(move |_| draft.read().problems(version.get()));
    let send = move |_| {
        if saving.get_untracked() || readonly.get_untracked() {
            return;
        }
        let held = draft.get_untracked();
        let at = version.get_untracked();
        if let Some(first) = held.problems(at).first() {
            report.set(first.text());
            return;
        }
        saving.set(true);
        refusal.set(None);
        report.set(tr(Msg::Saving));
        let client = client.get_value();
        spawn_local(async move {
            send_it(
                &client,
                at,
                &held,
                Reports {
                    draft,
                    report,
                    refusal,
                    session,
                    canonical,
                },
            )
            .await;
            saving.set(false);
        });
    };
    let listed = move || {
        problems
            .read()
            .iter()
            .map(|problem| view! { <li>{problem.text()}</li> }.into_any())
            .collect::<Vec<AnyView>>()
    };
    view! {
        <Show when=move || !readonly.get() fallback=|| ()>
            <section class="fui-card" aria-labelledby="map-save-heading">
                <h2 id="map-save-heading" class="fui-card-title">
                    {tr(Msg::Save)}
                </h2>
                <p class="fui-muted">
                    {tr(Msg::UpdateStatesVersionReplacesSoChange)}
                </p>
                <Show when=move || !problems.read().is_empty() fallback=|| ()>
                    <ul class=format!("mt-default grid gap-tight {}", styles::MUTED)>{listed}</ul>
                </Show>
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
                                {tr(Msg::CreateThisConceptMap)}
                            } else {
                                {tr(Msg::SaveThisConceptMap)}
                            }
                        }}
                    </button>
                </p>
            </section>
        </Show>
    }
    .into_any()
}

/// Where a save writes what it did, so one argument carries them together.
#[derive(Clone, Copy)]
struct Reports {
    /// The draft, which a save that was taken gives an id and a version.
    draft: RwSignal<MapDraft>,
    /// The live region.
    report: RwSignal<String>,
    /// The refusal the screen renders whole.
    refusal: RwSignal<Option<FhirError>>,
    /// The session, so a spent token is dropped.
    session: Session,
    /// The canonical the server now holds the map under.
    canonical: StoredValue<String>,
}

/// Sends one save and writes down what came back.
async fn send_it(client: &FhirClient, version: FhirVersion, held: &MapDraft, into: Reports) {
    let body = held.body(version);
    let written = if held.id.is_empty() {
        Box::pin(client.create(version, CONCEPT_MAP, &body)).await
    } else {
        Box::pin(client.update(
            version,
            CONCEPT_MAP,
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
}

/// What the live region says after a save the server took.
fn saved_text(version_id: &str) -> String {
    if version_id.is_empty() {
        tr(Msg::SavedServerStatedNoVersion)
    } else {
        tr_with(
            Msg::SavedServerNowHoldsVersion,
            &[("version_id", (version_id).to_string().into())],
        )
    }
}

/// Reads one group inside the guard, without cloning the rest.
fn group_of<T: Default>(draft: RwSignal<MapDraft>, key: Key, project: fn(&MapGroup) -> T) -> T {
    draft.with(|draft| {
        draft
            .groups
            .iter()
            .find(|group| group.key == key)
            .map_or_else(T::default, project)
    })
}

/// Applies `change` to the group `key` names.
fn with_group(draft: RwSignal<MapDraft>, key: Key, change: impl FnOnce(&mut MapGroup)) {
    draft.update(|draft| {
        if let Some(group) = draft.groups.iter_mut().find(|group| group.key == key) {
            change(group);
        }
    });
}

/// Reads one code of one group inside the guard.
fn element_of<T: Default>(
    draft: RwSignal<MapDraft>,
    group: Key,
    key: Key,
    project: fn(&MapElement) -> T,
) -> T {
    draft.with(|draft| {
        draft
            .groups
            .iter()
            .find(|held| held.key == group)
            .and_then(|held| held.elements.iter().find(|element| element.key == key))
            .map_or_else(T::default, project)
    })
}

/// Applies `change` to one code of one group.
fn with_element(
    draft: RwSignal<MapDraft>,
    group: Key,
    key: Key,
    change: impl FnOnce(&mut MapElement),
) {
    draft.update(|draft| {
        if let Some(element) = draft
            .groups
            .iter_mut()
            .find(|held| held.key == group)
            .and_then(|held| held.elements.iter_mut().find(|element| element.key == key))
        {
            change(element);
        }
    });
}

/// Reads one target of one code inside the guard.
fn target_of<T: Default>(
    draft: RwSignal<MapDraft>,
    group: Key,
    element: Key,
    key: Key,
    project: fn(&MapTarget) -> T,
) -> T {
    draft.with(|draft| {
        draft
            .groups
            .iter()
            .find(|held| held.key == group)
            .and_then(|held| held.elements.iter().find(|held| held.key == element))
            .and_then(|held| held.targets.iter().find(|target| target.key == key))
            .map_or_else(T::default, project)
    })
}

/// Applies `change` to one target of one code.
fn with_target(
    draft: RwSignal<MapDraft>,
    group: Key,
    element: Key,
    key: Key,
    change: impl FnOnce(&mut MapTarget),
) {
    draft.update(|draft| {
        if let Some(target) = draft
            .groups
            .iter_mut()
            .find(|held| held.key == group)
            .and_then(|held| held.elements.iter_mut().find(|held| held.key == element))
            .and_then(|held| held.targets.iter_mut().find(|target| target.key == key))
        {
            change(target);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::outcome::OperationOutcome;

    #[test]
    fn a_new_concept_map_and_an_edited_one_title_the_page_apart() {
        assert_eq!(
            editor_title(message(Msg::TitleConceptMapEditor), &editor_mode("  ")),
            "New concept map"
        );
        assert_eq!(
            editor_title(
                message(Msg::TitleConceptMapEditor),
                &editor_mode("https://terminology.example/colour-map")
            ),
            "Edit https://terminology.example/colour-map"
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
    fn the_announcement_says_which_map_the_preview_went_through() {
        let answer = TranslateAnswer::default();
        assert!(
            previewed_text(true, Ok(&answer)).contains("the map on this screen"),
            "a preview the server took inline says so"
        );
        assert!(
            previewed_text(false, Ok(&answer)).contains("did not take the map inline"),
            "and one that fell back says why"
        );
    }

    #[test]
    fn a_preview_that_did_not_answer_says_what_stopped_it() {
        let outcome: OperationOutcome = serde_json::from_str(
            r#"{"resourceType":"OperationOutcome","issue":[
                 {"severity":"error","code":"not-supported",
                  "details":{"text":"this server takes no inline concept map"}}]}"#,
        )
        .expect("the server's own answer parses");
        let error = FhirError::Refused {
            url: String::from("https://tx.example.org/r4b/ConceptMap/$translate"),
            status: http::StatusCode::BAD_REQUEST,
            outcome,
        };
        assert_eq!(
            previewed_text(true, Err(&error)),
            "The preview did not answer: this server takes no inline concept map",
            "the server's own wording reaches the live region"
        );
    }

    #[test]
    fn the_announcement_carries_the_server_s_own_wording() {
        let outcome: OperationOutcome = serde_json::from_str(
            r#"{"resourceType":"OperationOutcome","issue":[
                 {"severity":"error","code":"invariant",
                  "details":{"text":"A ConceptMap must have a status"}}]}"#,
        )
        .expect("the server's own answer parses");
        let error = FhirError::Refused {
            url: String::from("https://tx.example.org/r4b/ConceptMap"),
            status: http::StatusCode::UNPROCESSABLE_ENTITY,
            outcome,
        };
        assert_eq!(diagnostics(&error), "A ConceptMap must have a status");
    }
}
