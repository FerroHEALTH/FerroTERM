// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Listing the terminology resources a FHIR server serves, as feed entries.
//!
//! Some services keep their FHIR resources behind their FHIR API rather than
//! in the syndication feed: the Nictiz Nationale Terminologieserver's feed
//! carries only the SNOMED CT binary index, while its `CodeSystem`,
//! `ValueSet`, and `ConceptMap` resources answer on the FHIR endpoint. This
//! module reads those search results into the same [`Entry`] shape the feed
//! yields, so the subscription, the replace rule, and the resource lane apply
//! unchanged. What it reads from a resource is the same in every FHIR version
//! from R4 on: `url`, `version`, `id`, `meta.lastUpdated`, and the search
//! bundle's `next` link
//! (<https://hl7.org/fhir/R4/search.html#paging>).
//!
//! No FHIR specification governs a feed built from a search; the mapping to an
//! entry is our own design.

use crate::model::{Category, CategoryTerm, ContentLink, Entry, Feed, LinkRel};
use crate::source::{Authorization, SourceError};

/// The resource types the listing reads, in the order it reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceType {
    /// `CodeSystem`.
    CodeSystem,
    /// `ValueSet`.
    ValueSet,
    /// `ConceptMap`.
    ConceptMap,
}

impl ResourceType {
    /// Every type the listing reads.
    pub const ALL: [Self; 3] = [Self::CodeSystem, Self::ValueSet, Self::ConceptMap];

    /// The type's name, as the request path and `resourceType` spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CodeSystem => "CodeSystem",
            Self::ValueSet => "ValueSet",
            Self::ConceptMap => "ConceptMap",
        }
    }

    /// The category term an entry of this type carries.
    #[must_use]
    pub const fn term(self) -> CategoryTerm {
        match self {
            Self::CodeSystem => CategoryTerm::FhirCodeSystem,
            Self::ValueSet => CategoryTerm::FhirValueSet,
            Self::ConceptMap => CategoryTerm::FhirConceptMap,
        }
    }
}

/// The media type every request asks for and every entry links.
pub const FHIR_JSON: &str = "application/fhir+json";

// NOTE: `_summary=true` keeps the listing to the summary elements, which is
// what the entry needs, and 500 is a page size Ontoserver serves in one
// answer (measured on 2026-09-25, issue #602).
const PAGE: &str = "_summary=true&_count=500";

/// Lists the terminology resources at `base_url` as one feed.
///
/// Each type in [`ResourceType::ALL`] is searched and paged to the end. An
/// entry is identified as `{Type}/{id}`, carries the resource's `url` and
/// `version` as its content item, `meta.lastUpdated` as its date, and one
/// `alternate` link to the resource itself, with no digest.
///
/// # Errors
///
/// Returns [`SourceError::Request`] when a page cannot be fetched,
/// [`SourceError::Status`] when the server answers a page with a status that
/// is not a success, and [`SourceError::ApiParse`] when a page is not a
/// JSON bundle.
pub async fn list(
    client: &reqwest::Client,
    base_url: &str,
    authorization: &Authorization,
) -> Result<Feed, SourceError> {
    let base = base_url.trim_end_matches('/');
    let mut entries = Vec::new();
    for resource_type in ResourceType::ALL {
        let mut next = Some(format!("{base}/{}?{PAGE}", resource_type.as_str()));
        while let Some(url) = next.take() {
            let page = fetch_page(client, &url, authorization).await?;
            next = next_link(&page);
            for resource in page
                .get("entry")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.get("resource"))
            {
                entries.push(entry_of(base, resource_type, resource));
            }
        }
    }
    Ok(Feed {
        title: Some(format!("the FHIR API at {base}")),
        id: Some(base.to_owned()),
        entries,
        ..Feed::default()
    })
}

async fn fetch_page(
    client: &reqwest::Client,
    url: &str,
    authorization: &Authorization,
) -> Result<serde_json::Value, SourceError> {
    let response = authorization
        .apply(client.get(url).header(reqwest::header::ACCEPT, FHIR_JSON))
        .send()
        .await
        .map_err(|source| SourceError::Request {
            url: url.to_owned(),
            source,
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(SourceError::Status {
            url: url.to_owned(),
            status,
        });
    }
    let body = response
        .bytes()
        .await
        .map_err(|source| SourceError::Request {
            url: url.to_owned(),
            source,
        })?;
    serde_json::from_slice(&body).map_err(|source| SourceError::ApiParse {
        url: url.to_owned(),
        source,
    })
}

/// The `next` link of a search bundle, when it has one.
fn next_link(page: &serde_json::Value) -> Option<String> {
    page.get("link")?
        .as_array()?
        .iter()
        .find(|link| link.get("relation").and_then(serde_json::Value::as_str) == Some("next"))
        .and_then(|link| link.get("url"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// One resource of a search page as a feed entry.
fn entry_of(base: &str, resource_type: ResourceType, resource: &serde_json::Value) -> Entry {
    let text = |name: &str| {
        resource
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let id = text("id").unwrap_or_default();
    let url = text("url");
    let title = text("title")
        .or_else(|| text("name"))
        .or_else(|| url.clone())
        .unwrap_or_else(|| id.clone());
    let updated = resource
        .get("meta")
        .and_then(|meta| meta.get("lastUpdated"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse().ok());
    // NOTE: a `not-present` CodeSystem carries no concepts
    // (<https://hl7.org/fhir/R4/valueset-codesystem-content-mode.html>), so it
    // gets its own term and the selection reports it instead of serving a stub.
    let category = if text("content").as_deref() == Some("not-present") {
        Category {
            term: CategoryTerm::Other(format!(
                "{}(content=not-present)",
                resource_type.term().as_str()
            )),
            label: Some(format!("FHIR {} without content", resource_type.as_str())),
            scheme: Some(base.to_owned()),
        }
    } else {
        Category {
            term: resource_type.term(),
            label: Some(format!("FHIR {}", resource_type.as_str())),
            scheme: Some(base.to_owned()),
        }
    };
    Entry {
        id: format!("{}/{id}", resource_type.as_str()),
        title,
        category: Some(category),
        updated,
        content_item_identifier: url,
        content_item_version: text("version"),
        fhir_version: None,
        links: vec![ContentLink {
            href: format!("{base}/{}/{id}", resource_type.as_str()),
            rel: LinkRel::Alternate,
            media_type: Some(String::from(FHIR_JSON)),
            length: None,
            checksum: None,
            validated: false,
        }],
        ..Entry::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{ResourceType, entry_of, next_link};
    use crate::model::CategoryTerm;

    #[test]
    fn a_resource_becomes_an_entry_the_selection_can_read() {
        let resource = serde_json::json!({
            "resourceType": "CodeSystem",
            "id": "icd-10-nl-2021v3-1c",
            "url": "http://hl7.org/fhir/sid/icd-10-nl",
            "version": "ICD-10 2021v3cd",
            "meta": {"lastUpdated": "2025-11-08T01:47:54.230+01:00"},
            "title": null,
            "name": "ICD10NL"
        });
        let entry = entry_of(
            "https://example.invalid/fhir",
            ResourceType::CodeSystem,
            &resource,
        );
        assert_eq!(
            entry.id, "CodeSystem/icd-10-nl-2021v3-1c",
            "the entry is the resource's address"
        );
        assert_eq!(
            entry.title, "ICD10NL",
            "the name stands in for an absent title"
        );
        assert_eq!(
            entry.term(),
            CategoryTerm::FhirCodeSystem,
            "the type is the category"
        );
        assert_eq!(
            entry.content_item_identifier.as_deref(),
            Some("http://hl7.org/fhir/sid/icd-10-nl"),
            "the canonical is the content item"
        );
        assert_eq!(
            entry.content_item_version.as_deref(),
            Some("ICD-10 2021v3cd"),
            "the version is the content item version"
        );
        assert_eq!(
            entry.updated.map(|at| at.to_string()),
            Some(String::from("2025-11-08T00:47:54.23Z")),
            "lastUpdated is the date the replace rule compares"
        );
        let link = entry.content_link().expect("the entry links the resource");
        assert_eq!(
            link.href, "https://example.invalid/fhir/CodeSystem/icd-10-nl-2021v3-1c",
            "the link reads the resource by id"
        );
        assert!(link.checksum.is_none(), "a FHIR API advertises no digest");
    }

    #[test]
    fn a_code_system_without_content_is_named_as_such() {
        let resource = serde_json::json!({
            "resourceType": "CodeSystem", "id": "s", "url": "http://snomed.info/sct",
            "version": "http://snomed.info/sct/11000146104/version/20260831",
            "content": "not-present"
        });
        let entry = entry_of(
            "https://example.invalid/fhir",
            ResourceType::CodeSystem,
            &resource,
        );
        assert_eq!(
            entry.term(),
            CategoryTerm::Other(String::from("FHIR_CodeSystem(content=not-present)")),
            "a stub is not a code system, and the selection reports the term"
        );
    }

    #[test]
    fn a_resource_without_a_canonical_still_becomes_an_entry() {
        let resource = serde_json::json!({"resourceType": "ValueSet", "id": "x"});
        let entry = entry_of(
            "https://example.invalid/fhir",
            ResourceType::ValueSet,
            &resource,
        );
        assert_eq!(entry.title, "x", "the id is the last resort for a title");
        assert!(
            entry.content_item_identifier.is_none(),
            "the selection reports it as having no canonical rather than this module dropping it"
        );
    }

    #[test]
    fn the_next_link_is_followed_and_its_absence_ends_the_paging() {
        let page = serde_json::json!({"link": [
            {"relation": "self", "url": "https://example.invalid/fhir/ValueSet?_count=2"},
            {"relation": "next", "url": "https://example.invalid/fhir/ValueSet?_count=2&_getpagesoffset=2"}
        ]});
        assert_eq!(
            next_link(&page).as_deref(),
            Some("https://example.invalid/fhir/ValueSet?_count=2&_getpagesoffset=2"),
            "the next link is the one followed"
        );
        let last = serde_json::json!({"link": [{"relation": "self", "url": "u"}]});
        assert!(next_link(&last).is_none(), "the last page has no next link");
    }
}
