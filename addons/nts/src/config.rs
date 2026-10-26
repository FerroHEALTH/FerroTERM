// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the service is, and which of its content a run takes.
//!
//! The configuration carries no secret: the synchronisation service hands the
//! credentials over ([`crate::credentials::Credentials`]), so a configuration
//! block can be committed, logged, and shown in a run record whole.

use std::collections::BTreeSet;

use terminology_syndication::model::CategoryTerm;
use terminology_syndication::select::Subscription;

/// The base URL of the Nationale Terminologie Server.
pub const NTS_BASE_URL: &str = "https://terminologieserver.nl";

/// The path of the syndication feed under the base URL.
pub const FEED_PATH: &str = "/synd/syndication.xml";

/// The path of the FHIR endpoint under the base URL.
///
/// The feed carries only the SNOMED CT binary index (measured 2026-09-25,
/// issue #602); the `CodeSystem`, `ValueSet`, and `ConceptMap` resources
/// answer here.
pub const FHIR_API_PATH: &str = "/fhir";

/// The path of the SMART configuration document under the base URL.
///
/// The document is served beside the FHIR endpoint, which is where SMART App
/// Launch places it: "the well-known URI `.well-known/smart-configuration` of
/// the FHIR base URL"
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
pub const DISCOVERY_PATH: &str = "/fhir/.well-known/smart-configuration";

/// The canonical identifiers of the systems the service publishes.
///
/// A subscription names systems by canonical identifier, which is what the
/// feed puts in `ncts:contentItemIdentifier`.
pub mod canonical {
    /// SNOMED CT, the Netherlands edition.
    ///
    /// The SNOMED CT URI Standard gives an edition the URI
    /// `http://snomed.info/sct/{moduleId}` and lists `11000146104` as the
    /// Netherlands edition (<https://confluence.ihtsdotools.org/display/DOCURI>).
    pub const SNOMED_CT_NL: &str = "http://snomed.info/sct/11000146104";

    /// LOINC, whose canonical the FHIR specification fixes
    /// (<https://hl7.org/fhir/R4/terminologies-systems.html>).
    pub const LOINC: &str = "http://loinc.org";

    /// UCUM, whose canonical the FHIR specification fixes
    /// (<https://hl7.org/fhir/R4/terminologies-systems.html>).
    pub const UCUM: &str = "http://unitsofmeasure.org";

    /// ICD-10, whose canonical the FHIR specification fixes
    /// (<https://hl7.org/fhir/R4/terminologies-systems.html>).
    pub const ICD_10: &str = "http://hl7.org/fhir/sid/icd-10";

    /// ICD-10, the Dutch translation the service serves under its own
    /// canonical, versioned `ICD-10 2021v3cd` and its predecessors.
    pub const ICD_10_NL: &str = "http://hl7.org/fhir/sid/icd-10-nl";

    /// The Nederlandse Labcodeset, served as a LOINC supplement.
    pub const LABCODESET: &str = "http://labterminologie.nl/cs/labconcepts";

    /// The three Labcodeset concept maps, one canonical each.
    pub const LABCODESET_MAPS: [&str; 3] = [
        "http://labterminologie.nl/cm/labconcepts-materials",
        "http://labterminologie.nl/cm/labconcepts-outcomes",
        "http://labterminologie.nl/cm/labconcepts-ucum",
    ];

    /// NHG-Tabel 24, the Dutch ICPC-1, under the HL7-assigned canonical.
    pub const ICPC_1_NL: &str = "http://hl7.org/fhir/sid/icpc-1-nl";

    /// The prefix of the other NHG tables; a table's canonical is this prefix,
    /// its number, and its slug (`nhg-tabel-45-diagnostische-bepalingen`).
    pub const NHG_TABLE_PREFIX: &str = "https://referentiemodel.nhg.org/tabellen/";

    /// The prefix of the zib value sets, which ART-DECOR publishes as
    /// `{prefix}{oid}--{yyyymmddhhmmss}`.
    pub const ZIB_VALUE_SET_PREFIX: &str = "http://decor.nictiz.nl/fhir/ValueSet/";

    /// The canonicals a run subscribes to unless the configuration names its
    /// own.
    pub const DEFAULT: [&str; 4] = [SNOMED_CT_NL, LOINC, UCUM, ICD_10];
}

// NOTE: the canonicals above LOINC/UCUM/ICD-10 were read from the service's
// FHIR API on 2026-09-25 (issue #602); its syndication feed listed SNOMED CT
// only, so none of them is a feed identifier yet.

/// The add-on's configuration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NtsConfig {
    /// The name a run record and a log line give this source.
    pub name: String,
    /// The service's base URL, without a trailing slash.
    pub base_url: String,
    /// The feed's address, when it is not [`FEED_PATH`] under the base URL.
    pub feed_url: Option<String>,
    /// The FHIR endpoint's address, when it is not [`FHIR_API_PATH`] under
    /// the base URL.
    pub fhir_api_url: Option<String>,
    /// Whether a run lists the FHIR endpoint beside the feed.
    pub fhir_api: bool,
    /// The canonical identifiers the run takes.
    pub systems: BTreeSet<String>,
    /// The category terms the run takes, as the feed writes them.
    pub categories: BTreeSet<String>,
}

impl Default for NtsConfig {
    fn default() -> Self {
        Self {
            name: String::from("nts"),
            base_url: String::from(NTS_BASE_URL),
            feed_url: None,
            fhir_api_url: None,
            fhir_api: true,
            systems: canonical::DEFAULT
                .iter()
                .map(|&s| String::from(s))
                .collect(),
            categories: [
                CategoryTerm::SnomedRf2Snapshot,
                CategoryTerm::SnomedRf2All,
                CategoryTerm::FhirCodeSystem,
                CategoryTerm::FhirValueSet,
                CategoryTerm::FhirConceptMap,
                CategoryTerm::FhirBundle,
            ]
            .iter()
            .map(|term| String::from(term.as_str()))
            .collect(),
        }
    }
}

impl NtsConfig {
    /// The address of the syndication feed.
    #[must_use]
    pub fn feed_url(&self) -> String {
        self.feed_url
            .clone()
            .unwrap_or_else(|| format!("{}{FEED_PATH}", self.base_url.trim_end_matches('/')))
    }

    /// The address of the FHIR endpoint a run lists, when it lists one.
    #[must_use]
    pub fn fhir_api_url(&self) -> Option<String> {
        self.fhir_api.then(|| {
            self.fhir_api_url.clone().unwrap_or_else(|| {
                format!("{}{FHIR_API_PATH}", self.base_url.trim_end_matches('/'))
            })
        })
    }

    /// The address of the SMART configuration document.
    #[must_use]
    pub fn discovery_url(&self) -> String {
        format!("{}{DISCOVERY_PATH}", self.base_url.trim_end_matches('/'))
    }

    /// The subscription the configuration describes.
    #[must_use]
    pub fn subscription(&self) -> Subscription {
        let mut subscription = Subscription::new();
        for canonical in &self.systems {
            subscription = subscription.with_system(canonical.clone());
        }
        for term in &self.categories {
            subscription = subscription.with_category(CategoryTerm::parse(term));
        }
        subscription
    }

    /// The category terms the source is expected to offer, in a stable order.
    #[must_use]
    pub fn yields(&self) -> Vec<CategoryTerm> {
        self.categories
            .iter()
            .map(|term| CategoryTerm::parse(term))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{NtsConfig, canonical};
    use terminology_syndication::model::CategoryTerm;

    #[test]
    fn the_default_feed_is_the_service_listing() {
        let config = NtsConfig::default();
        assert_eq!(
            config.feed_url(),
            "https://terminologieserver.nl/synd/syndication.xml",
            "the default feed is the one the service publishes"
        );
        assert_eq!(
            config.discovery_url(),
            "https://terminologieserver.nl/fhir/.well-known/smart-configuration",
            "discovery sits beside the FHIR endpoint"
        );
        assert_eq!(
            config.fhir_api_url().as_deref(),
            Some("https://terminologieserver.nl/fhir"),
            "the FHIR endpoint is listed by default, since the feed carries no resources"
        );
        let without = NtsConfig {
            fhir_api: false,
            ..NtsConfig::default()
        };
        assert!(
            without.fhir_api_url().is_none(),
            "a deployment can keep a run to the feed"
        );
    }

    #[test]
    fn another_base_url_moves_both_addresses() {
        let config = NtsConfig {
            base_url: String::from("https://apps.example.invalid/ontoserver/"),
            ..NtsConfig::default()
        };
        assert_eq!(
            config.feed_url(),
            "https://apps.example.invalid/ontoserver/synd/syndication.xml",
            "a second deployment is this add-on pointed elsewhere"
        );
        assert_eq!(
            config.discovery_url(),
            "https://apps.example.invalid/ontoserver/fhir/.well-known/smart-configuration",
            "discovery follows the base URL"
        );
    }

    #[test]
    fn the_subscription_names_its_systems_by_canonical() {
        let subscription = NtsConfig::default().subscription();
        assert!(
            subscription.systems.admits(canonical::SNOMED_CT_NL, ""),
            "the Netherlands edition is subscribed by default"
        );
        assert!(
            subscription.systems.admits(
                "http://snomed.info/sct",
                "http://snomed.info/sct/11000146104/version/20260831"
            ),
            "the service identifies a release by the bare code system and the edition version URI"
        );
        assert!(
            !subscription.systems.admits(
                "http://snomed.info/sct",
                "http://snomed.info/sct/900000000000207008/version/20260901"
            ),
            "a release of another edition is outside the subscription"
        );
        assert!(
            !subscription
                .systems
                .admits("http://example.invalid/CodeSystem/other", ""),
            "a system the configuration does not name is outside the subscription"
        );
        assert!(
            subscription
                .categories
                .contains(&CategoryTerm::SnomedRf2Snapshot),
            "the snapshot is what a build reads"
        );
        assert!(
            !subscription.categories.contains(&CategoryTerm::BinaryIndex),
            "the binary index is never subscribed"
        );
    }

    #[test]
    fn the_configuration_body_carries_no_secret() {
        let config: NtsConfig =
            serde_json::from_str(r#"{"name":"nts"}"#).expect("the configuration reads");
        assert_eq!(config.name, "nts");
        for refused in [
            r#"{"password":"secret"}"#,
            r#"{"credentials":{"from":"environment"}}"#,
        ] {
            assert!(
                serde_json::from_str::<NtsConfig>(refused).is_err(),
                "a credential field in the configuration body is refused: {refused}"
            );
        }
    }
}
