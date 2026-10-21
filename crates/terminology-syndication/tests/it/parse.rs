// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the synthetic Atom documents into the typed model.

use terminology_syndication::model::{CategoryTerm, Checksum, LinkRel, SnomedRelease};
use terminology_syndication::parse::{self, ParseError};

use crate::fixtures;

fn full_feed() -> terminology_syndication::model::Feed {
    parse::feed(&fixtures::synthetic("full-feed.xml")).expect("the fixture is a well-formed feed")
}

fn entry(title: &str) -> terminology_syndication::model::Entry {
    full_feed()
        .entries
        .into_iter()
        .find(|entry| entry.title == title)
        .unwrap_or_else(|| panic!("the fixture has an entry titled {title}"))
}

#[test]
fn the_feed_metadata_is_read() {
    let feed = full_feed();
    assert_eq!(
        feed.title.as_deref(),
        Some("Example Terminology Syndication Feed")
    );
    assert_eq!(
        feed.id.as_deref(),
        Some("urn:uuid:00000000-0000-4000-8000-000000000000")
    );
    assert_eq!(
        feed.generator.as_deref(),
        Some("Example Terminology Server")
    );
    assert_eq!(
        feed.profile.as_deref(),
        Some("http://ns.electronichealth.net.au/ncts/syndication/asf/profile/1.0.0")
    );
    assert_eq!(
        feed.updated.map(|updated| updated.to_string()),
        Some(String::from("2026-01-31T00:00:00Z"))
    );
}

#[test]
fn every_named_category_term_is_read_from_the_feed() {
    let terms: Vec<CategoryTerm> = full_feed()
        .entries
        .iter()
        .map(terminology_syndication::model::Entry::term)
        .collect();
    for named in CategoryTerm::NAMED {
        assert!(
            terms.contains(&named),
            "the fixture offers {named} and the parser must read it"
        );
    }
}

#[test]
fn a_term_the_model_does_not_name_is_kept_verbatim() {
    assert_eq!(
        entry("Example Structure Definition").term(),
        CategoryTerm::Other(String::from("FHIR_StructureDefinition"))
    );
    assert_eq!(
        entry("Example Tabular Export").term(),
        CategoryTerm::Other(String::from("EXAMPLE_TSV"))
    );
}

#[test]
fn the_category_label_and_scheme_travel_with_the_term() {
    let category = entry("Example Binary Index 2.0.13")
        .category
        .expect("the entry declares a category");
    assert_eq!(category.term, CategoryTerm::BinaryIndex);
    assert_eq!(category.label.as_deref(), Some("Binary Index"));
    assert_eq!(
        category.scheme.as_deref(),
        Some("http://ontoserver.csiro.au/syndication/rf2/2.0.13")
    );
}

#[test]
fn a_sha256_checksum_is_read_and_folded_to_lowercase() {
    let link = entry("Example Edition 31 January 2026 (RF2 SNAPSHOT)")
        .content_link()
        .cloned()
        .expect("the entry offers an alternate link");
    assert_eq!(
        link.checksum,
        Some(Checksum::Sha256(String::from(
            "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900"
        )))
    );
    assert_eq!(link.length, Some(1024));
    assert_eq!(link.media_type.as_deref(), Some("application/zip"));
    assert_eq!(link.rel, LinkRel::Alternate);
}

#[test]
fn an_md5_checksum_is_read() {
    let entry = entry("Example Extension 15 December 2025 v1.0");
    let link = entry
        .content_link()
        .expect("the entry offers an alternate link");
    assert_eq!(
        link.checksum,
        Some(Checksum::Md5(String::from(
            "ffeeddccbbaa99887766554433221100"
        )))
    );
}

#[test]
fn a_related_link_is_kept_beside_the_content_link() {
    let entry = entry("Example Extension 15 December 2025 v1.0");
    assert_eq!(
        entry.links.len(),
        2,
        "the entry publishes a note and a package"
    );
    assert_eq!(entry.links[0].rel, LinkRel::Related);
    assert_eq!(
        entry.content_link().map(|link| link.href.as_str()),
        Some("https://example.invalid/content/extension-20251215-all.zip")
    );
}

#[test]
fn an_entry_with_no_alternate_link_offers_no_content_link() {
    let entry = entry("Example Entry Without A Content Link");
    assert_eq!(entry.links.len(), 1);
    assert_eq!(entry.content_link(), None);
}

#[test]
fn the_ncts_extension_elements_are_read() {
    let entry = entry("Example Terminology Bundle");
    assert_eq!(
        entry.content_item_identifier.as_deref(),
        Some("https://example.invalid/fhir/Bundle/example")
    );
    assert_eq!(entry.content_item_version.as_deref(), Some("1.0.0"));
    assert_eq!(entry.bundle_interpretation.as_deref(), Some("batch"));
}

#[test]
fn the_ontoserver_and_snomed_extension_elements_are_read() {
    let permission = entry("Example Laboratory Codes & Units").permission;
    assert_eq!(permission.as_deref(), Some("restricted.read"));
    let dependency = entry("Example Extension 15 December 2025 v1.0").edition_dependency;
    assert_eq!(
        dependency.as_deref(),
        Some("http://snomed.info/sct/11000001107/version/20260131")
    );
}

#[test]
fn a_feed_may_bind_the_extension_namespace_to_any_prefix() {
    let entry = entry("Example Laboratory Codes & Units");
    assert_eq!(
        entry.content_item_identifier.as_deref(),
        Some("https://example.invalid/fhir/CodeSystem/example"),
        "the entry binds the NCTS namespace to asf:, and the reader resolves namespaces"
    );
    assert_eq!(entry.fhir_version.as_deref(), Some("4.0.1"));
    let link = entry
        .content_link()
        .expect("the entry offers an alternate link");
    assert_eq!(
        link.checksum,
        Some(Checksum::Sha256(String::from(
            "3333333333333333333333333333333333333333333333333333333333333333"
        )))
    );
    assert!(link.validated, "onto:validated is read");
}

#[test]
fn a_predefined_entity_in_a_title_resolves() {
    let titles: Vec<String> = full_feed()
        .entries
        .into_iter()
        .map(|entry| entry.title)
        .collect();
    assert!(
        titles.contains(&String::from("Example Laboratory Codes & Units")),
        "&amp; resolves to & in the title: {titles:?}"
    );
}

#[test]
fn a_quoted_source_feed_does_not_overwrite_the_entry() {
    let entry = entry("Example Edition 31 January 2026 (RF2 SNAPSHOT)");
    assert_eq!(entry.id, "urn:uuid:11111111-1111-4111-8111-111111111111");
    assert_eq!(
        entry.links.len(),
        1,
        "the link inside atom:source belongs to the quoted feed, not the entry"
    );
}

#[test]
fn an_author_name_does_not_overwrite_the_entry_title() {
    let entry = entry("Example Edition 31 January 2026 (RF2 SNAPSHOT)");
    assert_eq!(
        entry.summary.as_deref(),
        Some("An invented edition used to shape the fixture.")
    );
}

#[test]
fn a_snomed_version_uri_yields_the_edition_and_version() {
    let entry = entry("Example Edition 31 January 2026 (RF2 SNAPSHOT)");
    assert_eq!(
        entry.snomed_release(),
        Some(SnomedRelease {
            edition: String::from("11000001107"),
            version: String::from("20260131"),
        })
    );
}

#[test]
fn a_version_that_is_not_a_snomed_uri_yields_no_release() {
    assert_eq!(entry("Example Value Set").snomed_release(), None);
}

#[test]
fn a_document_that_is_not_an_atom_feed_is_refused() {
    let error = parse::feed(&fixtures::synthetic("not-a-feed.xml"))
        .expect_err("an RSS document is not an Atom feed");
    match error {
        ParseError::NotAFeed { root } => assert_eq!(root, "rss"),
        other => panic!("expected NotAFeed, got {other:?}"),
    }
}

#[test]
fn a_date_that_is_not_rfc_3339_is_refused() {
    let error = parse::feed(&fixtures::synthetic("bad-date.xml"))
        .expect_err("a prose date is not RFC 3339");
    match error {
        ParseError::Timestamp { element, value, .. } => {
            assert_eq!(element, "updated");
            assert_eq!(value, "31 January 2026");
        }
        other => panic!("expected Timestamp, got {other:?}"),
    }
}

#[test]
fn a_length_that_is_not_a_byte_count_is_refused() {
    let error = parse::feed(&fixtures::synthetic("bad-length.xml"))
        .expect_err("a prose length is not a byte count");
    match error {
        ParseError::Length { value, .. } => assert_eq!(value, "a few bytes"),
        other => panic!("expected Length, got {other:?}"),
    }
}

#[test]
fn a_document_that_is_not_well_formed_is_refused() {
    let error = parse::feed("<feed xmlns=\"http://www.w3.org/2005/Atom\"><entry></feeed></feed>")
        .expect_err("a mismatched end tag is not well-formed");
    assert!(
        matches!(error, ParseError::Xml(_)),
        "expected Xml, got {error:?}"
    );
}

#[test]
fn a_listing_cut_short_is_refused_rather_than_read_as_a_short_feed() {
    let whole = fixtures::synthetic("full-feed.xml");
    let cut = whole
        .split_once("<entry>")
        .map(|(head, tail)| {
            let kept: String = tail.chars().take(200).collect();
            format!("{head}<entry>{kept}")
        })
        .expect("the fixture has entries");
    let error = parse::feed(&cut).expect_err("a truncated listing is not a feed");
    assert!(
        matches!(error, ParseError::Truncated | ParseError::Xml(_)),
        "expected Truncated, got {error:?}"
    );
}

#[test]
fn a_document_with_no_root_element_is_refused() {
    let error = parse::feed("   ").expect_err("an empty document carries no feed");
    assert!(
        matches!(error, ParseError::Truncated),
        "expected Truncated, got {error:?}"
    );
}
