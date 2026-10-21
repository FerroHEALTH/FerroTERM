// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parser against the vendored public listings.
//!
//! The three files under `vendor/feeds/` are the listings of three real
//! services, fetched verbatim by `scripts/vendor/syndication-feeds.sh` and
//! stamped in `vendor/feeds/PROVENANCE.md`. They are refreshed by hand, never
//! in CI, so a count below that stops holding means the listing moved: read
//! the new document before re-pinning the number.

use std::collections::BTreeMap;

use terminology_syndication::model::{CategoryTerm, Checksum, Entry, Feed};
use terminology_syndication::parse;

use crate::fixtures;

fn corpus(name: &str) -> Feed {
    parse::feed(&fixtures::corpus(name))
        .unwrap_or_else(|error| panic!("{name} is a well-formed syndication document: {error}"))
}

/// How many entries carry each category term, in term order.
fn terms(feed: &Feed) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in &feed.entries {
        *counts.entry(entry.term().to_string()).or_insert(0_usize) += 1;
    }
    counts
}

/// The terms of the entries the model does not name, with their counts.
fn unnamed(feed: &Feed) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in &feed.entries {
        if let CategoryTerm::Other(term) = entry.term() {
            *counts.entry(term).or_insert(0_usize) += 1;
        }
    }
    counts
}

fn expect(counts: &[(&str, usize)]) -> BTreeMap<String, usize> {
    counts
        .iter()
        .map(|(term, count)| ((*term).to_owned(), *count))
        .collect()
}

/// How many entries advertise a digest of each algorithm.
fn algorithms(feed: &Feed) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for entry in &feed.entries {
        if let Some(checksum) = entry.content_link().and_then(|link| link.checksum.as_ref()) {
            *counts.entry(checksum.algorithm()).or_insert(0_usize) += 1;
        }
    }
    counts
}

#[test]
fn the_australian_listing_parses_whole() {
    let feed = corpus("ncts.xml");
    assert_eq!(feed.entries.len(), 59);
    assert_eq!(
        feed.title.as_deref(),
        Some("National Clinical Terminology Service Syndication Feed")
    );
    assert_eq!(
        feed.profile.as_deref(),
        Some("http://ns.electronichealth.net.au/ncts/syndication/asf/profile/1.0.0")
    );
    assert_eq!(
        terms(&feed),
        expect(&[
            ("AMT_CSV", 6),
            ("AMT_TSV", 6),
            ("BINARY", 10),
            ("FHIR_Bundle", 19),
            ("SCT_RF2_ALL", 6),
            ("SCT_RF2_FULL", 6),
            ("SCT_RF2_SNAPSHOT", 6),
        ])
    );
}

#[test]
fn the_nhs_england_listing_parses_whole() {
    let feed = corpus("nhs-england.xml");
    assert_eq!(feed.entries.len(), 113);
    assert_eq!(
        terms(&feed),
        expect(&[
            ("BINARY", 14),
            ("FHIR_Bundle", 12),
            ("FHIR_CodeSystem", 38),
            ("FHIR_ConceptMap", 42),
            ("FHIR_Package", 1),
            ("FHIR_ValueSet", 6),
        ])
    );
}

#[test]
fn the_mlds_listing_parses_whole() {
    let feed = corpus("mlds.xml");
    assert_eq!(feed.entries.len(), 202);
    assert_eq!(
        terms(&feed),
        expect(&[("SCT_RF2_ALL", 196), ("SCT_RF2_FULL", 6)])
    );
}

#[test]
fn no_category_term_of_the_corpus_is_lost() {
    assert_eq!(
        unnamed(&corpus("ncts.xml")),
        expect(&[("AMT_CSV", 6), ("AMT_TSV", 6)]),
        "a term the model does not name is kept verbatim, never dropped"
    );
    assert_eq!(
        unnamed(&corpus("nhs-england.xml")),
        BTreeMap::new(),
        "every NHS England term is one the model names"
    );
    assert_eq!(
        unnamed(&corpus("mlds.xml")),
        BTreeMap::new(),
        "every MLDS term is one the model names"
    );
}

#[test]
fn the_ontoserver_services_advertise_sha256_digests() {
    assert_eq!(
        algorithms(&corpus("ncts.xml")),
        BTreeMap::from([("sha256", 59)])
    );
    assert_eq!(
        algorithms(&corpus("nhs-england.xml")),
        BTreeMap::from([("sha256", 113)])
    );
}

#[test]
fn the_mlds_listing_advertises_md5_digests() {
    let feed = corpus("mlds.xml");
    assert_eq!(algorithms(&feed), BTreeMap::from([("md5", 176)]));
    let undigested = feed
        .entries
        .iter()
        .filter(|entry| {
            entry
                .content_link()
                .is_some_and(|link| link.checksum.is_none())
        })
        .count();
    assert_eq!(
        undigested, 25,
        "an MLDS package link may advertise no digest, and the fetch path refuses such an entry"
    );
}

#[test]
fn every_corpus_entry_carries_an_identity_a_date_and_a_canonical() {
    for name in ["ncts.xml", "nhs-england.xml", "mlds.xml"] {
        let feed = corpus(name);
        for entry in &feed.entries {
            assert!(!entry.id.is_empty(), "{name}: an entry has no id");
            assert!(!entry.title.is_empty(), "{name}: {} has no title", entry.id);
            assert!(
                entry.updated.is_some(),
                "{name}: {} has no updated date",
                entry.id
            );
            assert!(
                entry.content_item_identifier.is_some(),
                "{name}: {} names no canonical identifier",
                entry.id
            );
        }
    }
}

#[test]
fn every_ontoserver_corpus_entry_offers_a_content_link() {
    for name in ["ncts.xml", "nhs-england.xml"] {
        let feed = corpus(name);
        for entry in &feed.entries {
            assert!(
                entry.content_link().is_some(),
                "{name}: {} offers no content link",
                entry.id
            );
        }
    }
}

#[test]
fn an_mlds_entry_that_offers_only_related_links_has_no_content_link() {
    let feed = corpus("mlds.xml");
    let linkless: Vec<&str> = feed
        .entries
        .iter()
        .filter(|entry| entry.content_link().is_none())
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(
        linkless,
        ["urn:uuid:307deb09-075f-41f7-841c-d82bc87580d6"],
        "an entry whose every link is rel=related offers nothing to fetch, and selection says so"
    );
}

#[test]
fn a_snomed_version_uri_in_the_corpus_yields_its_edition_and_version() {
    let feed = corpus("mlds.xml");
    let releases: Vec<_> = feed
        .entries
        .iter()
        .filter_map(Entry::snomed_release)
        .collect();
    assert_eq!(
        releases.len(),
        feed.entries.len(),
        "every MLDS entry versions itself with a SNOMED CT version URI"
    );
    for release in releases {
        assert_eq!(release.version.len(), 8, "a release version is YYYYMMDD");
    }
}

#[test]
fn a_corpus_entry_with_a_release_note_keeps_both_links_apart() {
    let feed = corpus("mlds.xml");
    let with_notes = feed
        .entries
        .iter()
        .filter(|entry| entry.links.len() > 1)
        .count();
    assert_eq!(
        with_notes, 190,
        "an MLDS entry publishes its release notes as a related link beside the package"
    );
    for entry in &feed.entries {
        let Some(link) = entry.content_link() else {
            continue;
        };
        assert!(
            link.checksum.is_none() || matches!(link.checksum, Some(Checksum::Md5(_))),
            "{}: a digest on an MLDS package link is the md5 the sct: extension advertises",
            entry.id
        );
    }
}
