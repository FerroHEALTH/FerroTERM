// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Expand: the codes a value set holds, a page at a time, with
//! `ValueSet/$expand` (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
//!
//! `count` and `offset` page the expansion on the server; the pager's links
//! carry the next offset, so every page is a link of its own.

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::{Noun, Paging, Window};
use ferrohealth_ui::table::pager::{Listing, link_pager};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::form::{Field, INPUT, NARROW, check};
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::surface::{SectionHead, definitions, panel_section};
use ferrohealth_ui::view::transform::field_checks;
use leptos::prelude::*;

use crate::fhir::expansion::{ExpandRequest, Expansion};
use crate::fhir::{CODE_SYSTEM, VALUE_SET};
use crate::locale::{Msg, message, tr, tr_with};
use crate::pages::operations::{
    Op, before, choices, frame, options, run, select, sent, text, verdict,
};
use crate::pages::parts::{failure, loaded, name_of};
use crate::prefs::MAX_COUNT;
use crate::request::{Request, request};
use crate::routes::ui_link;

/// The address parameter carrying the value set's canonical.
const URL_PARAM: &str = "url";

/// The address parameter carrying the text filter.
const FILTER_PARAM: &str = "filter";

/// The address parameter carrying the page size.
const COUNT_PARAM: &str = "count";

/// The address parameter carrying the first concept of the page.
const OFFSET_PARAM: &str = "offset";

/// The address parameter asking for active concepts alone.
const ACTIVE_PARAM: &str = "activeOnly";

/// The address parameter asking for every designation.
const DESIGNATIONS_PARAM: &str = "includeDesignations";

/// What one run asks for, read from the address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Asked {
    url: String,
    filter: String,
    count: u32,
    offset: u32,
    active: bool,
    designations: bool,
}

impl Asked {
    /// The run the address names, the page size the reader's preference
    /// when it names none.
    fn read(request: &Request) -> Self {
        let number = |name: &str| request.text(name).parse::<u32>().ok();
        Self {
            url: request.text(URL_PARAM),
            filter: request.text(FILTER_PARAM),
            count: number(COUNT_PARAM)
                .unwrap_or(request.prefs.page_size)
                .clamp(1, MAX_COUNT),
            offset: number(OFFSET_PARAM).unwrap_or(0),
            active: request.text(ACTIVE_PARAM) == "true",
            designations: request.text(DESIGNATIONS_PARAM) == "true",
        }
    }

    /// The request the run sends.
    fn request(&self, display_language: &str) -> ExpandRequest {
        ExpandRequest {
            url: self.url.clone(),
            filter: Some(self.filter.clone()).filter(|text| !text.is_empty()),
            count: Some(self.count),
            offset: Some(self.offset),
            display_language: Some(display_language.to_owned()).filter(|tag| !tag.is_empty()),
            active_only: self.active.then_some(true),
            include_designations: self.designations.then_some(true),
        }
    }

    /// The address of this run at `offset`.
    fn at(&self, request: &Request, offset: u32) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair(URL_PARAM, &self.url)
            .append_pair(COUNT_PARAM, &self.count.to_string())
            .append_pair(OFFSET_PARAM, &offset.to_string());
        if !self.filter.is_empty() {
            query.append_pair(FILTER_PARAM, &self.filter);
        }
        if self.active {
            query.append_pair(ACTIVE_PARAM, "true");
        }
        if self.designations {
            query.append_pair(DESIGNATIONS_PARAM, "true");
        }
        format!(
            "{}&{}",
            ui_link(Op::Expand.path(), request.version),
            query.finish()
        )
    }
}

/// The Expand tab.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn ExpandPage() -> impl IntoView {
    let request = request();
    loaded(async move {
        let sets_named = request
            .client
            .published_names(request.version, VALUE_SET)
            .await
            .ok();
        let sets = choices(sets_named.as_ref());
        let set_names = sets_named
            .as_ref()
            .map(crate::fhir::named::NamedSearch::names)
            .unwrap_or_default();
        let asked = Asked::read(&request);
        let picked = if asked.url.is_empty() {
            sets.first()
                .map(|(value, _)| value.clone())
                .unwrap_or_default()
        } else {
            asked.url.clone()
        };
        let count = asked.count.to_string();
        let fields = view! {
            {select("expand-url", URL_PARAM, tr(Msg::CheckValueSet), &sets, &picked, None)}
            {text(
                "expand-filter",
                FILTER_PARAM,
                tr(Msg::ExpandFilter),
                &asked.filter,
                tr(Msg::ExpandFilterHint),
            )}
            {Field::new("expand-count", tr(Msg::ExpandPageSize))
                .view(
                    view! {
                        <input
                            id="expand-count"
                            name=COUNT_PARAM
                            type="number"
                            min="1"
                            max=MAX_COUNT.to_string()
                            class=format!("{INPUT} {NARROW}")
                            value=count
                        />
                    },
                )}
            {field_checks(
                view! {
                    {check(
                        "expand-active",
                        tr(Msg::ExpandActiveOnly),
                        view! {
                            <input
                                type="checkbox"
                                id="expand-active"
                                name=ACTIVE_PARAM
                                value="true"
                                checked=asked.active
                            />
                        },
                    )}
                    {check(
                        "expand-designations",
                        tr(Msg::ExpandDesignations),
                        view! {
                            <input
                                type="checkbox"
                                id="expand-designations"
                                name=DESIGNATIONS_PARAM
                                value="true"
                                checked=asked.designations
                            />
                        },
                    )}
                },
            )}
        }
        .into_any();
        let form = options(
            &request,
            &[],
            fields,
            run(
                "expand-run",
                tr(Msg::OpsExpand),
                Glyph::LIST_TREE,
                "$expand",
            ),
        );
        let answer = if asked.url.is_empty() {
            before(tr(Msg::ExpandBefore))
        } else {
            answer(&request, &asked, &name_of(&set_names, &asked.url)).await
        };
        frame(&request, Op::Expand, form, answer)
    })
}

/// The answer to `asked`, the value set named `set`.
async fn answer(request: &Request, asked: &Asked, set: &str) -> AnyView {
    let sent_request = asked.request(&request.prefs.display_language);
    let url = request.client.expand_url(request.version, &sent_request);
    match request.client.expand(request.version, &sent_request).await {
        Err(error) => view! {
            {panel_section(None, failure(&error))}
            {sent(&url, None)}
        }
        .into_any(),
        Ok(expanded) => match expanded.expansion() {
            None => view! {
                {verdict(Outcome::Warn, tr(Msg::ValueSetNoExpansion).into_any(), None)}
                {sent(&url, None)}
            }
            .into_any(),
            Some(expansion) => {
                let names = request
                    .client
                    .published_names(request.version, CODE_SYSTEM)
                    .await
                    .map(|named| named.names())
                    .unwrap_or_default();
                view! {
                    {verdict(Outcome::Ok, title(&expansion, asked, set), None)}
                    {concepts(request, &expansion, asked, &names)}
                    {applied(&expansion)}
                    {sent(&url, None)}
                }
                .into_any()
            }
        },
    }
}

/// The verdict's sentence: which concepts of how many the page shows.
fn title(expansion: &Expansion, asked: &Asked, set: &str) -> AnyView {
    let shown = expansion.listed();
    let first = asked.offset.saturating_add(1);
    let last = asked.offset.saturating_add(shown);
    let words = match expansion.total {
        _ if shown == 0 => tr_with(Msg::ExpandNone, &[("set", set.to_owned().into())]),
        Some(total) => tr_with(
            Msg::ExpandShownOf,
            &[
                ("first", first.into()),
                ("last", last.into()),
                ("total", total.into()),
                ("set", set.to_owned().into()),
            ],
        ),
        None => tr_with(
            Msg::ExpandShown,
            &[
                ("first", first.into()),
                ("last", last.into()),
                ("set", set.to_owned().into()),
            ],
        ),
    };
    words.into_any()
}

/// The page of concepts, with the pager when the expansion has more.
fn concepts(
    request: &Request,
    expansion: &Expansion,
    asked: &Asked,
    names: &BTreeMap<String, String>,
) -> AnyView {
    let columns = [
        Column::text("concept", tr(Msg::ValueSetConcept)).role(Role::Title),
        Column::text("system", tr(Msg::ValueSetSystem)),
        Column::text("version", tr(Msg::LookupVersion)).role(Role::Quiet),
    ];
    let [_, system, version] = &columns;
    let rows = expansion
        .concepts
        .iter()
        .map(|concept| {
            let mut drawn = Coding::new(
                concept.code.clone(),
                concept.display.clone().unwrap_or_default(),
            );
            if concept.inactive {
                drawn = drawn.inactive();
            }
            let system_name = names.get(&concept.system).cloned().map_or_else(
                || ferrohealth_ui::view::coding::canonical(&concept.system),
                IntoAny::into_any,
            );
            view! {
                <tr>
                    {title_cell(None, coding(drawn, Layout::Inline), None)}
                    {cell(system, system_name)}
                    {cell(version, concept.version.clone().unwrap_or_default())}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::ExpandConcepts);
    let pager = expansion
        .total
        .filter(|total| *total > asked.count)
        .map(|total| {
            let page = asked.offset.div_euclid(asked.count).saturating_add(1);
            let window = Window::over(
                total,
                Paging {
                    page,
                    size: asked.count,
                },
            );
            link_pager(
                Listing {
                    label: tr(Msg::ExpandPages).into(),
                    noun: Noun::new(message(Msg::ExpandNoun)),
                    sizes: &[],
                    stated: None,
                },
                window,
                &|paging| {
                    let offset = paging.page.saturating_sub(1).saturating_mul(paging.size);
                    asked.at(request, offset)
                },
            )
        });
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: None,
            actions: None,
        }),
        view! {
            {data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            })}
            {pager}
        },
    )
}

/// The parameters the server says it applied, each with its name.
fn applied(expansion: &Expansion) -> Option<AnyView> {
    if expansion.parameters.is_empty() {
        return None;
    }
    let rows = expansion
        .parameters
        .iter()
        .map(|line| (line.name.clone(), view! { {line.value.clone()} }.into_any()))
        .collect::<Vec<_>>();
    Some(panel_section(
        Some(SectionHead {
            title: tr(Msg::ExpandApplied),
            count: None,
            actions: None,
        }),
        definitions(rows),
    ))
}

#[cfg(test)]
mod tests {
    use super::Asked;

    #[test]
    fn a_page_link_keeps_every_choice_of_the_run() {
        let asked = Asked {
            url: String::from("http://x/vs"),
            filter: String::from("a b"),
            count: 10,
            offset: 0,
            active: true,
            designations: false,
        };
        let request = crate::request::Request::read();
        let href = asked.at(&request, 20);
        assert!(href.contains("url=http%3A%2F%2Fx%2Fvs"), "{href}");
        assert!(href.contains("offset=20"), "{href}");
        assert!(href.contains("filter=a+b"), "{href}");
        assert!(href.contains("activeOnly=true"), "{href}");
        assert!(!href.contains("includeDesignations"), "{href}");
    }
}
