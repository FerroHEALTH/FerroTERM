//! The shell chrome every screen renders inside.

// The diagnostic lands on macro output rather than on an item written here,
// so the expectation covers the module tree.
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]

/// The wording every screen uses for a fact the server did not declare.
///
/// Absence is stated rather than left blank: a reader cannot tell a fact the
/// server omitted from a cell the page failed to fill.
pub(crate) const NOT_DECLARED: &str = "not declared";

/// Whether FerroUI reads `value` as a date-time, which it then draws as a
/// timestamp rather than as the server's text.
pub(crate) fn is_date_time(value: &str) -> bool {
    // NOTE: `reading` returns a value that is no date-time verbatim, so a changed
    // reading is the one sign that FerroUI parsed it.
    ferrohealth_ui::view::time::reading(value) != value
}

/// Returns a value a resource or a record states: a date-time as FerroUI's
/// [`timestamp`](ferrohealth_ui::view::time::timestamp) draws it, on one line,
/// anything else as the text, which wraps.
pub(crate) fn stated(value: String) -> leptos::prelude::AnyView {
    use leptos::prelude::IntoAny as _;
    if is_date_time(&value) {
        ferrohealth_ui::view::time::timestamp(&value).into_any()
    } else {
        value.into_any()
    }
}

pub(crate) mod coded;
pub(crate) mod detail;
pub(crate) mod failure;
pub(crate) mod field;
pub(crate) mod health;
pub(crate) mod history;
pub(crate) mod icon;
pub(crate) mod reading;
pub(crate) mod reload;
pub(crate) mod runs;
pub(crate) mod shell;
pub(crate) mod state;
pub(crate) mod system_table;

/// Moves the browser's focus to the element `id` names, once the page runs
/// in a browser; the server half has no focus to move.
pub(crate) fn focus(id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use leptos::wasm_bindgen::JsCast as _;
        let found = leptos::prelude::document()
            .get_element_by_id(id)
            .and_then(|element| element.dyn_into::<leptos::web_sys::HtmlElement>().ok());
        if let Some(element) = found
            && element.focus().is_err()
        {
            leptos::logging::warn!("this browser refused to move the focus");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: &str = id;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use leptos::prelude::{Owner, RenderHtml as _};

    use super::{is_date_time, stated};

    fn render(value: &str) -> String {
        Owner::new().with(|| stated(value.to_owned()).to_html())
    }

    #[test]
    fn an_instant_is_drawn_as_a_timestamp_in_utc() {
        let html = render("2026-10-18T18:31:52.427842+02:00");
        assert!(html.contains("<time"), "{html}");
        assert!(html.contains("2026-10-18 16:31:52 UTC"), "{html}");
        assert!(
            html.contains("title=\"2026-10-18T18:31:52.427842+02:00\""),
            "{html}"
        );
    }

    #[test]
    fn a_date_and_free_text_stay_text_that_wraps() {
        for value in ["2031-01-01", "PUT", "http://example.org/fhir/CodeSystem/a"] {
            assert!(!is_date_time(value), "{value}");
            let html = render(value);
            assert!(!html.contains("fui-time"), "{value}: {html}");
            assert!(html.contains(value), "{html}");
        }
    }
}
