// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Server: what each FHIR root declares, side by side (M5), the operations
//! each declares and at which level, and the evidence this build ships.
//!
//! One process answers `/r4`, `/r4b`, `/r5` and `/r6`, each with its own
//! operation set, so the screen reads the four `CapabilityStatement`
//! documents and draws what it finds in them; it names no version's
//! behaviour of its own. The evidence was read out of the repository's
//! committed files when the console was built, so it describes the build and
//! never the deployment.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::root_matrix::{MatrixRoot, MatrixRow, RootMatrix, root_matrix};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{SectionHead, panel_section, panel_sections, section_intro};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::widget::action_menu::{Act, Action, Item, Menu};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::comparison::{Cell, Comparison, RootAnswer, Row, compare, notes, state_sentence};
use crate::evidence::{Conformance, Latency, embedded};
use crate::fhir::error::FhirError;
use crate::fhir::problem::Problem;
use crate::fhir::sync::Reloaded;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::dashboard::host;
use crate::pages::parts::{failure, heading, loaded};
use crate::posted::{INTENT, posted};
use crate::request::{Request, request};
use crate::routes::{SERVER_PATH, SERVER_ROUTE, ui_path};

/// The intent of the Reload the served set form.
const RELOAD: &str = "reload";

/// The id of the reload confirmation.
const RELOAD_DIALOG: &str = "server-reload";

/// The query parameter carrying the status a reload was answered with.
const RELOADED_PARAM: &str = "reloaded";

/// The three tabs of the screen, each a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServerTab {
    /// The four roots side by side.
    Versions,
    /// The operations each root declares.
    Operations,
    /// The evidence this build ships.
    Evidence,
}

impl ServerTab {
    /// The tab the route's segment names: none for FHIR versions.
    #[must_use]
    pub(crate) fn of(segment: Option<&str>) -> Option<Self> {
        match segment {
            None => Some(Self::Versions),
            Some("operations") => Some(Self::Operations),
            Some("evidence") => Some(Self::Evidence),
            Some(_) => None,
        }
    }

    /// The tab's key, its route's segment after the screen's.
    const fn key(self) -> &'static str {
        match self {
            Self::Versions => "versions",
            Self::Operations => "operations",
            Self::Evidence => "evidence",
        }
    }

    /// The tab's route.
    fn href(self) -> String {
        match self {
            Self::Versions => ui_path(SERVER_PATH),
            Self::Operations | Self::Evidence => format!("{}/{}", ui_path(SERVER_PATH), self.key()),
        }
    }
}

/// The binder tabs over `panel`, `current` raised.
fn tabbed(current: ServerTab, panel: AnyView) -> AnyView {
    let tabs = [
        (ServerTab::Versions, Msg::ServerTabVersions),
        (ServerTab::Operations, Msg::ServerTabOperations),
        (ServerTab::Evidence, Msg::ServerTabEvidence),
    ]
    .into_iter()
    .map(|(tab, words)| Tab::new(tab.key(), tab.href(), tr(words)))
    .collect();
    binder(tr(Msg::ServerSections), tabs, current.key(), panel)
}

/// Server, on the tab the route names.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn ServerPage() -> impl IntoView {
    let request = request();
    let params = use_params_map();
    let segment = params.with_untracked(|params| params.get("tab"));
    let Some(tab) = ServerTab::of(segment.as_deref()) else {
        return crate::pages::errors::not_found_page();
    };
    let reloads = request.reloads();
    let header = heading(
        tr(Msg::ShellServer),
        Some(tr_with(
            Msg::ServerSubtitle,
            &[("host", host(&request.root).into())],
        )),
        Vec::new(),
        if reloads {
            reload_actions()
        } else {
            ActionGroup::none()
        },
    );
    let form = posted().filter(|form| reloads && form.intent() == RELOAD);
    let reload = (reloads || form.is_some()).then(|| {
        let request = request.clone();
        loaded(async move {
            let answered = match form {
                Some(_) => Some(request.client.reload().await),
                None => None,
            };
            reload_outcome(&request, answered)
        })
    });
    let body = match tab {
        ServerTab::Evidence => tabbed(tab, evidence()),
        ServerTab::Versions | ServerTab::Operations => loaded(async move {
            let answers = read_roots(&request).await;
            let counts = system_counts(&request, &answers).await;
            tabbed(tab, roots_panel(&request, tab, &answers, &counts))
        }),
    };
    view! {
        {header}
        {reloads.then(reload_dialog)}
        {reload}
        {body}
    }
    .into_any()
}

/// The title row's actions: Reload the served set in More.
fn reload_actions() -> ActionGroup {
    ActionGroup::more_only(Menu::new(
        "server-more",
        tr(Msg::ServerMore),
        vec![Item::from(Action::new(
            Some(Glyph::REFRESH_CW),
            tr(Msg::ServerReload),
            Act::Dialog(String::from(RELOAD_DIALOG)),
        ))],
    ))
}

/// The confirmation Reload the served set opens, posting the reload to this
/// screen.
fn reload_dialog() -> AnyView {
    let confirm = action_button(
        Action::new(
            Some(Glyph::REFRESH_CW),
            tr(Msg::ServerReload),
            Act::Post {
                action: String::from(SERVER_ROUTE),
                fields: vec![(String::from(INTENT), String::from(RELOAD))],
            },
        ),
        Variant::Primary,
    );
    confirm_dialog(
        Confirmation {
            id: RELOAD_DIALOG.into(),
            title: tr(Msg::ServerReloadQuestion).into(),
            message: tr(Msg::ServerReloadMessage).into(),
        },
        confirm,
    )
    .into_any()
}

/// What a posted reload came to: a redirect naming the status when the
/// server took it, what it said when it refused, the failure when it could
/// not be reached, or the status a redirect named.
fn reload_outcome(request: &Request, answered: Option<Result<Reloaded, FhirError>>) -> AnyView {
    match answered {
        Some(Ok(reloaded)) if reloaded.status.is_success() => {
            leptos_axum::redirect(&format!(
                "{SERVER_ROUTE}?{RELOADED_PARAM}={}",
                reloaded.status.as_u16()
            ));
            ().into_any()
        }
        Some(Ok(reloaded)) => notice(
            Tone::Bad,
            Announce::Alert,
            None,
            tr_with(
                Msg::ServerReloadRefused,
                &[
                    ("status", reloaded.status.as_u16().to_string().into()),
                    (
                        "body",
                        Problem::read(&reloaded.body)
                            .and_then(|problem| problem.detail.or(problem.title))
                            .unwrap_or(reloaded.body)
                            .into(),
                    ),
                ],
            ),
        )
        .into_any(),
        Some(Err(error)) => failure(&error),
        None => request.param(RELOADED_PARAM).map_or_else(
            || ().into_any(),
            |status| {
                notice(
                    Tone::Ok,
                    Announce::Status,
                    None,
                    tr_with(Msg::ServerReloaded, &[("status", status.into())]),
                )
                .into_any()
            },
        ),
    }
}

/// What each root answered, in release order.
async fn read_roots(request: &Request) -> Vec<(FhirVersion, RootAnswer)> {
    let mut answers = Vec::new();
    for version in FhirVersion::ALL {
        let answer = RootAnswer::read(request.client.capability_statement(version).await);
        answers.push((version, answer));
    }
    answers
}

/// How many code systems each root's `TerminologyCapabilities` declares,
/// `None` for a root that did not answer.
async fn system_counts(
    request: &Request,
    answers: &[(FhirVersion, RootAnswer)],
) -> Vec<Option<usize>> {
    let mut counts = Vec::new();
    for (version, answer) in answers {
        let count = if answer.failure().is_some() {
            None
        } else {
            request
                .client
                .terminology_capabilities(*version)
                .await
                .ok()
                .map(|capabilities| capabilities.cards().len())
        };
        counts.push(count);
    }
    counts
}

/// A comparison cell in words, since a tint carries no meaning.
fn cell_view(found: &Cell) -> Option<AnyView> {
    match found {
        Cell::Stated(text) => Some(text.clone().into_any()),
        Cell::NotDeclared => None,
        Cell::Unread => Some(tr(Msg::ServerNoAnswer).into_any()),
    }
}

/// The comparison `rows` as matrix rows, an operation named in the code
/// face.
fn matrix_rows(rows: &[Row]) -> Vec<MatrixRow> {
    rows.iter()
        .map(|row| {
            let name = if row.label.contains('$') {
                view! { <code>{row.label.clone()}</code> }.into_any()
            } else {
                row.label.clone().into_any()
            };
            MatrixRow {
                name,
                cells: row.cells.iter().map(cell_view).collect(),
            }
        })
        .collect()
}

/// The panel of the FHIR versions or the operations tab.
fn roots_panel(
    request: &Request,
    tab: ServerTab,
    answers: &[(FhirVersion, RootAnswer)],
    counts: &[Option<usize>],
) -> AnyView {
    let comparison = compare(answers);
    let roots: Vec<MatrixRoot> = comparison
        .columns
        .iter()
        .zip(answers)
        .map(|(column, (_, answer))| MatrixRoot {
            name: column.version.label().to_owned(),
            release: column.fhir_version.as_ref().map_or_else(
                || column.version.release().to_owned(),
                |release| format!("FHIR {release}"),
            ),
            answered: answer.failure().is_none(),
        })
        .collect();
    let silent: Vec<&str> = answers
        .iter()
        .filter(|(_, answer)| answer.failure().is_some())
        .map(|(version, _)| version.label())
        .collect();
    let report = (!silent.is_empty()).then(|| {
        load_report(LoadReport {
            outcome: Outcome::Bad,
            arrival: Arrival::WithPage,
            title: tr_with(
                Msg::ServerRootsSilent,
                &[("roots", silent.join(", ").into())],
            )
            .into_any(),
            counts: Vec::new(),
            action: Some(
                view! {
                    <a href=request.back() class="fui-button fui-button-secondary fui-button-small">
                        {tr(Msg::ServerRetry)}
                    </a>
                }
                .into_any(),
            ),
        })
    });
    let (title, rows, intro) = match tab {
        ServerTab::Operations => operations_rows(&comparison, answers),
        ServerTab::Versions | ServerTab::Evidence => {
            version_rows(request, &comparison, answers, counts)
        }
    };
    let matrix = root_matrix(RootMatrix {
        caption: title.clone(),
        corner: tr(Msg::ServerDeclaration),
        roots,
        rows,
    });
    panel_sections(view! {
        {panel_section(
            None,
            view! {
                {section_intro(intro)}
                {report}
            },
        )}
        {panel_section(
            Some(SectionHead {
                title,
                count: None,
                actions: None,
            }),
            matrix,
        )}
    })
}

/// The Operations tab's matrix: its title, its rows (an operation a root
/// adds marked as its own) and its intro.
fn operations_rows(
    comparison: &Comparison,
    answers: &[(FhirVersion, RootAnswer)],
) -> (String, Vec<MatrixRow>, String) {
    let mut rows = matrix_rows(&comparison.operations);
    let own: Vec<(String, String)> = answers
        .iter()
        .flat_map(|(_, answer)| notes(answer))
        .collect();
    rows.iter_mut()
        .zip(&comparison.operations)
        .for_each(|(row, operation)| {
            if own.iter().any(|(label, _)| *label == operation.label) {
                let label = operation.label.clone();
                row.name = view! {
                    <code>{label}</code>
                    " "
                    <span class="fui-small fui-muted">{tr(Msg::ServerOwnOperation)}</span>
                }
                .into_any();
            }
        });
    (
        tr(Msg::ServerOperationsTitle),
        rows,
        tr(Msg::ServerOperationsIntro),
    )
}

/// The FHIR versions tab's matrix: its title, its rows (the base address and
/// the code system count after the release) and its intro.
fn version_rows(
    request: &Request,
    comparison: &Comparison,
    answers: &[(FhirVersion, RootAnswer)],
    counts: &[Option<usize>],
) -> (String, Vec<MatrixRow>, String) {
    let mut rows = matrix_rows(&comparison.facts);
    rows.insert(
        1,
        MatrixRow {
            name: tr(Msg::ServerBaseAddress).into_any(),
            cells: FhirVersion::ALL
                .into_iter()
                .map(|version| {
                    Some(view! { <code>{request.client.version_base(version)}</code> }.into_any())
                })
                .collect(),
        },
    );
    rows.insert(
        2,
        MatrixRow {
            name: tr(Msg::ServerCodeSystems).into_any(),
            cells: counts
                .iter()
                .map(|count| {
                    count.map(|count| {
                        ferrohealth_ui::i18n::number::grouped(
                            u64::try_from(count).unwrap_or(u64::MAX),
                        )
                        .into_any()
                    })
                })
                .collect(),
        },
    );
    (tr(Msg::ServerVersionsTitle), rows, state_sentence(answers))
}

/// The evidence this build ships: the ecosystem suite and the latency bars.
fn evidence() -> AnyView {
    let evidence = embedded();
    let line = tr_with(
        Msg::ServerEvidenceLine,
        &[("release", evidence.release.into())],
    );
    panel_sections(view! {
        {panel_section(None, section_intro(line))}
        {suite(evidence.conformance)}
        {latency(evidence.latency)}
    })
}

/// The HL7 terminology ecosystem suite, mode by mode.
fn suite(conformance: Conformance) -> AnyView {
    let columns = [
        Column::text("mode", tr(Msg::ServerMode))
            .role(Role::Title)
            .mono(),
        Column::text("surface", tr(Msg::ServerSurface)).mono(),
        Column::text("passed", tr(Msg::ServerPassed)).numeric(),
        Column::text("needs", tr(Msg::ServerNeeds)),
    ];
    let [_, surface, passed, needs] = &columns;
    let rows = conformance
        .modes
        .iter()
        .map(|mode| {
            let words = tr_with(
                Msg::ServerPassedOf,
                &[("passed", mode.passed.into()), ("ran", mode.ran.into())],
            );
            let mark = if mode.passed == mode.ran {
                Mark::Ok
            } else {
                Mark::Warn
            };
            let counted = state(mark, words, None);
            let content = if mode.needs == "nothing" {
                tr(Msg::ServerNeedsNothing)
            } else {
                mode.needs.to_owned()
            };
            view! {
                <tr>
                    {title_cell(None, view! { <code>{mode.name}</code> }, None)}
                    {cell(surface, view! { <code>{mode.surface}</code> })} {cell(passed, counted)}
                    {cell(needs, content)}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::ServerSuiteTitle);
    let table = data_table(DataTable {
        caption: &caption,
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(conformance.modes.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        view! {
            {section_intro(tr(Msg::ServerSuiteIntro))}
            {table}
        },
    )
}

/// The latency bars the project claims, and what one run measured.
fn latency(latency: Latency) -> AnyView {
    let columns = [
        Column::text("bench", tr(Msg::ServerBenchmark))
            .role(Role::Title)
            .mono(),
        Column::text("target", tr(Msg::ServerTarget)).numeric(),
        Column::text("measured", tr(Msg::ServerMeasured)).numeric(),
    ];
    let [_, target, measured] = &columns;
    let rows = latency
        .bars
        .iter()
        .map(|bar| {
            let micro = |value: u32| tr_with(Msg::ServerMicroseconds, &[("value", value.into())]);
            view! {
                <tr>
                    {title_cell(None, view! { <code>{bar.bench}</code> }, None)}
                    {cell(target, micro(bar.max_us))} {cell(measured, micro(bar.measured_us))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::ServerLatencyTitle);
    let table = data_table(DataTable {
        caption: &caption,
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(latency.bars.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        view! {
            {section_intro(
                tr_with(Msg::ServerLatencyIntro, &[("machine", latency.machine.into())]),
            )}
            {table}
        },
    )
}

#[cfg(test)]
mod tests {
    use super::ServerTab;

    #[test]
    fn each_tab_is_a_route() {
        assert_eq!(Some(ServerTab::Versions), ServerTab::of(None));
        assert_eq!(
            Some(ServerTab::Operations),
            ServerTab::of(Some("operations"))
        );
        assert_eq!(Some(ServerTab::Evidence), ServerTab::of(Some("evidence")));
        assert_eq!(
            None,
            ServerTab::of(Some("versions")),
            "the first tab is the screen's own route"
        );
    }
}
