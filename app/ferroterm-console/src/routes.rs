// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The in-application addresses the shell links to.
//!
//! Every link keeps the selected FHIR version in the query, so a link a reader
//! copies reproduces exactly what they were looking at.

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::url::RequestUrl;

/// Where the console's router is based: the console is its own server, so
/// its pages sit at the root of its origin.
pub(crate) const UI_BASE: &str = "";

/// The query parameter that carries the selected FHIR version.
pub(crate) const VERSION_PARAM: &str = "fhir";

/// The path a code system's own screen sits under.
pub(crate) const SYSTEMS_PATH: &str = "systems";

/// The concept browser's path below the base.
pub(crate) const BROWSE_PATH: &str = "browse";

/// The expansion runner's path below the base.
pub(crate) const EXPAND_PATH: &str = "expand";

/// The `$validate-code` and `$subsumes` runners' path below the base.
pub(crate) const VALIDATE_PATH: &str = "validate";

/// The value set screen's path below the base.
pub(crate) const VALUE_SETS_PATH: &str = "valuesets";

/// The value set composer's path below the base, in the editor bundle.
pub(crate) const COMPOSE_PATH: &str = "compose";

/// The concept map screen's path below the base.
pub(crate) const CONCEPT_MAPS_PATH: &str = "conceptmaps";

/// The path segment of the translate runner.
pub(crate) const TRANSLATE_PATH: &str = "translate";

/// The path segment of the lookup runner.
pub(crate) const LOOKUP_PATH: &str = "lookup";

/// The path the command bar sends what a reader typed to.
pub(crate) const FIND_PATH: &str = "find";

/// The code system editor's path below the base, in the editor bundle.
pub(crate) const CODE_SYSTEM_EDITOR_PATH: &str = "codesystem";

/// The concept map editor's path below the base, in the editor bundle.
pub(crate) const CONCEPT_MAP_EDITOR_PATH: &str = "conceptmap";

/// The history and restore screen's path below the base, in the editor bundle.
pub(crate) const HISTORY_PATH: &str = "history";

/// The route the history screen is served and posted at.
pub(crate) const HISTORY_ROUTE: &str = "/history";

/// The route the code system editor is served and posted at.
pub(crate) const CODE_SYSTEM_EDITOR_ROUTE: &str = "/codesystem";

/// The route the value set composer is served and posted at.
pub(crate) const COMPOSE_ROUTE: &str = "/compose";

/// The route the concept map editor is served and posted at.
pub(crate) const CONCEPT_MAP_EDITOR_ROUTE: &str = "/conceptmap";

/// The query parameter that carries a resource type into the history screen.
pub(crate) const TYPE_PARAM: &str = "type";

/// The query parameter that carries a logical id into the history screen.
pub(crate) const ID_PARAM: &str = "id";

/// The query parameter the code system editor takes its canonical in.
///
/// It is the field the screen's own query type names, so the two move
/// together or the link it builds stops opening what it says it opens.
pub(crate) const EDITOR_SYSTEM_PARAM: &str = "system";

/// The query parameter the value set composer takes its logical id in.
pub(crate) const COMPOSER_ID_PARAM: &str = "id";

/// The query parameter the concept map editor takes its canonical in.
pub(crate) const MAP_EDITOR_PARAM: &str = "map";

/// The link back to the screen that edits one resource.
///
/// Each authoring screen opens the resource it edits by the fact it addresses
/// it with: a canonical for a code system and a concept map, the logical id
/// for a value set. A type no screen here edits answers `None`.
pub(crate) fn editing_link(
    resource_type: &str,
    canonical: &str,
    id: &str,
    version: FhirVersion,
) -> Option<String> {
    let (path, name, value) = match resource_type {
        "CodeSystem" => (CODE_SYSTEM_EDITOR_PATH, EDITOR_SYSTEM_PARAM, canonical),
        "ValueSet" => (COMPOSE_PATH, COMPOSER_ID_PARAM, id),
        "ConceptMap" => (CONCEPT_MAP_EDITOR_PATH, MAP_EDITOR_PARAM, canonical),
        _unedited => return None,
    };
    let mut url = base_url()
        .segment(path)
        .query(VERSION_PARAM, version.segment());
    if !value.is_empty() {
        url = url.query(name, value);
    }
    Some(url.render(""))
}

/// The link to one resource's own screen, and the words that name it.
///
/// A code system's screen is addressed by its canonical, a value set's and a
/// concept map's by the logical id. A type with no screen here, or a resource
/// missing the fact its screen needs, answers `None`.
pub(crate) fn screen_link(
    resource_type: &str,
    canonical: &str,
    id: &str,
    version: FhirVersion,
) -> Option<(String, String)> {
    match resource_type {
        "CodeSystem" if !canonical.is_empty() => {
            Some((system_link(canonical, version), tr(Msg::BackCodeSystem)))
        }
        "ValueSet" if !id.is_empty() => Some((
            resource_link(VALUE_SETS_PATH, id, version),
            tr(Msg::BackValueSet),
        )),
        "ConceptMap" if !id.is_empty() => Some((
            resource_link(CONCEPT_MAPS_PATH, id, version),
            tr(Msg::BackConceptMap),
        )),
        _ => None,
    }
}

/// The link that opens one resource's versions, from a screen editing it.
///
/// The type and the id are the two facts the history interaction addresses an
/// instance by (<https://hl7.org/fhir/R4B/http.html#history>), so they are
/// what the address carries.
pub(crate) fn history_link(resource_type: &str, id: &str, version: FhirVersion) -> String {
    base_url()
        .segment(HISTORY_PATH)
        .query(VERSION_PARAM, version.segment())
        .query(TYPE_PARAM, resource_type)
        .query(ID_PARAM, id)
        .render("")
}

/// The About screen's path below the base.
pub(crate) const ABOUT_PATH: &str = "about";

/// The About screen's address, which the footer links to.
pub(crate) const ABOUT_ROUTE: &str = "/about";

/// The Operations workbench's first tab, which its navigation entry opens.
pub(crate) const OPERATIONS_PATH: &str = "lookup";

/// The Server screen's path below the base.
pub(crate) const SERVER_PATH: &str = "server";

/// The Server screen's address, which it posts a reload to.
pub(crate) const SERVER_ROUTE: &str = "/server";

/// The Synchronisation screens' path below the base.
pub(crate) const SYNC_PATH: &str = "sync";

/// The Synchronisation screen's address, which it posts Run now and
/// Activate to.
pub(crate) const SYNC_ROUTE: &str = "/sync";

/// The Configuration screen's path below the base.
pub(crate) const CONFIGURATION_PATH: &str = "configuration";

/// Your preferences' path below the base.
pub(crate) const PREFERENCES_PATH: &str = "preferences";

/// The keyboard shortcut legend's path below the base.
pub(crate) const SHORTCUTS_PATH: &str = "shortcuts";

/// The route every preference posts to, its value under its cookie key.
pub(crate) const PREFERENCE_ROUTE: &str = "/preference";

/// The route Reset to defaults posts to.
pub(crate) const RESET_ROUTE: &str = "/preferences/reset";

/// The route the navigation's Collapse and Expand post to.
pub(crate) const SIDEBAR_ROUTE: &str = "/sidebar";

/// The route the top bar's FHIR version select leads to, which keeps the
/// version for the browser's session and returns to the page.
pub(crate) const FHIR_VERSION_ROUTE: &str = "/fhir-version";

/// The sign-in page.
pub(crate) const SIGN_IN_PAGE: &str = "/auth/sign-in";

/// The route that starts a sign-in at the OpenID Provider.
pub(crate) const LOGIN_ROUTE: &str = "/auth/login";

/// The route Sign out posts to.
pub(crate) const SIGN_OUT_ROUTE: &str = "/auth/sign-out";

/// The query parameter that carries the page to return to.
pub(crate) const BACK_PARAM: &str = "back";

/// The absolute path of the console's page `path`, with `""` naming the
/// dashboard.
pub(crate) fn ui_path(path: &str) -> String {
    let mut url = base_url();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        url = url.segment(part);
    }
    rooted(url.render(""))
}

/// The sign-in page, returning to `back` once the reader is signed in.
pub(crate) fn sign_in_link(back: &str) -> String {
    rooted(
        base_url()
            .segment("auth")
            .segment("sign-in")
            .query(BACK_PARAM, back)
            .render(""),
    )
}

/// The choice of `version` in the top bar's select, on the page at `path`
/// with the query `search`: the route that keeps it for the session and
/// returns to the same page read through it.
pub(crate) fn version_choice(path: &str, search: &str, version: FhirVersion) -> String {
    rooted(
        base_url()
            .segment("fhir-version")
            .query(VERSION_PARAM, version.segment())
            .query(BACK_PARAM, &version_link(path, search, version))
            .render(""),
    )
}

/// The query parameter that carries a code system canonical into a screen.
pub(crate) const SYSTEM_PARAM: &str = "system";

/// The query parameter that carries a code system version into a screen.
pub(crate) const SYSTEM_VERSION_PARAM: &str = "version";

/// The router base as a URL of its own, one percent-encoded segment at a time.
///
/// The base is more than one segment in the editor bundle, so it is never
/// written as a single segment: that would escape the separator inside it.
pub(crate) fn base_url() -> RequestUrl {
    let mut url = RequestUrl::new();
    for segment in UI_BASE.split('/').filter(|segment| !segment.is_empty()) {
        url = url.segment(segment);
    }
    url
}

/// Builds a link to one of the viewer's own pages.
///
/// `path` is the page's path below the base, with `""` naming the index.
pub(crate) fn ui_link(path: &str, version: FhirVersion) -> String {
    let mut url = base_url();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        url = url.segment(part);
    }
    rooted(url.query(VERSION_PARAM, version.segment()).render(""))
}

/// `link` as an absolute path: the root page renders as its query alone,
/// which a browser resolves against the current page rather than the root.
fn rooted(link: String) -> String {
    if link.starts_with('/') {
        link
    } else {
        format!("/{link}")
    }
}

/// Builds the link to one code system's own screen.
///
/// The canonical is one percent-encoded path segment, so a URI that carries
/// its own query string, or any other structural character, survives the
/// route: `leptos_router` splits the raw path on a literal `/` and only then
/// decodes the segment it captured.
// NOTE: a click navigation runs the path through `decodeURI`, which keeps the
// reserved set escaped (<https://tc39.es/ecma262/#sec-decodeuri-encodeduri>),
// so a canonical carrying a literal `%` is the one shape that does not survive.
pub(crate) fn system_link(system: &str, version: FhirVersion) -> String {
    base_url()
        .segment(SYSTEMS_PATH)
        .segment(system)
        .query(VERSION_PARAM, version.segment())
        .render("")
}

/// Builds the link to one stored resource's own screen under `path`, the
/// logical id one percent-encoded path segment
/// (<https://hl7.org/fhir/R4B/http.html#read>).
pub(crate) fn resource_link(path: &str, id: &str, version: FhirVersion) -> String {
    base_url()
        .segment(path)
        .segment(id)
        .query(VERSION_PARAM, version.segment())
        .render("")
}

/// Builds the link into a screen that works over one code system.
///
/// The system, and the code system version when one is declared, travel as
/// query parameters, so the address the reader lands on says exactly what it
/// is showing.
pub(crate) fn system_tool_link(
    path: &str,
    system: &str,
    system_version: Option<&str>,
    version: FhirVersion,
) -> String {
    let mut url = base_url()
        .segment(path)
        .query(VERSION_PARAM, version.segment())
        .query(SYSTEM_PARAM, system);
    if let Some(code) = system_version {
        url = url.query(SYSTEM_VERSION_PARAM, code);
    }
    url.render("")
}

/// Builds the link that opens one value set canonical in the expansion runner.
///
/// The runner takes the canonical as its `url` parameter, so the link names
/// that and nothing else: every other parameter is the runner's own default
/// or the reader's stored page size.
pub(crate) fn expansion_link(canonical: &str, version: FhirVersion) -> String {
    base_url()
        .segment(EXPAND_PATH)
        .query(VERSION_PARAM, version.segment())
        .query("url", canonical)
        .render("")
}

/// Rewrites the address a reader is on to select another FHIR version.
///
/// The switcher stays on the page the reader is reading, so switching version
/// is a navigation within the same route rather than a jump to the index.
/// Every other parameter the address carries is kept, because a screen that
/// works over one code system carries it in the query and switching version
/// must not drop what the reader is looking at. `search` is the query without
/// its leading `?`, already percent-encoded, so each pair is carried verbatim.
pub(crate) fn version_link(pathname: &str, search: &str, version: FhirVersion) -> String {
    let mut kept: Vec<&str> = search
        .split('&')
        .filter(|pair| {
            !pair.is_empty()
                && pair.split_once('=').map_or(*pair, |(name, _)| name) != VERSION_PARAM
        })
        .collect();
    let selected = format!("{VERSION_PARAM}={}", version.segment());
    kept.push(&selected);
    format!("{pathname}?{}", kept.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An address under the bundle's own base.
    ///
    /// Every expectation is written against the base rather than against a
    /// literal, because the crate builds twice and the two bundles are based
    /// at different paths.
    fn under(path: &str) -> String {
        format!("{UI_BASE}{path}")
    }

    #[test]
    fn the_index_link_carries_the_version() {
        assert_eq!(ui_link("", FhirVersion::R4B), under("/?fhir=r4b"));
    }

    #[test]
    fn a_page_link_carries_the_version() {
        assert_eq!(
            ui_link("settings", FhirVersion::R5),
            under("/settings?fhir=r5")
        );
    }

    #[test]
    fn a_nested_page_link_keeps_its_segments() {
        assert_eq!(
            ui_link("/systems/detail", FhirVersion::R4),
            under("/systems/detail?fhir=r4"),
            "leading and repeated separators do not produce empty segments"
        );
    }

    #[test]
    fn the_base_is_built_one_segment_at_a_time() {
        assert_eq!(
            base_url().render(""),
            UI_BASE,
            "the base renders as it is written, with no separator escaped"
        );
    }

    /// The segment `system_link` wrote, as `leptos_router` reads it back.
    ///
    /// The router splits the raw path on a literal `/` and decodes the
    /// captured segment afterwards, so this models both halves: take the
    /// segment, then percent-decode it.
    fn round_trip(link: &str) -> String {
        let below = link
            .strip_prefix(&under("/systems/"))
            .expect("the link addresses a code system screen");
        let segment = below.split('?').next().unwrap_or_default();
        percent_encoding::percent_decode_str(segment)
            .decode_utf8()
            .expect("the encoder emits UTF-8")
            .into_owned()
    }

    #[test]
    fn a_code_system_canonical_round_trips_through_the_route() {
        let system = "https://terminology.example/animals";
        let link = system_link(system, FhirVersion::R4B);
        assert_eq!(
            link,
            under("/systems/https:%2F%2Fterminology.example%2Fanimals?fhir=r4b")
        );
        assert_eq!(round_trip(&link), system);
    }

    #[test]
    fn a_canonical_that_carries_a_query_string_round_trips_too() {
        let system = "https://terminology.example/x?fhir_vs=isa/12345&lang=nl";
        let link = system_link(system, FhirVersion::R6);
        assert_eq!(
            link.matches('?').count(),
            1,
            "the only `?` in the link is the one that opens the version query: {link}"
        );
        assert_eq!(round_trip(&link), system);
    }

    #[test]
    fn a_tool_link_carries_the_system_and_the_version() {
        assert_eq!(
            system_tool_link(
                EXPAND_PATH,
                "https://terminology.example/x",
                Some("2031-01-01"),
                FhirVersion::R5
            ),
            under(
                "/expand?fhir=r5&system=https%3A%2F%2Fterminology.example%2Fx&version=2031-01-01"
            )
        );
    }

    #[test]
    fn a_tool_link_for_a_system_with_no_declared_version_omits_it() {
        assert_eq!(
            system_tool_link(
                BROWSE_PATH,
                "https://terminology.example/x",
                None,
                FhirVersion::R4
            ),
            under("/browse?fhir=r4&system=https%3A%2F%2Fterminology.example%2Fx"),
            "an absent version is left out rather than sent as an empty one"
        );
    }

    #[test]
    fn an_expansion_link_carries_the_canonical_the_runner_reads() {
        assert_eq!(
            expansion_link(
                "http://terminology.example/x?fhir_vs=isa/1",
                FhirVersion::R4B
            ),
            under("/expand?fhir=r4b&url=http%3A%2F%2Fterminology.example%2Fx%3Ffhir_vs%3Disa%2F1"),
            "an implicit canonical carrying its own query string stays in one parameter"
        );
    }

    #[test]
    fn a_history_link_names_the_instance_the_versions_belong_to() {
        assert_eq!(
            history_link("CodeSystem", "colours/1", FhirVersion::R4B),
            under("/history?fhir=r4b&type=CodeSystem&id=colours%2F1"),
            "an id carrying a separator stays inside the parameter it belongs to"
        );
    }

    #[test]
    fn each_resource_type_links_back_to_the_screen_that_edits_it() {
        let system = "https://terminology.example/colours";
        assert_eq!(
            editing_link("CodeSystem", system, "colours", FhirVersion::R4B),
            Some(under(
                "/codesystem?fhir=r4b&system=https%3A%2F%2Fterminology.example%2Fcolours"
            ))
        );
        assert_eq!(
            editing_link("ValueSet", system, "palette", FhirVersion::R5),
            Some(under("/compose?fhir=r5&id=palette")),
            "the composer opens a value set by its logical id"
        );
        assert_eq!(
            editing_link("ConceptMap", system, "m", FhirVersion::R6),
            Some(under(
                "/conceptmap?fhir=r6&map=https%3A%2F%2Fterminology.example%2Fcolours"
            ))
        );
        assert_eq!(
            editing_link("Patient", system, "p", FhirVersion::R4),
            None,
            "no screen here edits a type the editor does not write"
        );
        assert_eq!(
            editing_link("CodeSystem", "", "", FhirVersion::R4),
            Some(under("/codesystem?fhir=r4")),
            "a resource with nothing to address it by opens the empty screen"
        );
    }

    #[test]
    fn switching_version_keeps_the_page_the_reader_is_on() {
        assert_eq!(
            version_link(&under("/settings"), "", FhirVersion::R6),
            under("/settings?fhir=r6")
        );
    }

    #[test]
    fn switching_version_keeps_every_other_parameter_the_address_carries() {
        assert_eq!(
            version_link(
                &under("/expand"),
                "fhir=r4b&system=https%3A%2F%2Fterminology.example%2Fx&version=2031-01-01",
                FhirVersion::R5
            ),
            under(
                "/expand?system=https%3A%2F%2Fterminology.example%2Fx&version=2031-01-01&fhir=r5"
            ),
            "a switcher that dropped the system would change what the screen shows"
        );
    }

    #[test]
    fn switching_version_does_not_leave_the_version_it_replaced_behind() {
        let rewritten = version_link(&under("/expand"), "fhir=r4&fhir=r5&q=a", FhirVersion::R6);
        assert_eq!(rewritten.matches("fhir=").count(), 1, "{rewritten}");
        assert_eq!(rewritten, under("/expand?q=a&fhir=r6"));
    }

    #[test]
    fn a_parameter_without_a_value_survives_the_rewrite() {
        assert_eq!(
            version_link(&under("/browse"), "flat&fhir=r4", FhirVersion::R4B),
            under("/browse?flat&fhir=r4b"),
            "the query is carried verbatim, so a bare parameter is not invented into a pair"
        );
    }

    #[test]
    fn a_resource_screen_is_addressed_by_what_it_is_read_by() {
        let system = "https://example.org/fhir/CodeSystem/colours";
        assert_eq!(
            screen_link("CodeSystem", system, "colours", FhirVersion::R4B).map(|(link, _)| link),
            Some(system_link(system, FhirVersion::R4B))
        );
        assert_eq!(
            screen_link("ValueSet", system, "palette", FhirVersion::R5).map(|(link, _)| link),
            Some("/valuesets/palette?fhir=r5".to_owned())
        );
        assert_eq!(
            screen_link("ConceptMap", system, "m", FhirVersion::R6).map(|(link, _)| link),
            Some("/conceptmaps/m?fhir=r6".to_owned())
        );
        assert_eq!(screen_link("ValueSet", system, "", FhirVersion::R4), None);
        assert_eq!(screen_link("Patient", system, "p", FhirVersion::R4), None);
    }

    /// The `back` a link carries, decoded as the route reads it.
    fn back_of(link: &str) -> String {
        let (_, query) = link.split_once('?').expect("a query");
        crate::request::param_in(query, BACK_PARAM).expect("a back parameter")
    }

    #[test]
    fn a_page_path_is_rooted_and_drops_empty_segments() {
        assert_eq!(ui_path(""), under("/"), "the dashboard is the root page");
        assert_eq!(ui_path("/systems//detail/"), under("/systems/detail"));
        assert_eq!(
            ui_path("preferences/reading"),
            under("/preferences/reading")
        );
    }

    #[test]
    fn the_sign_in_link_carries_the_page_to_return_to_encoded_whole() {
        let back = "/systems?q=a&fhir=r4";
        let link = sign_in_link(back);
        assert_eq!(
            link,
            under("/auth/sign-in?back=%2Fsystems%3Fq%3Da%26fhir%3Dr4"),
            "the page's own query cannot leak into the sign-in page's"
        );
        assert_eq!(back_of(&link), back);
        assert_eq!(
            crate::post::return_path(&back_of(&link)),
            Some(back),
            "the sign-in returns to a page the console accepts as its own"
        );
    }

    #[test]
    fn a_version_choice_keeps_the_page_and_reads_it_through_the_version() {
        let link = version_choice(&under("/systems"), "q=a&fhir=r4", FhirVersion::R5);
        assert!(
            link.starts_with(&under("/fhir-version?fhir=r5&back=")),
            "{link}"
        );
        assert_eq!(back_of(&link), under("/systems?q=a&fhir=r5"));
    }
}
