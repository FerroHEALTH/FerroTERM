// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! About: the console, the server it reads and the manufacturer, with the
//! `CapabilityStatement` of the FHIR version in view.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::view::surface::Header;
use ferrohealth_ui::view::template::about::{Link, Manufacturer, SelfDescription, about_page};
use leptos::prelude::*;
use leptos_meta::Title;

use crate::fhir::capability::CapabilityStatement;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::parts::loaded;
use crate::request::request;
use crate::shell::{PRODUCT, VERSION};

/// The library the console is drawn with, at the version its manifest pins.
pub(crate) const FERROUI: &str = "ferrohealth-ui 0.4.0";

/// The id of the self-description's Copy button.
const COPY_ID: &str = "about-copy";

/// The FHIR roots the server answers on, each with its release.
fn roots() -> String {
    FhirVersion::ALL
        .into_iter()
        .map(|version| {
            let release = version.release().trim_start_matches("FHIR ");
            format!("{} ({release})", version.label())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// About.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn AboutPage() -> impl IntoView {
    let request = request();
    loaded(async move {
        let version = request.version;
        let statement = request.client.capability_statement(version).await.ok();
        let document = request.client.capability_document(version).await.ok();
        let server = statement
            .as_ref()
            .and_then(CapabilityStatement::software_line)
            .unwrap_or_else(|| tr(Msg::AboutServerUnanswered));
        let self_description = document.map(|document| SelfDescription {
            title: tr_with(
                Msg::AboutStatementTitle,
                &[("version", version.label().into())],
            ),
            line: tr_with(
                Msg::AboutStatementLine,
                &[
                    ("version", version.label().into()),
                    ("path", format!("/{}/metadata", version.segment()).into()),
                ],
            ),
            language: Language::Json,
            text: serde_json::to_string_pretty(&document).unwrap_or_default(),
            copy_id: String::from(COPY_ID),
            download: request.client.metadata_url(version),
        });
        let page = about_page(ferrohealth_ui::view::template::about::AboutPage {
            header: Header {
                title: tr(Msg::AboutTitle).into(),
                subtitle: Some(tr(Msg::AboutSubtitle).into()),
                subtitle_view: None,
                crumbs: Vec::new(),
                mono: false,
                badge: None,
            },
            release: format!("{PRODUCT} {VERSION}"),
            versions: vec![
                (tr(Msg::AboutServer), server.into_any()),
                (tr(Msg::AboutFhirRoots), roots().into_any()),
                (tr(Msg::AboutFerroui), FERROUI.into_any()),
            ],
            manufacturer: Manufacturer {
                licence: tr(Msg::AboutLicence),
                support: Link {
                    label: String::from(SUPPORT),
                    href: format!("mailto:{SUPPORT}"),
                },
                documentation: Some(Link {
                    label: tr(Msg::AboutBook),
                    href: String::from(BOOK),
                }),
            },
            cards: Vec::new(),
            self_description,
            replica: None,
        });
        view! {
            <Title text=tr(Msg::AboutTitle) />
            {page}
        }
        .into_any()
    })
}

/// The maker's support address.
const SUPPORT: &str = "info@cadasto.com";

/// The product's documentation.
const BOOK: &str = "https://ferroterm.eu/docs/";

#[cfg(test)]
mod tests {
    use super::FERROUI;

    #[test]
    fn about_names_the_library_version_the_manifest_pins() {
        let manifest =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
                .expect("the workspace manifest reads");
        let pin = manifest
            .lines()
            .find_map(|line| line.strip_prefix("ferrohealth-ui = "))
            .and_then(|rest| rest.split_once("\"=").map(|(_, pinned)| pinned))
            .and_then(|pinned| pinned.split_once('"').map(|(version, _)| version))
            .expect("the manifest pins ferrohealth-ui exactly");
        assert_eq!(format!("ferrohealth-ui {pin}"), FERROUI);
    }
}
