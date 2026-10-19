//! The settings pane: what the console remembers about this reader.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::theme::{Density, Theme};
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::form::{FORM, INPUT, SELECT, field, help_id};
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_router::hooks::use_location;

use crate::components::shell::SelectedVersion;
use crate::fhir::FhirClient;
use crate::fhir::version::FhirVersion;
use crate::paging::MAX_COUNT;
use crate::settings::{SaveSettings, Settings};
use ferrohealth_ui::view::form::columns;
use ferrohealth_ui::view::surface::{card, definitions};

/// Shows and edits the reader's preferences.
///
/// Every value here lives in a cookie of the console's own, which the
/// console's server reads while it renders; nothing is kept in the browser's
/// storage and nothing reaches the terminology server. The form is a plain
/// post the console answers with a redirect back here, so it works before
/// the page hydrates.
pub(crate) fn pane() -> AnyView {
    let settings = expect_context::<Settings>();
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let base = move || client.version_base(version.get());
    let location = use_location();
    // NOTE: no specification governs this: our own design; the post returns
    // to the pane it was sent from, its query and all.
    let back = move || {
        let search = location.search.get();
        let search = search.trim_start_matches('?');
        let path = location.pathname.get();
        if search.is_empty() {
            path
        } else {
            format!("{path}?{search}")
        }
    };

    let in_use = definitions(vec![(
        "FHIR base in use",
        view! { <span class="fui-mono break-all">{base}</span> }.into_any(),
    )])
    .into_any();

    let fields = view! {
        {choice_field(
            "viewer-theme",
            "theme",
            "Theme",
            Some("System follows the colour scheme this device asks for."),
            &Theme::ALL.map(|theme| (theme.key(), theme.label())),
            settings.theme.key(),
        )}
        {choice_field(
            "viewer-density",
            "density",
            "Density",
            Some("Compact tightens every space and row, so more of a listing fits one screen."),
            &Density::ALL.map(|density| (density.key(), density.label())),
            settings.density.key(),
        )}
        {choice_field(
            "viewer-fhir-version",
            "fhir",
            "Default FHIR version",
            Some("Used when an address carries no version of its own."),
            &FhirVersion::ALL.map(|version| (version.segment(), version.label().to_owned())),
            settings.version.segment(),
        )}
        {field(
            "viewer-display-language",
            "Display language",
            Some(
                "A BCP 47 tag sent as displayLanguage. Leave it empty to take the server default.",
            ),
            view! {
                <input
                    id="viewer-display-language"
                    name="language"
                    type="text"
                    placeholder="for example nl-NL"
                    class=INPUT
                    aria-describedby=help_id("viewer-display-language")
                    value=settings.language.clone()
                />
            },
        )}
        {field(
            "viewer-page-size",
            "Page size",
            Some("How many rows a paged screen asks for, from 1 to 1000."),
            view! {
                <input
                    id="viewer-page-size"
                    name="page_size"
                    type="number"
                    inputmode="numeric"
                    min="1"
                    max=MAX_COUNT.to_string()
                    class=INPUT
                    aria-describedby=help_id("viewer-page-size")
                    value=settings.page_size.to_string()
                />
            },
        )}
    }
    .into_any();

    let body = view! {
        <p class="fui-muted">"Kept by this console for this browser, in its own cookies."</p>
        {in_use}
        <form method="post" action=SaveSettings::url() class=FORM>
            <input type="hidden" name="back" value=back />
            {columns(fields)}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {label(Some(Glyph::SAVE), "Save the settings")}
                </button>
            </div>
        </form>
    };
    card("Preferences", body).into_any()
}

/// One labelled select over `choices` (each a value and its words), with
/// `current` selected.
fn choice_field(
    id: &'static str,
    name: &'static str,
    label: &'static str,
    help: Option<&'static str>,
    choices: &[(&'static str, String)],
    current: &str,
) -> AnyView {
    let options = choices
        .iter()
        .map(|(value, words)| {
            let selected = *value == current;
            view! {
                <option value=*value selected=selected>
                    {words.clone()}
                </option>
            }
        })
        .collect_view();
    let described = help.map(|_| help_id(id));
    field(
        id,
        label,
        help,
        view! {
            <select id=id name=name class=SELECT aria-describedby=described>
                {options}
            </select>
        },
    )
    .into_any()
}
