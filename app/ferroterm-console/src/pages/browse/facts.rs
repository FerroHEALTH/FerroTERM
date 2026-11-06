// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept's facts as the browser and the lookup page draw them: its
//! designations and its properties, one stacked table each.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Shape, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::fhir::concept::Concept;
use crate::locale::{Msg, tr};
use crate::markup::code;

/// The concept's designations.
pub(crate) fn designations(locale: &Locale, concept: &Concept) -> Option<Html> {
    if concept.designations.is_empty() {
        return None;
    }
    let columns = [
        Column::text("term", tr(locale, Msg::LookupTerm)).role(Role::Title),
        Column::text("language", tr(locale, Msg::LookupTermLanguage)),
        Column::text("use", tr(locale, Msg::LookupUse)),
    ];
    let [_, language, usage] = &columns;
    let rows = Html::join(concept.designations.iter().map(|row| {
        Html::raw(format!(
            "<tr>{}{}{}</tr>",
            title_cell(None, None, Html::text(&row.value), None),
            cell(
                language,
                Html::text(row.language.clone().unwrap_or_default())
            ),
            cell(usage, Html::text(row.usage.clone().unwrap_or_default())),
        ))
    }));
    let caption = tr(locale, Msg::LookupDesignations);
    Some(panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(concept.designations.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        data_table(DataTable {
            shape: Shape::Stacked,
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        }),
    ))
}

/// The concept's properties.
pub(crate) fn properties(locale: &Locale, concept: &Concept) -> Option<Html> {
    if concept.properties.is_empty() {
        return None;
    }
    let columns = [
        Column::text("property", tr(locale, Msg::LookupProperty)).role(Role::Title),
        Column::text("value", tr(locale, Msg::LookupValue)),
    ];
    let [_, value] = &columns;
    let rows = Html::join(concept.properties.iter().map(|row| {
        Html::raw(format!(
            "<tr>{}{}</tr>",
            title_cell(
                None,
                None,
                code(&row.code),
                row.description.as_ref().map(Html::text)
            ),
            cell(value, Html::text(row.value.clone().unwrap_or_default())),
        ))
    }));
    let caption = tr(locale, Msg::LookupProperties);
    Some(panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(concept.properties.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        data_table(DataTable {
            shape: Shape::Stacked,
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        }),
    ))
}
