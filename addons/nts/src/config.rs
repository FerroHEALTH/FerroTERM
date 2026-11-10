// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the service is, and which of its content a run takes.
//!
//! The service's address, its timeouts, its TLS and the client credentials
//! grant are the synchronisation service's `[nts]` endpoint table, which
//! starts from [`endpoint`]. A source's own block carries no secret and no
//! address of the service, so it can be committed, logged, and shown in a run
//! record whole.

use std::collections::BTreeSet;

use ferrohealth_config::units::Duration;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::ClientAuth;
use ferrohealth_smart::config::credentials::{CredentialsSection, OauthSection};
use ferrohealth_smart::config::endpoint::EndpointSection;
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

/// The issuer identifier of the service's authorization server.
///
/// The client reads the token endpoint from this issuer's metadata
/// (OpenID Connect Discovery 1.0 §4), whose `issuer` member names this value
/// exactly (measured 2026-11-08).
pub const NTS_ISSUER: &str = "https://terminologieserver.nl/authorisation/auth/realms/nictiz";

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

/// The `[nts]` endpoint table a deployment starts from.
///
/// It names the service's base URL, and the client credentials grant
/// (RFC 6749 §4.4) against [`NTS_ISSUER`] with the client secret in the
/// request body, which is how the service's own examples send it (RFC 6749
/// §2.3.1).
///
/// A call may take an hour, since a download of a whole edition is one call,
/// and opening the connection may take 30 seconds. A token is renewed a
/// minute before it expires, so a token that expires between the decision to
/// use it and the service reading it does not fail a run. A deployment
/// writes `client_id` and its secret under `[nts.credentials.oauth]`.
#[must_use]
pub fn endpoint() -> EndpointSection {
    EndpointSection {
        url: SecretUrl::new(NTS_BASE_URL),
        timeout: Duration::from_hours(1),
        connect_timeout: Some(Duration::from_secs(30)),
        credentials: CredentialsSection {
            oauth: Some(OauthSection {
                issuer: String::from(NTS_ISSUER),
                client_auth: ClientAuth::ClientSecretPost,
                refresh_margin: Duration::from_mins(1),
                ..OauthSection::default()
            }),
            ..CredentialsSection::default()
        },
        ..EndpointSection::default()
    }
}

/// The add-on's configuration, one source's block.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NtsConfig {
    /// The name a run record and a log line give this source.
    pub name: String,
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
    /// The address of the syndication feed under the service's base URL
    /// `base`.
    #[must_use]
    pub fn feed_url(&self, base: &str) -> String {
        self.feed_url
            .clone()
            .unwrap_or_else(|| format!("{}{FEED_PATH}", base.trim_end_matches('/')))
    }

    /// The address of the FHIR endpoint a run lists under the service's base
    /// URL `base`, when it lists one.
    #[must_use]
    pub fn fhir_api_url(&self, base: &str) -> Option<String> {
        self.fhir_api.then(|| {
            self.fhir_api_url
                .clone()
                .unwrap_or_else(|| format!("{}{FHIR_API_PATH}", base.trim_end_matches('/')))
        })
    }

    /// The subscription the configuration describes.
    #[must_use]
    pub fn subscription(&self) -> Subscription {
        let mut subscription = Subscription::default();
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
    use super::{NTS_BASE_URL, NtsConfig, canonical, endpoint};
    use ferrohealth_smart::client::ClientAuth;
    use terminology_syndication::model::CategoryTerm;

    #[test]
    fn the_default_feed_is_the_service_listing() {
        let config = NtsConfig::default();
        let base = endpoint().url.expose().to_owned();
        assert_eq!(
            config.feed_url(&base),
            "https://terminologieserver.nl/synd/syndication.xml",
            "the default feed is the one the service publishes"
        );
        let oauth = endpoint()
            .credentials
            .oauth
            .expect("the default endpoint names the grant");
        assert_eq!(
            oauth.issuer, "https://terminologieserver.nl/authorisation/auth/realms/nictiz",
            "the tokens come from the service's own issuer"
        );
        assert_eq!(
            oauth.client_auth,
            ClientAuth::ClientSecretPost,
            "the secret goes in the body, as the service's examples send it"
        );
        assert_eq!(
            config.fhir_api_url(&base).as_deref(),
            Some("https://terminologieserver.nl/fhir"),
            "the FHIR endpoint is listed by default, since the feed carries no resources"
        );
        let without = NtsConfig {
            fhir_api: false,
            ..NtsConfig::default()
        };
        assert!(
            without.fhir_api_url(NTS_BASE_URL).is_none(),
            "a deployment can keep a run to the feed"
        );
    }

    #[test]
    fn another_base_url_moves_both_addresses() {
        let config = NtsConfig::default();
        let base = "https://apps.example.invalid/ontoserver/";
        assert_eq!(
            config.feed_url(base),
            "https://apps.example.invalid/ontoserver/synd/syndication.xml",
            "a second deployment is this add-on pointed elsewhere"
        );
        assert_eq!(
            config.fhir_api_url(base).as_deref(),
            Some("https://apps.example.invalid/ontoserver/fhir"),
            "the FHIR endpoint follows the base URL"
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
    fn a_key_0_4_5_retired_is_an_unknown_key() {
        for retired in [
            r#"{"base_url":"https://example.invalid"}"#,
            r#"{"issuer":"https://example.invalid/realm"}"#,
        ] {
            let refusal = serde_json::from_str::<NtsConfig>(retired)
                .expect_err("the strict load refuses a key it does not know")
                .to_string();
            assert!(refusal.contains("unknown field"), "{refusal}");
        }
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
