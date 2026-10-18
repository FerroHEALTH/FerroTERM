//! The value set composer: build a `ValueSet.compose`, preview it, save it.
//!
//! The screen is the editor bundle's own. It draws the composition rules FHIR
//! defines as they apply (<https://hl7.org/fhir/R4B/valueset.html#compositions>),
//! previews the unsaved definition with `$expand` by `POST`
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), and writes
//! through the RESTful API with `If-Match`
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>).
//!
//! Nothing here names a code system. The systems come from the served root's
//! `TerminologyCapabilities`, the value sets from `GET [base]/ValueSet`, and
//! a clause selects with the filter properties and operators that capability
//! statement declares for the version it is over.
//!
//! Every control on the form is drawn by one of the four shapes below, whose
//! listeners arrive boxed. A control shape written once as a generic over its
//! listener is compiled once per call site, and this screen has forty of them
//! (`docs/viewer.md` §13, the bundle bar).

use std::sync::Arc;

use leptos::ev::Event;
use leptos::ev::MouseEvent;
use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_query_map;

use crate::auth::Session;
use crate::auth::scopes::Letter;
use crate::components::failure::Failure;
use crate::components::field::group;
use crate::components::history::history_offer;
use crate::components::icon;
use crate::components::reading::Reading;
use crate::components::reload::READING_AGAIN;
use crate::components::reload::announced;
use crate::components::reload::focus_report;
use crate::components::reload::reload_offer;
use crate::components::shell::SelectedVersion;
use crate::fhir::FhirClient;
use crate::fhir::VALUE_SET;
use crate::fhir::compose::Clause;
use crate::fhir::compose::Draft;
use crate::fhir::compose::Preview;
use crate::fhir::compose::STATUSES;
use crate::fhir::compose::StoredValueSet;
use crate::fhir::compose::mark;
use crate::fhir::concept::ConceptQuery;
use crate::fhir::error::FhirError;
use crate::fhir::expansion::ExpandedValueSet;
use crate::fhir::named::Choice;
use crate::fhir::outcome::Issue;
use crate::fhir::outcome::OperationOutcome;
use crate::fhir::terminology::FilterRow;
use crate::fhir::terminology::ImplicitArgument;
use crate::fhir::terminology::ImplicitForm;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::terminology::VersionRow;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::Refusal;
use crate::offers::choices;
use crate::offers::published;
use crate::paging::MAX_COUNT;
use crate::paging::Page;
use crate::routes::COMPOSE_PATH;
use crate::routes::COMPOSER_ID_PARAM;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::styles;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::surface::stack;
use serde_json::Value;

/// The query parameter carrying the preview's text filter.
const FILTER_PARAM: &str = "filter";

/// The query parameter carrying the preview's page size.
const COUNT_PARAM: &str = "count";

/// The query parameter carrying the preview's offset.
const OFFSET_PARAM: &str = "offset";

/// The page the preview opens on when the address names none.
const DEFAULT_COUNT: u32 = 20;

/// How many concepts one code search answers.
const SEARCH_COUNT: u32 = 20;

/// The live region every refusal and every count is announced in.
const REPORT_ID: &str = "composer-report";

/// What a control does when the reader changes it.
type OnEvent = Box<dyn FnMut(Event)>;

/// What a control does when the reader presses it.
type OnClick = Box<dyn FnMut(MouseEvent)>;

/// What a choice control offers.
type Options = Box<dyn Fn() -> Vec<Choice> + Send + Sync>;

/// What one control is called, on the wire and on the screen.
///
/// The `id` is a `String` because a clause draws as many of these as it has
/// rows and a label points at one control
/// (<https://www.w3.org/TR/wai-aria-1.2/#namecalculation>).
struct Named {
    /// The `id` the label points at.
    id: String,
    /// The `name` the control carries.
    name: &'static str,
    /// What a reader calls it.
    label: &'static str,
    /// The sentence under it.
    note: &'static str,
}

/// Composes a local `ValueSet`, previews it, and saves it.
///
/// The composition itself is a document being edited rather than a filter, so
/// it lives in a signal; the preview's page and text filter are the address,
/// which is what makes a previewed page shareable and the back button work
/// (<https://github.com/leptos-rs/book/blob/main/src/router/20_form.md>).
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ValueSetComposerPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let session = expect_context::<Session>();

    let query = use_query_map();
    let id: Memo<String> = Memo::new(move |_| {
        query.with(|map| {
            map.get(COMPOSER_ID_PARAM)
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
    });
    let page: Memo<Preview> = Memo::new(move |_| {
        query.with(|map| Preview {
            filter: map
                .get(FILTER_PARAM)
                .map(|typed| typed.trim().to_owned())
                .filter(|typed| !typed.is_empty()),
            count: map
                .get(COUNT_PARAM)
                .and_then(|typed| typed.trim().parse::<u32>().ok())
                .unwrap_or(DEFAULT_COUNT)
                .clamp(1, MAX_COUNT),
            offset: map
                .get(OFFSET_PARAM)
                .and_then(|typed| typed.trim().parse::<u32>().ok())
                .unwrap_or_default(),
        })
    });

    // The resource is read again after a save, so the version an `If-Match`
    // states is the one the server just committed. The counter is written from
    // the save handler, which is an event rather than an effect.
    let saves = RwSignal::new(0_u32);
    let read = reading(&client, version, id, saves);
    let document = document_of(read);
    let answered = answered_of(document);
    let editing = Editing {
        edited: RwSignal::new(None),
        stored: Memo::new(move |_| {
            document.with(|held| match held {
                Some(document) => StoredValueSet::of(document).draft(document),
                None => Draft::new(),
            })
        }),
    };
    let writable = writable(id, answered, session);

    let report = RwSignal::new(String::new());
    let refusal: RwSignal<Option<FhirError>> = RwSignal::new(None);
    let previewed: RwSignal<Option<Draft>> = RwSignal::new(None);

    let declared = capabilities(&client, version);
    let offers = Offers {
        value_sets: choices(published(&client, version, VALUE_SET)),
        capabilities: declarations(declared),
        picking: RwSignal::new(None),
        term: RwSignal::new(String::new()),
    };

    let expansion = previewing(&client, version, previewed, page);
    let heading = heading(report, announcing(expansion));

    let editable = editable(id, writable);
    let versions = history_offer(VALUE_SET, id.into(), version);
    let refused_read = read_refusal(read);
    let refused_capabilities = capability_refusal(declared);
    let banner = read_only_banner(writable, id, session);
    let identity = identity_section(editing, editable);
    let clauses = clause_sections(editing, editable, offers);
    let actions = save_section(
        &client, version, editing, writable, session, report, refusal, saves, previewed,
    );
    let refused = refusal_section(refusal, editing, saves, report);
    let preview = preview_section(expansion, previewed, page, editing, report, id);

    view! {
        {heading}
        <div class="mt-loose grid items-start gap-loose lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
            {stack(
                view! {
                    {versions}
                    {refused_read}
                    {refused_capabilities}
                    {banner}
                    {identity}
                    {clauses}
                    {actions}
                    {refused}
                },
            )} <div class="min-w-0">{preview}</div>
        </div>
    }
}

/// What a read of the value set the address names answered.
///
/// It is the document the server sent, so a save writes over it rather than
/// replacing it (<https://hl7.org/fhir/R4B/http.html#update>). The refusal is
/// kept rather than folded into "nothing read yet", so an id this root does
/// not hold renders the server's own `OperationOutcome` instead of an empty
/// form.
type Reading = LocalResource<Option<Result<Value, FhirError>>>;

/// The title, the lead, and the one live region every message lands in.
///
/// One live region carries every message the screen has, so a screen reader
/// hears the count, the refusal, and the save in one place
/// (<https://www.w3.org/TR/wai-aria-1.2/#aria-live>). What an event wrote
/// wins until the next preview is asked for, which clears it.
fn heading(report: RwSignal<String>, announcement: Memo<String>) -> AnyView {
    let said = move || {
        let written = report.get();
        if written.is_empty() {
            announcement.get()
        } else {
            written
        }
    };
    view! {
        <Title text="Compose a value set" />
        <div>
            {ferrohealth_ui::view::surface::page_heading(
                ferrohealth_ui::view::surface::Header {
                    title: String::from("Compose a value set"),
                    subtitle: Some(
                        String::from(
                            "Draw in a published value set, add your own codes, and see what the selection holds.",
                        ),
                    ),
                    crumbs: vec![
                        ferrohealth_ui::view::surface::Crumb::new("Value sets", "/valuesets"),
                    ],
                    mono: false,
                },
                (),
            )}
        </div>
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
            {said}
        </p>
    }
    .into_any()
}

/// Reads the `ValueSet` the address names, again after every save.
fn reading(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    id: Memo<String>,
    saves: RwSignal<u32>,
) -> Reading {
    let client = client.clone();
    LocalResource::new(move || {
        let client = client.clone();
        let version = version.get();
        let id = id.get();
        let _after_a_save = saves.get();
        async move {
            if id.is_empty() {
                None
            } else {
                Some(client.value_set_stored(version, &id).await)
            }
        }
    })
}

/// The document the read answered, when it answered one.
fn document_of(read: Reading) -> Memo<Option<Value>> {
    Memo::new(move |_| {
        read.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok())
                .cloned()
        })
    })
}

/// The elements the composer reads out of that document.
fn answered_of(document: Memo<Option<Value>>) -> Memo<Option<StoredValueSet>> {
    Memo::new(move |_| document.with(|held| held.as_ref().map(StoredValueSet::of)))
}

/// What the server said when it refused the read, in its own words.
fn read_refusal(read: Reading) -> AnyView {
    let shown = move || {
        read.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().err())
                .map(|error| view! { <Failure error=Signal::stored(error.clone()) /> }.into_any())
        })
    };
    view! { <div class="mt-default">{shown}</div> }.into_any()
}

/// Whether the screen may offer to save what it is showing.
///
/// National content has no write path: the built indexes open read-only, and
/// the server stamps `meta.versionId` only on what it holds a writable record
/// of (<https://hl7.org/fhir/R4B/http.html#concurrency>), so a resource
/// carrying none is read even for a token that holds every scope.
fn writable(
    id: Memo<String>,
    answered: Memo<Option<StoredValueSet>>,
    session: Session,
) -> Memo<bool> {
    Memo::new(move |_| {
        let new_draft = id.with(String::is_empty);
        let letter = if new_draft {
            Letter::Create
        } else {
            Letter::Update
        };
        session.can(VALUE_SET, letter)
            && (new_draft
                || answered.with(|held| held.as_ref().is_some_and(StoredValueSet::writable)))
    })
}

/// Whether the form itself takes what the reader types.
///
/// Composing and previewing are reads, so a new draft is composed by anyone;
/// the save is the write, and the control for it is drawn only where the token
/// opens it. A resource the server holds and will not take a write for opens
/// read-only, which is what national content does under every role.
fn editable(id: Memo<String>, writable: Memo<bool>) -> Memo<bool> {
    Memo::new(move |_| id.with(String::is_empty) || writable.get())
}

/// The draft on screen: the reader's edits, or the resource as it was read.
///
/// Holding the edits beside the read rather than copying the read into a
/// signal is what keeps an effect out of this screen: nothing writes a signal
/// from another signal's value, and reopening the resource is dropping the
/// edits (`working_with_signals` §4).
#[derive(Clone, Copy)]
struct Editing {
    /// What the reader has changed, absent until they change something.
    edited: RwSignal<Option<Draft>>,
    /// The resource as the server answered it, or a new draft.
    stored: Memo<Draft>,
}

impl Editing {
    /// One value of the draft on screen, as its own memo.
    fn part<T>(self, read: impl Fn(&Draft) -> T + Send + Sync + 'static) -> Memo<T>
    where
        T: PartialEq + Send + Sync + 'static,
    {
        Memo::new(move |_| {
            self.edited
                .with(|held| held.as_ref().map(&read))
                .unwrap_or_else(|| self.stored.with(&read))
        })
    }

    /// One string of the draft on screen, as the signal a control is seeded
    /// from.
    fn text(self, read: impl Fn(&Draft) -> String + Send + Sync + 'static) -> Signal<String> {
        self.part(read).into()
    }

    /// The draft on screen, cloned, for a request that carries the whole of it.
    fn draft(self) -> Draft {
        self.edited
            .with(Clone::clone)
            .unwrap_or_else(|| self.stored.get())
    }

    /// The draft on screen, read without subscribing to it.
    ///
    /// A caller describing something that has already happened, such as a
    /// refusal that came back, reads it this way so the view it draws does not
    /// rebuild on every keystroke in the form.
    fn held(self) -> Draft {
        self.edited
            .with_untracked(Clone::clone)
            .unwrap_or_else(|| self.stored.get_untracked())
    }

    /// Applies one change to the draft on screen.
    fn change(self, apply: impl FnOnce(&mut Draft)) {
        let mut draft = self.held();
        apply(&mut draft);
        self.edited.set(Some(draft));
    }

    /// Drops the edits, so the screen shows the resource as the server holds it.
    fn reopen(self) {
        self.edited.set(None);
    }

    /// One clause of the draft on screen, as its own memo.
    fn clause(self, key: u32) -> Memo<Clause> {
        self.part(move |draft| draft.clause(key).cloned().unwrap_or_default())
    }

    /// Changes one clause of the draft on screen.
    fn change_clause(self, key: u32, apply: impl FnOnce(&mut Clause)) {
        self.change(move |draft| {
            if let Some(clause) = draft.clause_mut(key) {
                apply(clause);
            }
        });
    }
}

/// What the pickers on the form offer, read once for the whole screen.
#[derive(Clone, Copy)]
struct Offers {
    /// The value sets this root publishes.
    value_sets: Memo<Vec<Choice>>,
    /// What the root declares about the systems it serves.
    capabilities: Memo<TerminologyCapabilities>,
    /// The clause whose code search is open, when one is.
    picking: RwSignal<Option<u32>>,
    /// What that search is looking for.
    term: RwSignal<String>,
}

impl Offers {
    /// The code systems this root serves, as a picker's offers.
    fn systems(self) -> Vec<Choice> {
        self.capabilities.with(|declared| {
            declared
                .cards()
                .into_iter()
                .filter(|card| !card.url.is_empty())
                .map(|card| Choice {
                    canonical: card.url.clone(),
                    label: card.url,
                })
                .collect()
        })
    }

    /// The filters the served version declares for `system`.
    fn filters(self, system: &str, pinned: &str) -> Vec<FilterRow> {
        self.declared(system, pinned, |version| version.filters.clone())
    }

    /// The implicit value set forms the served version declares for `system`.
    fn implicit_forms(self, system: &str, pinned: &str) -> Vec<ImplicitForm> {
        self.declared(system, pinned, |version| version.implicit_forms.clone())
    }

    /// What `read` takes from the served version of `system` a clause pinned
    /// to `pinned` is answered against, or the empty value when none is served.
    ///
    /// A system serving several versions declares per version, and the
    /// default version is the one an unversioned clause is answered against
    /// (<https://hl7.org/fhir/R5/terminology-module.html#version>).
    fn declared<T: Default>(self, system: &str, pinned: &str, read: fn(&VersionRow) -> T) -> T {
        self.capabilities.with(|declared| {
            declared
                .card(system)
                .and_then(|card| {
                    card.versions
                        .iter()
                        .find(|version| {
                            if pinned.is_empty() {
                                version.is_default
                            } else {
                                version.code.as_deref() == Some(pinned)
                            }
                        })
                        .or_else(|| pinned.is_empty().then(|| card.versions.first()).flatten())
                        .map(read)
                })
                .unwrap_or_default()
        })
    }
}

/// What the root declares about the systems it serves.
///
/// Every picker on the form draws from it, so a read that failed leaves the
/// systems and the filters empty. The refusal is kept beside the declarations
/// rather than folded into them, so the screen says why rather than offering
/// nothing without a word.
type Declared = LocalResource<Result<TerminologyCapabilities, FhirError>>;

/// Reads what the root declares about the systems it serves.
fn capabilities(client: &FhirClient, version: Signal<FhirVersion>) -> Declared {
    let client = client.clone();
    LocalResource::new(move || {
        let client = client.clone();
        let version = version.get();
        async move { client.terminology_capabilities(version).await }
    })
}

/// Those declarations, as the pickers read them.
fn declarations(read: Declared) -> Memo<TerminologyCapabilities> {
    Memo::new(move |_| {
        read.with(|answered| {
            answered
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .cloned()
                .unwrap_or_default()
        })
    })
}

/// What the server said when it refused that read, in its own words.
fn capability_refusal(read: Declared) -> AnyView {
    let shown = move || {
        read.with(|answered| {
            answered
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .map(|error| view! { <Failure error=Signal::stored(error.clone()) /> }.into_any())
        })
    };
    view! { <div class="mt-default">{shown}</div> }.into_any()
}

/// The offers `choices` makes, as a choice control's options.
fn options_of(choices: impl Fn() -> Vec<Choice> + Send + Sync + 'static) -> Options {
    Box::new(choices)
}

/// The options a choice control draws, with the value it holds among them.
///
/// A `<select>` drops a value it has no option for, and the offers arrive
/// after the address does, so the held value is drawn as its own option
/// whenever the list does not carry it. That is also what keeps a clause
/// naming a system this root does not serve readable.
fn drawn(options: &Options, held: &str) -> Vec<AnyView> {
    let mut offered = options();
    if !held.is_empty() && !offered.iter().any(|choice| choice.canonical == held) {
        offered.push(Choice {
            canonical: held.to_owned(),
            label: held.to_owned(),
        });
    }
    offered
        .into_iter()
        .map(|choice| {
            view! {
                <option value=choice.canonical.clone() title=choice.canonical>
                    {choice.label}
                </option>
            }
            .into_any()
        })
        .collect()
}

/// One labelled text control over a value the reader edits.
fn text_control(
    id: String,
    name: &'static str,
    label: Signal<String>,
    note: Signal<String>,
    value: Signal<String>,
    change: OnEvent,
) -> AnyView {
    let described_by = format!("{id}-note");
    view! {
        <div class="grid content-start gap-tight">
            <label for=id.clone() class=styles::LABEL>
                {move || label.get()}
            </label>
            <input
                id=id
                name=name
                type="text"
                class=styles::INPUT
                aria-describedby=described_by.clone()
                prop:value=move || value.get()
                on:input=change
            />
            <p id=described_by class=format!("{} wrap-break-word", styles::HINT)>
                {move || note.get()}
            </p>
        </div>
    }
    .into_any()
}

/// One labelled choice control.
fn select_control(
    named: Named,
    chosen: Signal<String>,
    empty: &'static str,
    options: Options,
    change: OnEvent,
) -> AnyView {
    select_with(named, chosen, empty, options, change, None)
}

/// One labelled choice control with the control that acts on it beside the
/// select, above the note, so the two line up.
fn select_with(
    named: Named,
    chosen: Signal<String>,
    empty: &'static str,
    options: Options,
    change: OnEvent,
    beside: Option<AnyView>,
) -> AnyView {
    let Named {
        id,
        name,
        label,
        note,
    } = named;
    let described_by = format!("{id}-note");
    // The value is read here as well as in `prop:value`, so the option for it
    // is drawn again when the offers arrive and the selection is reapplied.
    let offered = move || drawn(&options, &chosen.get());
    view! {
        <div class="grid content-start gap-tight">
            <label for=id.clone() class=styles::LABEL>
                {label}
            </label>
            <div class="flex items-center gap-default">
                <select
                    id=id
                    name=name
                    class=format!("{} min-w-0 flex-1", ferrohealth_ui::view::form::SELECT)
                    aria-describedby=described_by.clone()
                    prop:value=move || chosen.get()
                    on:change=change
                >
                    <option value="">{empty}</option>
                    {offered}
                </select>
                {beside}
            </div>
            <p id=described_by class=styles::HINT>
                {note}
            </p>
        </div>
    }
    .into_any()
}

/// A control that changes the draft without leaving the page.
fn act(label: &'static str, click: OnClick) -> AnyView {
    view! {
        <button type="button" class=styles::BUTTON on:click=click>
            {label}
        </button>
    }
    .into_any()
}

/// A control beside the control that drops what it edits.
fn removable(control: AnyView, remove: OnClick) -> AnyView {
    view! {
        <div class="flex items-end gap-default">
            <div class="min-w-0 flex-1">{control}</div>
            <button type="button" class=styles::BUTTON_QUIET on:click=remove>
                "Remove"
            </button>
        </div>
    }
    .into_any()
}

/// A fixed sentence, as the signal a control takes for its label or its note.
fn fixed(text: &'static str) -> Signal<String> {
    Signal::stored(String::from(text))
}

/// The notice a screen that cannot be saved carries.
fn read_only_banner(writable: Memo<bool>, id: Memo<String>, session: Session) -> AnyView {
    let sentence = move || {
        if id.with(String::is_empty) {
            if session.signed_in() {
                "This account carries no permission to create a value set, so nothing here can be saved. Composing and previewing work anyway."
            } else {
                "Sign in to save. Composing and previewing work without one."
            }
        } else {
            "This value set has no write path on this server, or this account carries no permission to change it, so it opens read-only."
        }
    };
    view! {
        <Show when=move || !writable.get() fallback=|| ()>
            <p class=format!("mt-default rounded-md p-loose {}", styles::NOTICE)>{sentence}</p>
        </Show>
    }
    .into_any()
}

/// The value set's own identity, and the one flag `compose` carries.
fn identity_section(editing: Editing, editable: Memo<bool>) -> AnyView {
    let fields = vec![
        text_control(
            String::from("compose-url"),
            "url",
            fixed("Canonical"),
            fixed("ValueSet.url, what everything else refers to this value set by."),
            editing.text(|draft| draft.url.clone()),
            Box::new(move |event| {
                let typed = event_target_value(&event);
                editing.change(move |draft| draft.url = typed);
            }),
        ),
        text_control(
            String::from("compose-name"),
            "name",
            fixed("Name"),
            fixed("ValueSet.name, the computer-friendly name."),
            editing.text(|draft| draft.name.clone()),
            Box::new(move |event| {
                let typed = event_target_value(&event);
                editing.change(move |draft| draft.name = typed);
            }),
        ),
        text_control(
            String::from("compose-title"),
            "title",
            fixed("Title"),
            fixed("ValueSet.title, the name written for a person."),
            editing.text(|draft| draft.title.clone()),
            Box::new(move |event| {
                let typed = event_target_value(&event);
                editing.change(move |draft| draft.title = typed);
            }),
        ),
        status_control(editing),
        inactive_control(editing),
    ];
    view! {
        <section class="fui-card" aria-labelledby="compose-identity-heading">
            <h2 id="compose-identity-heading" class="fui-card-title">
                "The value set"
            </h2>
            <fieldset
                class="fui-fieldset mt-default"
                aria-labelledby="compose-identity-heading"
                disabled=move || !editable.get()
            >
                <div class="grid gap-default sm:grid-cols-2">{fields}</div>
            </fieldset>
        </section>
    }
    .into_any()
}

/// The publication status the resource carries.
fn status_control(editing: Editing) -> AnyView {
    select_control(
        Named {
            id: String::from("compose-status"),
            name: "status",
            label: "Status",
            note: "ValueSet.status, which every definitional resource carries.",
        },
        editing.text(|draft| draft.status.clone()),
        "Choose a status",
        options_of(|| {
            STATUSES
                .into_iter()
                .map(|code| Choice {
                    canonical: code.to_owned(),
                    label: code.to_owned(),
                })
                .collect()
        }),
        Box::new(move |event| {
            let chosen = event_target_value(&event);
            editing.change(move |draft| draft.status = chosen);
        }),
    )
}

/// Whether inactive codes are in the selection.
///
/// It has three states: "if inactive = true, inactive codes are to be included
/// in the expansion, if inactive = false, the inactive codes will not be
/// included … If absent, the behavior is determined by the implementation"
/// (<https://hl7.org/fhir/R4B/valueset.html>).
fn inactive_control(editing: Editing) -> AnyView {
    let held = editing.part(|draft| draft.inactive);
    let shown = Signal::derive(move || {
        match held.get() {
            Some(true) => "true",
            Some(false) => "false",
            None => "",
        }
        .to_owned()
    });
    select_control(
        Named {
            id: String::from("compose-inactive"),
            name: "inactive",
            label: "Inactive codes",
            note: "compose.inactive. Left to the server, the expansion parameters decide.",
        },
        shown,
        "Leave it to the server",
        options_of(|| {
            vec![
                Choice {
                    canonical: String::from("true"),
                    label: String::from("Include them"),
                },
                Choice {
                    canonical: String::from("false"),
                    label: String::from("Leave them out"),
                },
            ]
        }),
        Box::new(move |event| {
            let chosen = match event_target_value(&event).as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _unset => None,
            };
            editing.change(move |draft| draft.inactive = chosen);
        }),
    )
}

/// The includes and the excludes, each as its own labelled section.
fn clause_sections(editing: Editing, editable: Memo<bool>, offers: Offers) -> AnyView {
    let includes = clause_section(editing, editable, offers, true);
    let excludes = clause_section(editing, editable, offers, false);
    view! {
        {includes}
        {excludes}
    }
    .into_any()
}

/// One side of the compose: every include, or every exclude.
fn clause_section(
    editing: Editing,
    editable: Memo<bool>,
    offers: Offers,
    included: bool,
) -> AnyView {
    let keys: Memo<Vec<u32>> = editing.part(move |draft| draft.clause_keys(included));
    let heading = if included { "Includes" } else { "Excludes" };
    let heading_id = if included {
        "compose-includes-heading"
    } else {
        "compose-excludes-heading"
    };
    let rule = if included {
        "Every include adds to the selection: the value set is the union of them all."
    } else {
        "Every exclude takes codes back out of that union, whichever include drew them in."
    };
    let cards = view! {
        <For
            each=move || keys.get()
            key=|key| *key
            children=move |key| clause_card(editing, editable, offers, key, included)
        />
    }
    .into_any();
    view! {
        <section class="fui-card" aria-labelledby=heading_id>
            <h2 id=heading_id class="fui-card-title">
                {heading}
            </h2>
            <p class="fui-muted">{rule}</p>
            <div class="mt-default grid gap-default">{cards}</div>
            <button
                type="button"
                class=format!("mt-default {}", styles::BUTTON)
                disabled=move || !editable.get()
                on:click=move |_| {
                    editing
                        .change(|draft| {
                            draft.add_clause(included);
                        });
                }
            >
                {if included { "Add an include" } else { "Add an exclude" }}
            </button>
        </section>
    }
    .into_any()
}

/// One clause: what it selects, and the rule that says how.
///
/// `included` is the side the clause is on, which is fixed when it is made:
/// nothing moves a clause between the includes and the excludes.
fn clause_card(
    editing: Editing,
    editable: Memo<bool>,
    offers: Offers,
    key: u32,
    included: bool,
) -> AnyView {
    let clause = editing.clause(key);
    let defect = editing.part(move |draft| {
        draft
            .broken()
            .into_iter()
            .find(|broken| broken.key == key)
            .map(|broken| {
                format!(
                    "{}: {}",
                    broken.defect.constraint(),
                    broken.defect.sentence()
                )
            })
    });
    let system = Memo::new(move |_| clause.with(|clause| clause.system.clone()));

    let value_sets = value_set_rows(editing, offers, key, clause);
    let system_control = system_picker(editing, offers, key, clause);
    let codes = concept_rows(editing, offers, key, clause, included);
    let filters = filter_rows(editing, offers, key, clause, system);

    view! {
        <fieldset class="fui-fieldset fui-well" disabled=move || !editable.get()>
            <legend>{if included { "Include" } else { "Exclude" }}</legend>
            <p class=styles::MUTED>{move || clause.with(Clause::rule)}</p>
            <Show when=move || defect.with(Option::is_some) fallback=|| ()>
                <p role="status" class=format!("rounded-md p-loose {}", styles::NOTICE)>
                    {move || defect.get().unwrap_or_default()}
                </p>
            </Show>
            {value_sets}
            {system_control}
            {codes}
            {filters}
            <div>
                <button
                    type="button"
                    class=Variant::Destructive.class()
                    on:click=move |_| editing.change(move |draft| draft.remove_clause(key))
                >
                    "Remove this clause"
                </button>
            </div>
        </fieldset>
    }
    .into_any()
}

/// The value sets one clause draws in whole, and the two ways to add one.
fn value_set_rows(editing: Editing, offers: Offers, key: u32, clause: Memo<Clause>) -> AnyView {
    let rows: Memo<Vec<u32>> = Memo::new(move |_| {
        clause.with(|clause| clause.value_sets.iter().map(|row| row.key).collect())
    });
    let listed = view! {
        <For
            each=move || rows.get()
            key=|row| *row
            children=move |row| value_set_row(editing, key, row, clause)
        />
    }
    .into_any();
    group(
        "Value sets",
        vec![
            view! { <div class="grid gap-default">{listed}</div> }.into_any(),
            add_published_value_set(editing, offers, key),
            add_implicit_value_set(editing, offers, key, clause),
        ],
    )
}

/// The canonical of one value set a clause draws in.
///
/// It is edited as the canonical it is: which forms a code system publishes a
/// value set under is that system's own business
/// (<https://hl7.org/fhir/R4B/valueset.html>), so the screen carries the text
/// the server will resolve rather than a grammar of its own.
fn value_set_row(editing: Editing, key: u32, row: u32, clause: Memo<Clause>) -> AnyView {
    let canonical: Memo<String> = Memo::new(move |_| {
        clause.with(|clause| {
            clause
                .value_sets
                .iter()
                .find(|held| held.key == row)
                .map(|held| held.canonical.clone())
                .unwrap_or_default()
        })
    });
    let control = text_control(
        format!("compose-valueset-{key}-{row}"),
        "valueSet",
        fixed("Canonical"),
        fixed("compose.include.valueSet, which draws the whole of that value set in."),
        canonical.into(),
        Box::new(move |event| {
            let typed = event_target_value(&event);
            editing.change(move |draft| draft.write_value_set(key, row, &typed));
        }),
    );
    removable(
        control,
        Box::new(move |_| {
            editing.change_clause(key, move |clause| {
                clause.value_sets.retain(|held| held.key != row);
            });
        }),
    )
}

/// The control that adds one of the value sets this root publishes.
fn add_published_value_set(editing: Editing, offers: Offers, key: u32) -> AnyView {
    let picked = RwSignal::new(String::new());
    let add = act(
        "Add",
        Box::new(move |_| {
            let canonical = picked.get_untracked();
            if !canonical.is_empty() {
                editing.change(move |draft| draft.add_value_set(key, &canonical));
                picked.set(String::new());
            }
        }),
    );
    select_with(
        Named {
            id: format!("compose-add-vs-{key}"),
            name: "addValueSet",
            label: "Add a published value set",
            note: "compose.include.valueSet, which draws the whole of that value set in.",
        },
        picked.into(),
        "Choose one this server holds",
        options_of(move || offers.value_sets.get()),
        Box::new(move |event| picked.set(event_target_value(&event))),
        Some(add),
    )
}

/// The control that adds an implicit value set of the clause's code system.
///
/// The forms are the templates the served version declares; the screen fills
/// their bracketed placeholders and never knows which system a template
/// belongs to. A version that declares none draws nothing here.
fn add_implicit_value_set(
    editing: Editing,
    offers: Offers,
    key: u32,
    clause: Memo<Clause>,
) -> AnyView {
    let forms: Memo<Vec<ImplicitForm>> = Memo::new(move |_| {
        clause.with(|clause| offers.implicit_forms(&clause.system, &clause.system_version))
    });
    let chosen = RwSignal::new(String::new());
    let arguments: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let form = Memo::new(move |_| {
        chosen.with(|pattern| {
            forms.with(|forms| forms.iter().find(|form| &form.pattern == pattern).cloned())
        })
    });
    let url = Memo::new(move |_| {
        form.with(|form| {
            form.as_ref()
                .and_then(|form| arguments.with(|held| form.instantiate(held)))
        })
    });
    let control = select_control(
        Named {
            id: format!("compose-implicit-{key}"),
            name: "implicitValueSet",
            label: "Add a value set the code system defines",
            note: "An implicit value set, named by a URL template the code system publishes.",
        },
        chosen.into(),
        "Choose a template",
        options_of(move || {
            forms
                .get()
                .into_iter()
                .map(|form| Choice {
                    canonical: form.pattern.clone(),
                    label: form.pattern,
                })
                .collect()
        }),
        Box::new(move |event| {
            arguments.set(Vec::new());
            chosen.set(event_target_value(&event));
        }),
    );
    let fields = move || {
        form.get()
            .map(|form| implicit_arguments(editing, key, clause, &form, arguments))
    };
    let add = move |_: MouseEvent| {
        if let Some(url) = url.get_untracked() {
            editing.change(move |draft| draft.add_value_set(key, &url));
            chosen.set(String::new());
            arguments.set(Vec::new());
        }
    };
    // The control stays in the document and is hidden while the version
    // declares no form, so the reader's choice survives a re-read of the
    // capabilities.
    view! {
        <div class="grid gap-default" hidden=move || forms.with(Vec::is_empty)>
            {control}
            {fields}
            <div>
                <button
                    type="button"
                    class=styles::BUTTON
                    disabled=move || url.with(Option::is_none)
                    on:click=add
                >
                    "Add"
                </button>
            </div>
        </div>
    }
    .into_any()
}

/// The controls that fill the placeholders of `form`.
///
/// A `code` placeholder takes a code picked from the clause's code system by
/// the same search that picks the clause's own codes; an `expression`
/// placeholder takes text, one field per placeholder.
fn implicit_arguments(
    editing: Editing,
    key: u32,
    clause: Memo<Clause>,
    form: &ImplicitForm,
    arguments: RwSignal<Vec<String>>,
) -> AnyView {
    let placeholders = form.placeholders();
    match form.argument {
        ImplicitArgument::None => ().into_any(),
        ImplicitArgument::Code => {
            let placeholder = placeholders.into_iter().next().unwrap_or_default();
            let picked = argument_control(key, 0, &placeholder, arguments);
            let search = code_search(
                editing,
                Search {
                    picking: RwSignal::new(None),
                    term: RwSignal::new(String::new()),
                },
                key,
                clause,
                PickInto::Argument(arguments),
            );
            view! { <div class="grid gap-default">{picked} {search}</div> }.into_any()
        }
        ImplicitArgument::Expression => {
            let fields: Vec<AnyView> = placeholders
                .into_iter()
                .enumerate()
                .map(|(index, placeholder)| argument_control(key, index, &placeholder, arguments))
                .collect();
            view! { <div class="grid gap-default">{fields}</div> }.into_any()
        }
    }
}

/// The text control holding the argument of one placeholder.
fn argument_control(
    key: u32,
    index: usize,
    placeholder: &str,
    arguments: RwSignal<Vec<String>>,
) -> AnyView {
    text_control(
        format!("compose-implicit-{key}-{index}"),
        "implicitArgument",
        Signal::stored(format!("[{placeholder}]")),
        fixed("What replaces that placeholder in the template."),
        Signal::derive(move || arguments.with(|held| held.get(index).cloned().unwrap_or_default())),
        Box::new(move |event| {
            let typed = event_target_value(&event);
            arguments.update(|held| fill(held, index, typed));
        }),
    )
}

/// Sets the argument at `index`, growing the list to reach it.
fn fill(held: &mut Vec<String>, index: usize, value: String) {
    if held.len() <= index {
        held.resize(index + 1, String::new());
    }
    if let Some(slot) = held.get_mut(index) {
        *slot = value;
    }
}

/// The code system one clause reads codes from, and the version it is pinned
/// to.
///
/// Every control carries the clause's key in its `id`, because a screen draws
/// as many of these as the compose has clauses and a label points at one
/// control (<https://www.w3.org/TR/wai-aria-1.2/#namecalculation>).
fn system_picker(editing: Editing, offers: Offers, key: u32, clause: Memo<Clause>) -> AnyView {
    let system = select_control(
        Named {
            id: format!("compose-system-{key}"),
            name: "system",
            label: "Code system",
            note: "compose.include.system, the code system the codes below are read from.",
        },
        Signal::derive(move || clause.with(|clause| clause.system.clone())),
        "Choose one this server serves",
        options_of(move || offers.systems()),
        Box::new(move |event| {
            let chosen = event_target_value(&event);
            editing.change_clause(key, move |clause| clause.system = chosen);
        }),
    );
    let version = text_control(
        format!("compose-system-version-{key}"),
        "systemVersion",
        fixed("Code system version"),
        fixed("compose.include.version. Left empty, the server resolves the version itself."),
        Signal::derive(move || clause.with(|clause| clause.system_version.clone())),
        Box::new(move |event| {
            let typed = event_target_value(&event);
            editing.change_clause(key, move |clause| clause.system_version = typed);
        }),
    );
    view! { <div class="grid gap-default sm:grid-cols-2">{system} {version}</div> }.into_any()
}

/// The codes one clause names, and the search that picks them.
fn concept_rows(
    editing: Editing,
    offers: Offers,
    key: u32,
    clause: Memo<Clause>,
    included: bool,
) -> AnyView {
    let rows: Memo<Vec<u32>> = Memo::new(move |_| {
        clause.with(|clause| clause.concepts.iter().map(|row| row.key).collect())
    });
    let listed = view! {
        <For
            each=move || rows.get()
            key=|row| *row
            children=move |row| concept_row(editing, key, row, clause, included)
        />
    }
    .into_any();
    group(
        "Codes",
        vec![
            view! { <div class="grid gap-default">{listed}</div> }.into_any(),
            concept_search(editing, offers, key, clause),
        ],
    )
}

/// One named code, with the display its author gives it.
///
/// An exclude carries no display control: "Any display names specified for the
/// codes are ignored" there (<https://hl7.org/fhir/R4B/valueset.html>,
/// `ValueSet.compose.exclude`).
fn concept_row(
    editing: Editing,
    key: u32,
    row: u32,
    clause: Memo<Clause>,
    included: bool,
) -> AnyView {
    let picked = Memo::new(move |_| {
        clause.with(|clause| {
            clause
                .concepts
                .iter()
                .find(|held| held.key == row)
                .map(|held| (held.code.clone(), held.served_display.clone()))
                .unwrap_or_default()
        })
    });
    let display = included.then(|| {
        text_control(
            format!("compose-display-{key}-{row}"),
            "display",
            fixed("The display this value set gives that code"),
            fixed("compose.include.concept.display, which overrides the code system's own."),
            Signal::derive(move || {
                clause.with(|clause| {
                    clause
                        .concepts
                        .iter()
                        .find(|held| held.key == row)
                        .map(|held| held.display.clone())
                        .unwrap_or_default()
                })
            }),
            Box::new(move |event| {
                let typed = event_target_value(&event);
                editing.change_clause(key, move |clause| {
                    if let Some(held) = clause.concepts.iter_mut().find(|held| held.key == row) {
                        held.display = typed;
                    }
                });
            }),
        )
    });
    let control = view! {
        <div class="grid gap-tight">
            <p class=styles::CODE>
                {move || picked.with(|(code, _)| code.clone())} " "
                <span class=styles::CODE_MUTED>
                    {move || picked.with(|(_, served)| served.clone())}
                </span>
            </p>
            {display}
        </div>
    }
    .into_any();
    removable(
        control,
        Box::new(move |_| {
            editing.change_clause(key, move |clause| {
                clause.concepts.retain(|held| held.key != row);
            });
        }),
    )
}

/// The search that picks a code out of the clause's own code system.
///
/// A code is picked from what the server answers rather than typed, so a
/// clause cannot name a code the system does not hold. The search is the same
/// `$expand` over an inline value set the concept browser uses.
fn concept_search(editing: Editing, offers: Offers, key: u32, clause: Memo<Clause>) -> AnyView {
    code_search(
        editing,
        Search {
            picking: offers.picking,
            term: offers.term,
        },
        key,
        clause,
        PickInto::Concepts,
    )
}

/// Which clause one code search is open for, and what it is looking for.
#[derive(Clone, Copy)]
struct Search {
    /// The clause whose search is open, when one is.
    picking: RwSignal<Option<u32>>,
    /// What that search is looking for.
    term: RwSignal<String>,
}

/// Where a code picked in a search goes.
#[derive(Clone, Copy)]
enum PickInto {
    /// Into the clause's own codes, `compose.include.concept`.
    Concepts,
    /// Into the first placeholder of an implicit value set template.
    Argument(RwSignal<Vec<String>>),
}

/// A code search over the clause's code system, handing the picked code to
/// `into`.
fn code_search(
    editing: Editing,
    offers: Search,
    key: u32,
    clause: Memo<Clause>,
    into: PickInto,
) -> AnyView {
    let (search_id, label) = match into {
        PickInto::Concepts => (
            format!("compose-search-{key}"),
            "Find a code in that code system",
        ),
        PickInto::Argument(_) => (
            format!("compose-implicit-search-{key}"),
            "Find the code for that template",
        ),
    };
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let system = Memo::new(move |_| clause.with(|held| held.system.clone()));
    let query: Memo<Option<ConceptQuery>> = Memo::new(move |_| {
        if offers.picking.get() != Some(key) {
            return None;
        }
        let system = system.get();
        let term = offers.term.get();
        let system_version = clause.with(|held| held.system_version.clone());
        // An empty filter lists the first concepts the system answers, which
        // is what the concept browser's search does and what gives a reader
        // who knows no term something to pick from
        // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
        (!system.is_empty()).then(|| ConceptQuery {
            system,
            system_version: Some(system_version).filter(|held| !held.is_empty()),
            filter: Some(term).filter(|term| !term.is_empty()),
            child_of: None,
            display_language: None,
            count: SEARCH_COUNT,
        })
    });
    let found = Resource::new(
        move || (version.get(), query.get()),
        move |(version, query)| {
            let client = client.clone();
            async move {
                match query {
                    Some(query) => Some(client.expand_inline(version, &query).await),
                    None => None,
                }
            }
        },
    );
    let typed: NodeRef<Input> = NodeRef::new();
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let asked = typed.get().map(|input| input.value()).unwrap_or_default();
        offers.picking.set(Some(key));
        offers.term.set(asked.trim().to_owned());
    };
    let results = move || {
        found.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .map(|result| match result {
                    Ok(value) => found_view(editing, key, value, into),
                    Err(error) => {
                        view! { <Failure error=Signal::stored(error.clone()) /> }.into_any()
                    }
                })
        })
    };
    view! {
        <div class="grid gap-tight">
            <form class="flex items-end gap-default" on:submit=submit>
                <div class="grid min-w-0 flex-1 gap-tight">
                    <label for=search_id.clone() class=styles::LABEL>
                        {label}
                    </label>
                    <input
                        id=search_id
                        name="q"
                        type="search"
                        class=styles::INPUT
                        node_ref=typed
                        disabled=move || system.with(String::is_empty)
                    />
                </div>
                <button
                    type="submit"
                    class=styles::BUTTON
                    disabled=move || system.with(String::is_empty)
                >
                    {ferrohealth_ui::icon::icon(icon::SEARCH, "16")}
                    "Search"
                </button>
            </form>
            <Show when=move || offers.picking.get() == Some(key) fallback=|| ()>
                <Reading label="Searching that code system">{results}</Reading>
            </Show>
        </div>
    }
    .into_any()
}

/// The concepts a search answered, each with the control that picks it.
fn found_view(editing: Editing, key: u32, value: &ExpandedValueSet, into: PickInto) -> AnyView {
    let Some(expansion) = value.expansion() else {
        return crate::components::state::empty(
            "The server answered no expansion for that search.",
        );
    };
    if expansion.concepts.is_empty() {
        return crate::components::state::empty(
            "No concept of that code system matches that text.",
        );
    }
    let rows: Vec<AnyView> = expansion
        .concepts
        .iter()
        .map(|concept| {
            let code = concept.code.clone();
            let display = concept.display.clone().unwrap_or_default();
            let shown_code = code.clone();
            let shown_display = display.clone();
            let add = match into {
                PickInto::Concepts => act(
                    "Add",
                    Box::new(move |_| {
                        let code = code.clone();
                        let display = display.clone();
                        editing.change(move |draft| draft.add_concept(key, &code, &display));
                    }),
                ),
                PickInto::Argument(arguments) => act(
                    "Use",
                    Box::new(move |_| {
                        let code = code.clone();
                        arguments.update(|held| fill(held, 0, code));
                    }),
                ),
            };
            view! {
                <li class="flex items-center justify-between gap-default">
                    <span class=styles::CODE>
                        {shown_code} " " <span class=styles::CODE_MUTED>{shown_display}</span>
                    </span>
                    {add}
                </li>
            }
            .into_any()
        })
        .collect();
    view! { <ul class="grid gap-tight">{rows}</ul> }.into_any()
}

/// The filters one clause selects with, drawn from what the version declares.
fn filter_rows(
    editing: Editing,
    offers: Offers,
    key: u32,
    clause: Memo<Clause>,
    system: Memo<String>,
) -> AnyView {
    let rows: Memo<Vec<u32>> = Memo::new(move |_| {
        clause.with(|clause| clause.filters.iter().map(|row| row.key).collect())
    });
    // A clause pinning a code system version is answered against that version,
    // so the offers are that version's own
    // (<https://hl7.org/fhir/R4B/terminologycapabilities.html>).
    let declared = Memo::new(move |_| {
        let pinned = clause.with(|clause| clause.system_version.clone());
        offers.filters(&system.get(), &pinned)
    });
    let listed = view! {
        <For
            each=move || rows.get()
            key=|row| *row
            children=move |row| filter_row(editing, key, row, clause)
        />
    }
    .into_any();
    let chosen = RwSignal::new(String::new());
    let add = act(
        "Add",
        Box::new(move |_| {
            let picked = chosen.get_untracked();
            let Some((property, operator)) = picked.split_once(' ') else {
                return;
            };
            let property = property.to_owned();
            let operator = operator.to_owned();
            editing.change(move |draft| draft.add_filter(key, &property, &operator));
            chosen.set(String::new());
        }),
    );
    let control = select_with(
        Named {
            id: format!("compose-add-filter-{key}"),
            name: "addFilter",
            label: "Add a filter this server declares",
            note: "compose.include.filter, over a property this code system version declares.",
        },
        chosen.into(),
        "Choose a property and an operator",
        options_of(move || {
            declared
                .get()
                .into_iter()
                .flat_map(|filter| {
                    let code = filter.code;
                    filter.operators.into_iter().map(move |operator| {
                        let named = format!("{code} {operator}");
                        Choice {
                            canonical: named.clone(),
                            label: named,
                        }
                    })
                })
                .collect()
        }),
        Box::new(move |event| chosen.set(event_target_value(&event))),
        Some(add),
    );
    let adder = view! {
        <div class="grid gap-tight">
            {control}
            <Show
                when=move || !system.with(String::is_empty) && declared.with(Vec::is_empty)
                fallback=|| ()
            >
                <p class=styles::HINT>
                    "This server declares no filter for that code system version."
                </p>
            </Show>
        </div>
    }
    .into_any();
    group(
        "Filters",
        vec![
            view! { <div class="grid gap-default">{listed}</div> }.into_any(),
            adder,
        ],
    )
}

/// One filter, with the value it applies.
fn filter_row(editing: Editing, key: u32, row: u32, clause: Memo<Clause>) -> AnyView {
    let held = Memo::new(move |_| {
        clause.with(|clause| {
            clause
                .filters
                .iter()
                .find(|filter| filter.key == row)
                .map(|filter| {
                    (
                        filter.property.clone(),
                        filter.op.clone(),
                        filter.value.clone(),
                    )
                })
                .unwrap_or_default()
        })
    });
    let control = text_control(
        format!("compose-filter-{key}-{row}"),
        "filterValue",
        Signal::derive(move || {
            held.with(|(property, operator, _)| format!("{property} {operator}"))
        }),
        fixed("The value the operator is applied with."),
        Signal::derive(move || held.with(|(_, _, value)| value.clone())),
        Box::new(move |event| {
            let typed = event_target_value(&event);
            editing.change_clause(key, move |clause| {
                if let Some(filter) = clause.filters.iter_mut().find(|held| held.key == row) {
                    filter.value = typed;
                }
            });
        }),
    );
    removable(
        control,
        Box::new(move |_| {
            editing.change_clause(key, move |clause| {
                clause.filters.retain(|held| held.key != row);
            });
        }),
    )
}

/// The save control, and what a refusal does to the screen.
#[expect(
    clippy::too_many_arguments,
    reason = "the save carries the client, the address, the draft, the rights, and the three signals a refusal writes"
)]
fn save_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    editing: Editing,
    writable: Memo<bool>,
    session: Session,
    report: RwSignal<String>,
    refusal: RwSignal<Option<FhirError>>,
    saves: RwSignal<u32>,
    previewed: RwSignal<Option<Draft>>,
) -> AnyView {
    let client = client.clone();
    let navigate = StoredValue::new(use_navigate());
    let saving = RwSignal::new(false);
    let savable = editing.part(Draft::savable);
    let save = move |_| {
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        refusal.set(None);
        report.set(String::from("Saving"));
        let client = client.clone();
        let version = version.get_untracked();
        let draft = editing.draft();
        let body = draft.resource().to_string();
        spawn_local(async move {
            let written = if draft.saved() {
                Box::pin(
                    client.update(
                        version,
                        VALUE_SET,
                        &draft.id,
                        &body,
                        Some(&draft.version_id)
                            .filter(|held| !held.is_empty())
                            .map(String::as_str),
                    ),
                )
                .await
            } else {
                Box::pin(client.create(version, VALUE_SET, &body)).await
            };
            saving.set(false);
            match written {
                Ok(answer) => {
                    let id = answer
                        .resource
                        .as_ref()
                        .and_then(|resource| resource.id.clone())
                        .unwrap_or_else(|| draft.id.clone());
                    report.set(String::from("Saved."));
                    editing.reopen();
                    previewed.set(None);
                    saves.update(|counted| *counted = counted.saturating_add(1));
                    if id != draft.id {
                        let target = base_url()
                            .segment(COMPOSE_PATH)
                            .query(VERSION_PARAM, version.segment())
                            .query(COMPOSER_ID_PARAM, &id)
                            .render("");
                        navigate.with_value(|navigate| {
                            navigate(
                                &target,
                                NavigateOptions {
                                    resolve: false,
                                    ..NavigateOptions::default()
                                },
                            );
                        });
                    }
                }
                Err(error) => {
                    let refused = Refusal::of(&error);
                    if refused.drops_the_token() {
                        session.release();
                    }
                    report.set(announced(&error));
                    refusal.set(Some(error));
                }
            }
        });
    };
    view! {
        // The control exists only where the held token opens it, because a
        // control the server would refuse is a worse answer than no control
        // (`app/ferroterm-viewer/src/auth/scopes.rs`). The banner above says
        // why there is none.
        <div class="mt-section flex flex-wrap items-center gap-default">
            <Show when=move || writable.get() fallback=|| ()>
                <button
                    type="button"
                    class=styles::SUBMIT
                    disabled=move || !savable.get() || saving.get()
                    on:click=save.clone()
                >
                    {move || if saving.get() { "Saving" } else { "Save this value set" }}
                </button>
            </Show>
            <Show when=move || writable.get() && !savable.get() fallback=|| ()>
                <p role="status" class=styles::HINT>
                    "A canonical and a clause that selects something are what a save needs."
                </p>
            </Show>
        </div>
    }
    .into_any()
}

/// What the server said when it refused a write, in its own words.
///
/// After a `412` the reload drops the edits and reads the value set again, so
/// the form and the version the next save states are the server's current
/// ones.
fn refusal_section(
    refusal: RwSignal<Option<FhirError>>,
    editing: Editing,
    saves: RwSignal<u32>,
    report: RwSignal<String>,
) -> AnyView {
    let shown = move || {
        refusal.with(|held| {
            held.clone()
                .map(|error| view! { <Failure error=error /> }.into_any())
        })
    };
    let press: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        editing.reopen();
        report.set(String::from(READING_AGAIN));
        saves.update(|counted| *counted = counted.saturating_add(1));
        focus_report(REPORT_ID);
    });
    let offer = reload_offer(refusal, press);
    view! { <div class="mt-default grid gap-default">{shown} {offer}</div> }.into_any()
}

/// The preview read: the answer to `$expand` over the compose on screen.
type Previewing = LocalResource<Option<Result<ExpandedValueSet, FhirError>>>;

/// Reads `$expand` over the draft the reader asked to preview.
fn previewing(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    previewed: RwSignal<Option<Draft>>,
    page: Memo<Preview>,
) -> Previewing {
    let client = client.clone();
    LocalResource::new(move || {
        let client = client.clone();
        let version = version.get();
        let draft = previewed.get();
        let page = page.get();
        async move {
            match draft {
                Some(draft) => Some(client.preview_compose(version, &draft, &page).await),
                None => None,
            }
        }
    })
}

/// What a preview answered, as the sentence the live region reads out.
fn announcing(expansion: Previewing) -> Memo<String> {
    Memo::new(move |_| {
        expansion.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .map(|result| match result {
                    Ok(value) => count_sentence(value),
                    Err(error) => summary_of(error),
                })
                .unwrap_or_default()
        })
    })
}

/// The preview: `$expand` over the compose as it stands, one page at a time.
fn preview_section(
    expansion: Previewing,
    previewed: RwSignal<Option<Draft>>,
    page: Memo<Preview>,
    editing: Editing,
    report: RwSignal<String>,
    id: Memo<String>,
) -> AnyView {
    let shown = move || {
        expansion.with(|answered| {
            answered.as_ref().map(|answered| match answered {
                None => crate::components::state::invitation(
                    "Run the preview to see what this definition selects. It expands the definition on screen, saved or not.",
                ),
                Some(Ok(value)) => preview_view(value, page, id),
                Some(Err(error)) => expansion_refusal(error, editing),
            })
        })
    };
    let controls = preview_controls(previewed, editing, page, report, id);
    view! {
        <section class="fui-card" aria-labelledby="compose-preview-heading">
            <h2 id="compose-preview-heading" class="fui-card-title">
                "The preview"
            </h2>
            {controls}
            <Reading label="Expanding the definition">{shown}</Reading>
        </section>
    }
    .into_any()
}

/// The preview's own controls: the text filter and the run.
fn preview_controls(
    previewed: RwSignal<Option<Draft>>,
    editing: Editing,
    page: Memo<Preview>,
    report: RwSignal<String>,
    id: Memo<String>,
) -> AnyView {
    let navigate = StoredValue::new(use_navigate());
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let filter: NodeRef<Input> = NodeRef::new();
    let seeded = Memo::new(move |_| page.with(|page| page.filter.clone().unwrap_or_default()));
    let run = move |event: SubmitEvent| {
        event.prevent_default();
        report.set(String::new());
        previewed.set(Some(editing.draft()));
        let typed = filter.get().map(|input| input.value()).unwrap_or_default();
        let target = address(
            &id.get_untracked(),
            version.get_untracked(),
            &Preview {
                filter: Some(typed.trim().to_owned()).filter(|typed| !typed.is_empty()),
                count: page.get_untracked().count,
                offset: 0,
            },
        );
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
    view! {
        <form class="mt-default flex items-end gap-default" on:submit=run>
            <div class="grid min-w-0 flex-1 gap-tight">
                <label for="compose-preview-filter" class=styles::LABEL>
                    "Filter the preview"
                </label>
                <input
                    id="compose-preview-filter"
                    name="filter"
                    type="search"
                    class=styles::INPUT
                    node_ref=filter
                    prop:value=move || seeded.get()
                />
            </div>
            <button type="submit" class=styles::SUBMIT>
                {ferrohealth_ui::icon::icon(icon::EXPAND, "16")}
                "Run the preview"
            </button>
        </form>
    }
    .into_any()
}

/// The screen's own address, with one preview page selected.
fn address(id: &str, version: FhirVersion, page: &Preview) -> String {
    let mut url = base_url()
        .segment(COMPOSE_PATH)
        .query(VERSION_PARAM, version.segment());
    if !id.is_empty() {
        url = url.query(COMPOSER_ID_PARAM, id);
    }
    if let Some(filter) = page.filter.as_ref() {
        url = url.query(FILTER_PARAM, filter);
    }
    url = url.query(COUNT_PARAM, &page.count.to_string());
    if page.offset > 0 {
        url = url.query(OFFSET_PARAM, &page.offset.to_string());
    }
    url.render("")
}

/// The page the preview answered, with the walk through the rest of it.
fn preview_view(value: &ExpandedValueSet, page: Memo<Preview>, id: Memo<String>) -> AnyView {
    let Some(expansion) = value.expansion() else {
        return crate::components::state::empty(
            "The server answered a ValueSet carrying no expansion.",
        );
    };
    let rows: Vec<AnyView> = expansion
        .concepts
        .iter()
        .map(|concept| {
            view! {
                <tr>
                    <td class=styles::TD_TIGHT>
                        <span class=styles::CODE>{concept.code.clone()}</span>
                    </td>
                    <td class=styles::TD>{concept.display.clone().unwrap_or_default()}</td>
                </tr>
            }
            .into_any()
        })
        .collect();
    let total = expansion.total;
    let listed = expansion.listed();
    let walk = pager(page, expansion.offset, total, listed, id);
    view! {
        <table class=format!("mt-default {}", styles::TABLE)>
            <thead>
                <tr>
                    <th scope="col" class=styles::TH>
                        "Code"
                    </th>
                    <th scope="col" class=styles::TH>
                        "Display"
                    </th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
        {walk}
    }
    .into_any()
}

/// The links that walk the preview's pages.
fn pager(
    page: Memo<Preview>,
    answered_offset: Option<u32>,
    total: Option<u32>,
    rows: u32,
    id: Memo<String>,
) -> AnyView {
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let current = page.get();
    let id = id.get();
    // The transition keeps the previous rows on screen while the next page
    // loads, so the walk follows the offset the server says it applied rather
    // than the one the address asks for
    // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
    let at = Page::at(answered_offset.unwrap_or(current.offset), current.count);
    let previous = at.previous();
    let next = match total {
        Some(total) => at.next(total),
        None => (rows >= at.count())
            .then(|| Page::at(at.offset().saturating_add(at.count()), at.count())),
    };
    let link = move |target: Page, label: &'static str| {
        let address = address(
            &id,
            version.get(),
            &Preview {
                filter: current.filter.clone(),
                count: target.count(),
                offset: target.offset(),
            },
        );
        view! {
            <a href=address class=styles::BUTTON>
                {label}
            </a>
        }
        .into_any()
    };
    let counted = match total {
        Some(total) => format!("{rows} of {total} concepts on this page."),
        None => format!("{rows} concepts on this page. The server declared no total."),
    };
    view! {
        <div class="mt-default flex items-center gap-default">
            {previous.map(|target| link(target, "Previous"))}
            {next.map(|target| link(target, "Next"))} <p class=styles::MUTED>{counted}</p>
        </div>
    }
    .into_any()
}

/// A refused preview: the server's outcome, and the character it points at.
fn expansion_refusal(error: &FhirError, editing: Editing) -> AnyView {
    let marked: Vec<AnyView> = error
        .outcome()
        .map(|outcome| outcome.issue.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(Issue::position)
        .filter_map(|(expression, column)| marked_expression(expression, column, editing))
        .collect();
    let shown = error.clone();
    view! {
        <div class="mt-default grid gap-default">
            <Failure error=Signal::stored(shown) />
            {marked}
        </div>
    }
    .into_any()
}

/// The expression the refusal is about, with the character it points at marked.
///
/// The filter and the column are the server's own, read from
/// `issue.expression` and the `operationoutcome-issue-col` extension, so
/// nothing here reads the diagnostic's words. The draft is read untracked:
/// this describes a refusal that has already arrived, so it must not
/// resubscribe the preview to the form.
fn marked_expression(expression: &str, column: u32, editing: Editing) -> Option<AnyView> {
    let (form, value, column) = editing.held().marked_value(expression, column)?;
    let (before, at, after) = mark(&value, column);
    Some(
        view! {
            <div class=format!("rounded-md p-loose {}", styles::NOTICE)>
                <p class=styles::LABEL>{format!("{form}, at character {column}")}</p>
                <p class=format!(
                    "mt-tight {}",
                    styles::CODE,
                )>{before} <span class="font-semibold underline">{at}</span> {after}</p>
            </div>
        }
        .into_any(),
    )
}

/// What a preview answered, as the sentence the live region reads out.
fn count_sentence(value: &ExpandedValueSet) -> String {
    let Some(expansion) = value.expansion() else {
        return String::from("The server answered no expansion for this definition.");
    };
    match expansion.total {
        Some(total) => format!("This definition selects {total} concepts."),
        None => format!(
            "{} concepts on this page. The server declared no total.",
            expansion.listed()
        ),
    }
}

/// What a refused preview says, as one sentence the live region reads out.
fn summary_of(error: &FhirError) -> String {
    let said = error
        .outcome()
        .map(OperationOutcome::lines)
        .unwrap_or_default()
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<String>>()
        .join(" ");
    if said.is_empty() {
        String::from("The server refused the preview.")
    } else {
        format!("The server refused the preview: {said}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_address_carries_the_page_it_shows() {
        assert_eq!(
            address(
                "local",
                FhirVersion::R4B,
                &Preview {
                    filter: Some(String::from("a b")),
                    count: 20,
                    offset: 40,
                }
            ),
            format!(
                "{}/{COMPOSE_PATH}?fhir=r4b&id=local&filter=a%20b&count=20&offset=40",
                crate::routes::UI_BASE
            ),
            "every value is percent-encoded into the query it belongs to"
        );
    }

    #[test]
    fn a_placeholder_argument_is_filled_in_place_and_the_list_grows_to_reach_it() {
        let mut held = Vec::new();
        fill(&mut held, 1, String::from("size"));
        assert_eq!(held, ["", "size"]);
        fill(&mut held, 0, String::from("https://t.example/1"));
        assert_eq!(held, ["https://t.example/1", "size"]);
    }

    #[test]
    fn an_unsaved_draft_addresses_no_resource() {
        let address = address(
            "",
            FhirVersion::R5,
            &Preview {
                filter: None,
                count: 20,
                offset: 0,
            },
        );
        assert!(!address.contains("id="), "{address}");
        assert!(!address.contains("offset="), "{address}");
        assert!(address.ends_with("count=20"), "{address}");
    }
}
