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
