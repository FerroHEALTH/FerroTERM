// SPDX-License-Identifier: BUSL-1.1
//! The `$lookup` runner: one code, read in the code system it belongs to.
//!
//! The screen reads and never writes. Every parameter of a run lives in the
//! address, so a lookup is a link a reader can share and the back button
//! walks the runs.

use ferrohealth_ui::table::model::filled_columns;
use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::surface::{
    card, chip, cluster, facts_row, page_header, panel_body, stack, titled_panel,
};
use ferrohealth_ui::widget::help::{help_switch, provide_help};
use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::NavigateOptions;
use leptos_router::hooks::{use_navigate, use_query_map};

use crate::components::NOT_DECLARED;
use crate::components::failure::Failure;
use crate::components::field::{
    CANONICAL_WORDS, Field, LANGUAGE_EXAMPLE, VERSION_WORDS, group, picked_field, text_field,
    text_field_showing, versioned_row,
};
use crate::components::icon;
use crate::components::reading::Reading;
use crate::components::runs::History;
use crate::components::shell::SelectedVersion;
use crate::fhir::concept::{Concept, LookupAnswer, LookupRequest};
use crate::fhir::expansion::DesignationRow;
use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, FhirClient};
use crate::offers::{choices, published, versions};
use crate::routes::{LOOKUP_PATH, VERSION_PARAM, base_url};
use crate::runs::Run;

/// The address parameter carrying the code system the code belongs to.
const SYSTEM_PARAM: &str = "system";

/// The address parameter carrying that code system's version.
const SYSTEM_VERSION_PARAM: &str = "systemVersion";

/// The address parameter carrying the code to read.
const CODE_PARAM: &str = "code";

/// The address parameter carrying the language displays are wanted in.
const LANGUAGE_PARAM: &str = "displayLanguage";

/// The columns of the designation table.
const DESIGNATION_COLUMNS: [Column; 3] = [
    Column::text("value", "Designation"),
    Column::text("language", "Language"),
    Column::text("use", "Use"),
];

/// The columns of the property table.
const PROPERTY_COLUMNS: [Column; 3] = [
    Column::text("code", "Property"),
    Column::text("value", "Value"),
    Column::text("description", "Meaning"),
];

/// One run, as the address carries it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LookupRun {
    /// The code system canonical.
    system: String,
    /// The code system version, empty for the one the server resolves to.
    system_version: String,
    /// The code.
    code: String,
    /// The BCP 47 tag displays are wanted in.
    display_language: String,
}

impl LookupRun {
    /// Reads a run out of the address.
    fn read(query: &dyn Fn(&str) -> Option<String>) -> Self {
        let text = |name: &str| query(name).unwrap_or_default().trim().to_owned();
        Self {
            system: text(SYSTEM_PARAM),
            system_version: text(SYSTEM_VERSION_PARAM),
            code: text(CODE_PARAM),
            display_language: text(LANGUAGE_PARAM),
        }
    }

    /// The request this run sends, or `None` before a system and a code are
    /// both named.
    fn request(&self) -> Option<LookupRequest> {
        let named = |text: &str| (!text.is_empty()).then(|| text.to_owned());
        (!self.system.is_empty() && !self.code.is_empty()).then(|| LookupRequest {
            system: self.system.clone(),
            system_version: named(&self.system_version),
            code: self.code.clone(),
            display_language: named(&self.display_language),
        })
    }

    /// The address this run is: a parameter the reader left empty is left out.
    fn address(&self, version: FhirVersion) -> String {
        let mut url = base_url()
            .segment(LOOKUP_PATH)
            .query(VERSION_PARAM, version.segment());
        for (name, value) in [
            (SYSTEM_PARAM, &self.system),
            (SYSTEM_VERSION_PARAM, &self.system_version),
            (CODE_PARAM, &self.code),
            (LANGUAGE_PARAM, &self.display_language),
        ] {
            if !value.is_empty() {
                url = url.query(name, value);
            }
        }
        url.render("")
    }
}

/// Runs `CodeSystem/$lookup` and draws the concept the server answered.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn LookupPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let _help = provide_help();
    let query = use_query_map();
    let run: Signal<LookupRun> =
        Memo::new(move |_| query.with(|map| LookupRun::read(&|name| map.get(name)))).into();
    let made = Memo::new(move |_| {
        run.with(|run| {
            run.request().map(|_| Run {
                screen: "Look up".to_owned(),
                subject: run.code.clone(),
                address: run.address(version.get()),
            })
        })
    });
    let history = History::recording(made);
    let form = form_card(&client, version, run);
    let answer = answer_panel(&client, version, run);
    view! {
        <Title text="Look up a code" />
        {page_header(
            "Look up a code",
            Some(
                String::from(
                    "A code's display, designations and properties, from this server's $lookup.",
                ),
            ),
            (),
        )}
        {stack(
            view! {
                {form}
                {answer}
                {history.view()}
            },
        )}
    }
}

/// The form, which navigates to the run it describes.
fn form_card(client: &FhirClient, version: Signal<FhirVersion>, run: Signal<LookupRun>) -> AnyView {
    let systems_search = published(client, version, CODE_SYSTEM);
    let systems = choices(systems_search);
    let system_versions = versions(
        systems_search,
        Memo::new(move |_| run.with(|run| run.system.clone())),
    );
    let system: NodeRef<Input> = NodeRef::new();
    let system_version: NodeRef<Input> = NodeRef::new();
    let code: NodeRef<Input> = NodeRef::new();
    let language: NodeRef<Input> = NodeRef::new();
    let navigate = use_navigate();
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let typed = LookupRun {
            system: typed_value(system),
            system_version: typed_value(system_version),
            code: typed_value(code),
            display_language: typed_value(language),
        };
        navigate(
            &typed.address(version.get()),
            NavigateOptions {
                resolve: false,
                ..NavigateOptions::default()
            },
        );
    };
    let seeded = |read: fn(&LookupRun) -> String| Memo::new(move |_| run.with(read));
    let form = view! {
        <form class="fui-form" on:submit=submit>
            {system_group(system, system_version, run, systems, system_versions)}
            {group(
                "The code",
                vec![
                    view! {
                        <div class="fui-columns">
                            {text_field(
                                Field {
                                    id: "lookup-code",
                                    name: "code",
                                    label: "Code",
                                    hint: "The code to read, exactly as the system writes it.",
                                },
                                code,
                                seeded(|run| run.code.clone()),
                            )}
                            {text_field_showing(
                                Field {
                                    id: "lookup-display-language",
                                    name: "displayLanguage",
                                    label: "Display language",
                                    hint: "A BCP 47 tag. Left empty, the server picks its own display.",
                                },
                                language,
                                seeded(|run| run.display_language.clone()),
                                Some(LANGUAGE_EXAMPLE),
                            )}
                        </div>
                    }
                        .into_any(),
                ],
            )}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(icon::LOOKUP, "16")}
                    "Look up"
                </button>
                {help_switch()}
            </div>
        </form>
    };
    card("Parameters", form).into_any()
}

/// Which code system the code is read in, and its version.
fn system_group(
    system: NodeRef<Input>,
    system_version: NodeRef<Input>,
    run: Signal<LookupRun>,
    systems: Memo<Vec<crate::fhir::named::Choice>>,
    system_versions: Memo<Vec<crate::fhir::named::Choice>>,
) -> AnyView {
    let seeded = |read: fn(&LookupRun) -> String| Memo::new(move |_| run.with(read));
    let row = view! {
        {versioned_row(
            vec![
                picked_field(
                    Field {
                        id: "lookup-system",
                        name: "system",
                        label: "Code system",
                        hint: "The system parameter of $lookup. The list is what this root publishes.",
                    },
                    system,
                    seeded(|run| run.system.clone()),
                    systems,
                    CANONICAL_WORDS,
                ),
                picked_field(
                    Field {
                        id: "lookup-system-version",
                        name: "systemVersion",
                        label: "Version",
                        hint: "The version parameter. Left on the first choice, the server answers from the version it resolves to.",
                    },
                    system_version,
                    seeded(|run| run.system_version.clone()),
                    system_versions,
                    VERSION_WORDS,
                ),
            ],
        )}
    }
    .into_any();
    group("The code system", vec![row])
}

/// The answer: the concept the server read, or why it read none.
fn answer_panel(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    run: Signal<LookupRun>,
) -> AnyView {
    let request = Memo::new(move |_| run.with(LookupRun::request));
    let client = client.clone();
    let answer = Resource::new(
        move || (version.get(), request.get()),
        move |(version, request)| {
            let client = client.clone();
            async move {
                match request {
                    Some(request) => Some(client.lookup(version, &request).await),
                    None => None,
                }
            }
        },
    );
    let drawn = move || {
        let at = version.get();
        let asked = run.get();
        answer.with(|answered| {
            answered.as_ref().map(|answer| match answer {
                None => titled_panel(
                    "The concept",
                    (),
                    panel_body(empty_quiet(
                        icon::LOOKUP,
                        "No code read yet",
                        "Choose a code system, type a code and look it up.",
                    )),
                )
                .into_any(),
                Some(Ok(answer)) => concept_view(answer, &asked, at),
                Some(Err(error)) => {
                    let error = error.clone();
                    titled_panel(
                        "The concept",
                        (),
                        panel_body(view! { <Failure error=Signal::stored(error) /> }),
                    )
                    .into_any()
                }
            })
        })
    };
    view! {
        <section aria-labelledby="lookup-answer-heading" class="fui-stack">
            <h2 id="lookup-answer-heading" class="fui-sr-only">
                "The concept"
            </h2>
            <Reading label="Looking the code up">{drawn}</Reading>
        </section>
    }
    .into_any()
}

/// What the server answered about the code.
fn concept_view(answer: &LookupAnswer, asked: &LookupRun, version: FhirVersion) -> AnyView {
    let concept = answer.concept();
    let display = concept
        .display
        .clone()
        .unwrap_or_else(|| String::from("The server answered no display"));
    let facts = facts_row(vec![
        ("Code", view! { <span class="fui-mono">{asked.code.clone()}</span> }.into_any()),
        ("Code system", concept.name.clone().unwrap_or_else(|| asked.system.clone()).into_any()),
        (
            "Version",
            view! { <span class="fui-mono">{concept.version.clone().unwrap_or_else(|| String::from("—"))}</span> }
                .into_any(),
        ),
    ]);
    let relatives = relatives_view(&concept, asked, version);
    let summary = view! {
        <div class="fui-panel-body fui-stack">
            <p class="text-title font-semibold" aria-live="polite">
                {display}
            </p>
            {facts}
            {relatives}
        </div>
    };
    view! {
        {titled_panel("The concept", (), summary)}
        {titled_panel("Designations", (), designations_table(&concept.designations))}
        {titled_panel("Properties", (), properties_table(&concept))}
    }
    .into_any()
}

/// The concept's parents and children, each a lookup of its own.
fn relatives_view(concept: &Concept, asked: &LookupRun, version: FhirVersion) -> AnyView {
    let link = |code: String| {
        let run = LookupRun {
            code: code.clone(),
            ..asked.clone()
        };
        view! {
            <a
                href=run.address(version)
                class="fui-button fui-button-secondary fui-button-small fui-mono"
            >
                {code}
            </a>
        }
        .into_any()
    };
    let parents: Vec<AnyView> = concept.parents().into_iter().map(link).collect();
    let children: Vec<AnyView> = concept.children().into_iter().map(link).collect();
    if parents.is_empty() && children.is_empty() {
        return ().into_any();
    }
    let row = |label: &'static str, codes: Vec<AnyView>| {
        (!codes.is_empty()).then(|| {
            view! {
                <div class="fui-cluster">
                    <span class="fui-label">{label}</span>
                    {codes}
                </div>
            }
        })
    };
    view! {
        <nav aria-label="Relatives of this concept" class="fui-stack">
            {row("Parents", parents)}
            {row("Children", children)}
        </nav>
    }
    .into_any()
}

/// Every designation the server holds for the concept.
fn designations_table(designations: &[DesignationRow]) -> AnyView {
    // A column no designation fills says nothing, so it is not drawn.
    let columns = filled_columns(
        &DESIGNATION_COLUMNS,
        designations,
        &["value"],
        |row, key| match key {
            "language" => row.language.is_some(),
            "use" => row.usage.is_some(),
            _ => true,
        },
    );
    let uses = columns.iter().any(|column| column.key == "use");
    let languages = columns.iter().any(|column| column.key == "language");
    let body = if designations.is_empty() {
        Body::Empty(
            empty_quiet(
                icon::LOOKUP,
                "No designation",
                "The server answered no designation.",
            )
            .into_any(),
        )
    } else {
        Body::Rows(
            designations
                .iter()
                .map(|designation| {
                    view! {
                        <tr>
                            <td>{designation.value.clone()}</td>
                            {languages
                                .then(|| {
                                    view! {
                                        <td data-label="Language" class="fui-mono">
                                            {designation
                                                .language
                                                .clone()
                                                .unwrap_or_else(|| String::from("—"))}
                                        </td>
                                    }
                                })}
                            {uses
                                .then(|| {
                                    view! {
                                        <td data-label="Use">
                                            {designation
                                                .usage
                                                .clone()
                                                .unwrap_or_else(|| String::from("—"))}
                                        </td>
                                    }
                                })}
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption: "Designations",
        columns: &columns,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// Every property the server answered, in its own order.
fn properties_table(concept: &Concept) -> AnyView {
    // A column no property fills says nothing, so it is not drawn.
    let columns = filled_columns(
        &PROPERTY_COLUMNS,
        &concept.properties,
        &["code", "value"],
        |row, key| key != "description" || row.description.is_some(),
    );
    let meanings = columns.iter().any(|column| column.key == "description");
    let body = if concept.properties.is_empty() {
        Body::Empty(
            empty_quiet(
                icon::LOOKUP,
                "No property",
                "The server answered no property.",
            )
            .into_any(),
        )
    } else {
        Body::Rows(
            concept
                .properties
                .iter()
                .map(|property| {
                    let value = property.value.clone().map_or_else(
                        || NOT_DECLARED.into_any(),
                        |value| {
                            if crate::components::is_date_time(&value) {
                                crate::components::stated(value)
                            } else {
                                chip(value).into_any()
                            }
                        },
                    );
                    view! {
                        <tr>
                            <td class="fui-mono">{property.code.clone()}</td>
                            <td data-label="Value">{cluster(value)}</td>
                            {meanings
                                .then(|| {
                                    view! {
                                        <td data-label="Meaning" class="fui-muted">
                                            {property.description.clone().unwrap_or_default()}
                                        </td>
                                    }
                                })}
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption: "Properties",
        columns: &columns,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// What a control holds now, trimmed, or the empty string when it is gone.
fn typed_value(node: NodeRef<Input>) -> String {
    node.get()
        .map(|input| input.value())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + use<'a> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn a_run_needs_a_system_and_a_code_and_sends_only_what_was_named() {
        let partial = LookupRun::read(&query(&[("system", "urn:a")]));
        assert_eq!(partial.request(), None, "no code, no request");
        let run = LookupRun::read(&query(&[("system", "urn:a"), ("code", " x ")]));
        let request = run.request().expect("a request");
        assert_eq!(request.code, "x");
        assert_eq!(request.system_version, None);
        assert_eq!(request.display_language, None);
    }

    #[test]
    fn a_run_round_trips_through_its_address() {
        let run = LookupRun {
            system: String::from("https://terminology.example/a?x=1"),
            system_version: String::from("2"),
            code: String::from("c 1"),
            display_language: String::new(),
        };
        let address = run.address(FhirVersion::R5);
        let pairs: Vec<(String, String)> = address
            .split_once('?')
            .map(|(_, query)| {
                query
                    .split('&')
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(name, value)| {
                        (name.to_owned(), crate::url::decode_query_component(value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let read = LookupRun::read(&|name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        });
        assert_eq!(read, run);
        assert!(
            !address.contains("displayLanguage"),
            "an empty parameter is left out"
        );
    }
}
