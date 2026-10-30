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

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::root_matrix::{MatrixRoot, MatrixRow, RootMatrix, root_matrix};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{SectionHead, panel_section, panel_sections, section_intro};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::comparison::{Cell, Comparison, RootAnswer, Row, compare, notes, state_sentence};
use crate::evidence::{Conformance, Latency, embedded};
use crate::fhir::error::FhirError;
use crate::fhir::problem::Problem;
use crate::fhir::sync::Reloaded;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, join, t};
use crate::pages::dashboard::host;
use crate::pages::parts::{failure, heading};
use crate::posted::INTENT;
use crate::request::Request;
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
fn tabbed(locale: &Locale, current: ServerTab, panel: Html) -> Html {
    let tabs = [
        (ServerTab::Versions, Msg::ServerTabVersions),
        (ServerTab::Operations, Msg::ServerTabOperations),
        (ServerTab::Evidence, Msg::ServerTabEvidence),
    ]
    .into_iter()
    .map(|(tab, words)| Tab::new(tab.key(), tab.href(), tr(locale, words)))
    .collect();
    binder(
        locale,
        tr(locale, Msg::ServerSections),
        tabs,
        current.key(),
        panel,
    )
}

/// Server, on the tab the route names.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let Some(tab) = ServerTab::of(request.segment(1).as_deref()) else {
        return crate::pages::errors::not_found_page(&request);
    };
    let reloads = request.reloads();
    let header = heading(
        &request,
        tr(locale, Msg::ShellServer),
        Some(tr_with(
            locale,
            Msg::ServerSubtitle,
            &[("host", host(&request.root).into())],
        )),
        Vec::new(),
        if reloads {
            reload_actions(locale)
        } else {
            ActionGroup::none()
        },
    );
    let form = request
        .posted
        .clone()
        .filter(|form| reloads && form.intent() == RELOAD);
    let reload = if reloads || form.is_some() {
        let answered = match form {
            Some(_) => Some(request.client.reload().await),
            None => None,
        };
        Some(reload_outcome(&request, answered))
    } else {
        None
    };
    let body = match tab {
        ServerTab::Evidence => tabbed(locale, tab, evidence(locale)),
        ServerTab::Versions | ServerTab::Operations => {
            let answers = read_roots(&request).await;
            let counts = system_counts(&request, &answers).await;
            tabbed(locale, tab, roots_panel(&request, tab, &answers, &counts))
        }
    };
    Html::join([
        header,
        if reloads {
            reload_dialog(locale)
        } else {
            Html::empty()
        },
        reload.unwrap_or_default(),
        body,
    ])
}

/// The title row's actions: Reload the served set in More.
fn reload_actions(locale: &Locale) -> ActionGroup {
    ActionGroup::more_only(Menu::new(
        "server-more",
        tr(locale, Msg::ServerMore),
        vec![Item::new(Action::new(
            Some(Glyph::REFRESH_CW),
            tr(locale, Msg::ServerReload),
            Act::Dialog(String::from(RELOAD_DIALOG)),
        ))],
    ))
}

/// The confirmation Reload the served set opens, posting the reload to this
/// screen.
fn reload_dialog(locale: &Locale) -> Html {
    let confirm = action_button(
        locale,
        Action::new(
            Some(Glyph::REFRESH_CW),
            tr(locale, Msg::ServerReload),
            Act::Post {
                action: String::from(SERVER_ROUTE),
                fields: vec![(String::from(INTENT), String::from(RELOAD))],
            },
        ),
        Variant::Primary,
    );
    confirm_dialog(
        locale,
        Confirmation {
            id: RELOAD_DIALOG.into(),
            title: tr(locale, Msg::ServerReloadQuestion).into(),
            message: tr(locale, Msg::ServerReloadMessage).into(),
        },
        confirm,
    )
}

/// What a posted reload came to: a redirect naming the status when the
/// server took it, what it said when it refused, the failure when it could
/// not be reached, or the status a redirect named.
fn reload_outcome(request: &Request, answered: Option<Result<Reloaded, FhirError>>) -> Html {
    let locale = &request.locale;
    match answered {
        Some(Ok(reloaded)) if reloaded.status.is_success() => {
            request.answering.redirect(&format!(
                "{SERVER_ROUTE}?{RELOADED_PARAM}={}",
                reloaded.status.as_u16()
            ));
            Html::empty()
        }
        Some(Ok(reloaded)) => notice(
            Tone::Bad,
            Announce::Alert,
            None,
            t(tr_with(
                locale,
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
            )),
        ),
        Some(Err(error)) => failure(locale, &error),
        None => request
            .param(RELOADED_PARAM)
            .map(|status| {
                notice(
                    Tone::Ok,
                    Announce::Status,
                    None,
                    t(tr_with(
                        locale,
                        Msg::ServerReloaded,
                        &[("status", status.into())],
                    )),
                )
            })
            .unwrap_or_default(),
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
fn cell_view(locale: &Locale, found: &Cell) -> Option<Html> {
    match found {
        Cell::Stated(text) => Some(t(text)),
        Cell::NotDeclared => None,
        Cell::Unread => Some(t(tr(locale, Msg::ServerNoAnswer))),
    }
}

/// The comparison `rows` as matrix rows, an operation named in the code
/// face.
fn matrix_rows(locale: &Locale, rows: &[Row]) -> Vec<MatrixRow> {
    rows.iter()
        .map(|row| {
            let name = if row.label.contains('$') {
                code(&row.label)
            } else {
                t(&row.label)
            };
            MatrixRow {
                name,
                cells: row
                    .cells
                    .iter()
                    .map(|found| cell_view(locale, found))
                    .collect(),
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
) -> Html {
    let locale = &request.locale;
    let comparison = compare(locale, answers);
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
            title: t(tr_with(
                locale,
                Msg::ServerRootsSilent,
                &[("roots", silent.join(", ").into())],
            )),
            counts: Vec::new(),
            action: Some(h(format!(
                r#"<a href="{}" class="fui-button fui-button-secondary fui-button-small">{}</a>"#,
                t(request.back()),
                t(tr(locale, Msg::ServerRetry))
            ))),
        })
    });
    let (title, rows, intro) = match tab {
        ServerTab::Operations => operations_rows(locale, &comparison, answers),
        ServerTab::Versions | ServerTab::Evidence => {
            version_rows(request, &comparison, answers, counts)
        }
    };
    let matrix = root_matrix(
        locale,
        RootMatrix {
            caption: title.clone(),
            corner: tr(locale, Msg::ServerDeclaration),
            roots,
            rows,
        },
    );
    panel_sections(Html::join([
        panel_section(
            locale,
            None,
            Html::join([section_intro(intro), report.unwrap_or_default()]),
        ),
        panel_section(
            locale,
            Some(SectionHead {
                title,
                count: None,
                actions: None,
            }),
            matrix,
        ),
    ]))
}

/// The Operations tab's matrix: its title, its rows (an operation a root
/// adds marked as its own) and its intro.
fn operations_rows(
    locale: &Locale,
    comparison: &Comparison,
    answers: &[(FhirVersion, RootAnswer)],
) -> (String, Vec<MatrixRow>, String) {
    let mut rows = matrix_rows(locale, &comparison.operations);
    let own: Vec<(String, String)> = answers
        .iter()
        .flat_map(|(_, answer)| notes(answer))
        .collect();
    rows.iter_mut()
        .zip(&comparison.operations)
        .for_each(|(row, operation)| {
            if own.iter().any(|(label, _)| *label == operation.label) {
                row.name = h(format!(
                    r#"{} <span class="fui-small fui-muted">{}</span>"#,
                    code(&operation.label),
                    t(tr(locale, Msg::ServerOwnOperation))
                ));
            }
        });
    (
        tr(locale, Msg::ServerOperationsTitle),
        rows,
        tr(locale, Msg::ServerOperationsIntro),
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
    let locale = &request.locale;
    let mut rows = matrix_rows(locale, &comparison.facts);
    rows.insert(
        1,
        MatrixRow {
            name: t(tr(locale, Msg::ServerBaseAddress)),
            cells: FhirVersion::ALL
                .into_iter()
                .map(|version| Some(code(request.client.version_base(version))))
                .collect(),
        },
    );
    rows.insert(
        2,
        MatrixRow {
            name: t(tr(locale, Msg::ServerCodeSystems)),
            cells: counts
                .iter()
                .map(|count| {
                    count.map(|count| {
                        t(ferrohealth_ui::i18n::number::grouped(
                            locale,
                            u64::try_from(count).unwrap_or(u64::MAX),
                        ))
                    })
                })
                .collect(),
        },
    );
    (
        tr(locale, Msg::ServerVersionsTitle),
        rows,
        state_sentence(locale, answers),
    )
}

/// The evidence this build ships: the ecosystem suite and the latency bars.
fn evidence(locale: &Locale) -> Html {
    let evidence = embedded();
    let line = tr_with(
        locale,
        Msg::ServerEvidenceLine,
        &[("release", evidence.release.into())],
    );
    panel_sections(Html::join([
        panel_section(locale, None, section_intro(line)),
        suite(locale, evidence.conformance),
        latency(locale, evidence.latency),
    ]))
}

/// The HL7 terminology ecosystem suite, mode by mode.
fn suite(locale: &Locale, conformance: Conformance) -> Html {
    let columns = [
        Column::text("mode", tr(locale, Msg::ServerMode))
            .role(Role::Title)
            .mono(),
        Column::text("surface", tr(locale, Msg::ServerSurface)).mono(),
        Column::text("passed", tr(locale, Msg::ServerPassed)).numeric(),
        Column::text("needs", tr(locale, Msg::ServerNeeds)),
    ];
    let [_, surface, passed, needs] = &columns;
    let rows = join(conformance.modes.iter().map(|mode| {
        let words = tr_with(
            locale,
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
            tr(locale, Msg::ServerNeedsNothing)
        } else {
            mode.needs.to_owned()
        };
        h(format!(
            "<tr>{}{}{}{}</tr>",
            title_cell(None, code(mode.name), None),
            cell(surface, code(mode.surface)),
            cell(passed, counted),
            cell(needs, t(content)),
        ))
    }));
    let caption = tr(locale, Msg::ServerSuiteTitle);
    let table = data_table(DataTable {
        caption: &caption,
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(conformance.modes.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        Html::join([section_intro(tr(locale, Msg::ServerSuiteIntro)), table]),
    )
}

/// The latency bars the project claims, and what one run measured.
fn latency(locale: &Locale, latency: Latency) -> Html {
    let columns = [
        Column::text("bench", tr(locale, Msg::ServerBenchmark))
            .role(Role::Title)
            .mono(),
        Column::text("target", tr(locale, Msg::ServerTarget)).numeric(),
        Column::text("measured", tr(locale, Msg::ServerMeasured)).numeric(),
    ];
    let [_, target, measured] = &columns;
    let micro = |value: u32| {
        t(tr_with(
            locale,
            Msg::ServerMicroseconds,
            &[("value", value.into())],
        ))
    };
    let rows = join(latency.bars.iter().map(|bar| {
        h(format!(
            "<tr>{}{}{}</tr>",
            title_cell(None, code(bar.bench), None),
            cell(target, micro(bar.max_us)),
            cell(measured, micro(bar.measured_us)),
        ))
    }));
    let caption = tr(locale, Msg::ServerLatencyTitle);
    let table = data_table(DataTable {
        caption: &caption,
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(latency.bars.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        Html::join([
            section_intro(tr_with(
                locale,
                Msg::ServerLatencyIntro,
                &[("machine", latency.machine.into())],
            )),
            table,
        ]),
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
