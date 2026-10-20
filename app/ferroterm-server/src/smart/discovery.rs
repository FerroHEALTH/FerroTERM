//! The SMART discovery document of this server, built from the issuer's
//! metadata as `ferrohealth-smart` read it.
//!
//! The document is this server's own assertion about itself
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>), so a value the
//! specification does not define for a member is dropped rather than passed
//! through from the issuer. It stays here, beside the crate's own builder, for
//! two members a terminology server states differently: `issuer` appears only
//! where `sso-openid-connect` is claimed, and `permission-patient` is never
//! claimed, because a `patient/` scope writes nothing on this server.

use ferrohealth_smart::metadata::ServerMetadata;

/// The grant the authorization code flow of a standalone launch runs on
/// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
const AUTHORIZATION_CODE: &str = "authorization_code";

/// The grant types the SMART discovery document names as its options.
const GRANT_TYPES: [&str; 2] = [AUTHORIZATION_CODE, "client_credentials"];

/// The client authentication methods the SMART discovery document names as its
/// options.
const TOKEN_ENDPOINT_AUTH_METHODS: [&str; 3] = [
    "client_secret_post",
    "client_secret_basic",
    "private_key_jwt",
];

/// The PKCE methods the SMART discovery document publishes.
///
/// "SMART servers SHALL support the `S256` `code_challenge_method` and SHALL
/// NOT support the `plain` method", so the list is this one whatever the issuer
/// advertises; `S256` is the SHA-256 challenge of RFC 7636 §4.2
/// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
const CODE_CHALLENGE_METHODS: [&str; 1] = ["S256"];

/// The scope an issuer advertises when it mints an identity token
/// (<https://openid.net/specs/openid-connect-core-1_0.html>, §3.1.2.1).
const OPENID: &str = "openid";

/// The members of `advertised` that `options` admits, in the order `options`
/// fixes them.
fn only(advertised: &[String], options: &[&str]) -> Vec<String> {
    options
        .iter()
        .filter(|option| advertised.iter().any(|value| value == *option))
        .map(|option| (*option).to_owned())
        .collect()
}

/// Whether the issuer runs the authorization code flow a standalone launch
/// needs (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
fn standalone_launch(metadata: &ServerMetadata) -> bool {
    metadata.authorization_endpoint.is_some()
        && metadata
            .grant_types()
            .iter()
            .any(|grant| grant == AUTHORIZATION_CODE)
}

/// Whether the issuer offers the OpenID Connect profile the
/// `sso-openid-connect` capability rests on.
fn openid_connect(metadata: &ServerMetadata) -> bool {
    metadata
        .scopes_supported
        .iter()
        .flatten()
        .any(|scope| scope == OPENID)
}

/// The SMART capabilities the deployment offers, sorted: the ones read off
/// the issuer's metadata and `added`, the ones `[auth] capabilities` names.
///
/// The scope syntaxes are this server's own; every other capability is claimed
/// only where the issuer advertises what it rests on
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
#[must_use]
pub fn capabilities(metadata: &ServerMetadata, added: &[String]) -> Vec<String> {
    let methods = metadata.token_endpoint_auth_methods();
    // NOTE: `permission-v2` is exemplified only by the search-parameter
    // syntax this server refuses, so withholding it is our own reading
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    let mut out = vec![
        String::from("permission-v1"),
        String::from("permission-user"),
    ];
    if standalone_launch(metadata) {
        out.push(String::from("launch-standalone"));
    }
    if openid_connect(metadata) {
        out.push(String::from("sso-openid-connect"));
    }
    if methods.iter().any(|method| method == "private_key_jwt") {
        out.push(String::from("client-confidential-asymmetric"));
    }
    if methods
        .iter()
        .any(|method| method == "client_secret_basic" || method == "client_secret_post")
    {
        out.push(String::from("client-confidential-symmetric"));
    }
    if methods.iter().any(|method| method == "none") {
        out.push(String::from("client-public"));
    }
    for capability in added {
        if !out.contains(capability) {
            out.push(capability.clone());
        }
    }
    out.sort();
    out
}

/// The `.well-known/smart-configuration` document of the deployment.
///
/// The members are the issuer's, with `capabilities` derived from what the
/// issuer advertises; `token_endpoint`, `grant_types_supported`,
/// `capabilities`, and `code_challenge_methods_supported` are the required
/// ones (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
/// `scopes_supported` lists the scopes `[auth] scopes_supported` names,
/// because "the server SHALL support all scopes listed here", and never the
/// issuer's own list.
#[must_use]
pub fn document(
    metadata: &ServerMetadata,
    token_endpoint: &str,
    scopes_supported: &[String],
    added: &[String],
) -> serde_json::Value {
    let mut document = serde_json::Map::new();
    let mut put = |name: &str, value: serde_json::Value| {
        document.insert(name.to_owned(), value);
    };
    let capabilities = capabilities(metadata, added);
    // NOTE: `issuer` is required where `sso-openid-connect` is claimed and
    // omitted otherwise
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    if capabilities
        .iter()
        .any(|capability| capability == "sso-openid-connect")
    {
        put("issuer", metadata.issuer.as_str().into());
    }
    if let Some(jwks_uri) = &metadata.jwks_uri {
        put("jwks_uri", jwks_uri.as_str().into());
    }
    put("token_endpoint", token_endpoint.into());
    put(
        "grant_types_supported",
        only(&metadata.grant_types(), &GRANT_TYPES).into(),
    );
    put("capabilities", capabilities.into());
    // NOTE: the member is required and `S256` is a SHALL in it, so the
    // server states its own PKCE support rather than the issuer's list
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    put(
        "code_challenge_methods_supported",
        CODE_CHALLENGE_METHODS
            .iter()
            .map(|method| (*method).to_owned())
            .collect::<Vec<_>>()
            .into(),
    );
    for (name, endpoint) in [
        ("authorization_endpoint", &metadata.authorization_endpoint),
        ("registration_endpoint", &metadata.registration_endpoint),
        ("introspection_endpoint", &metadata.introspection_endpoint),
        ("revocation_endpoint", &metadata.revocation_endpoint),
    ] {
        if let Some(endpoint) = endpoint {
            put(name, endpoint.as_str().into());
        }
    }
    let authentication = only(
        &metadata.token_endpoint_auth_methods(),
        &TOKEN_ENDPOINT_AUTH_METHODS,
    );
    for (name, values) in [
        ("scopes_supported", scopes_supported.to_vec()),
        (
            "response_types_supported",
            metadata
                .response_types_supported
                .clone()
                .unwrap_or_default(),
        ),
        ("token_endpoint_auth_methods_supported", authentication),
    ] {
        if !values.is_empty() {
            put(name, values.into());
        }
    }
    serde_json::Value::Object(document)
}

#[cfg(test)]
mod tests {
    use ferrohealth_smart::metadata::ServerMetadata;

    use super::{capabilities, document};

    fn metadata() -> ServerMetadata {
        ServerMetadata {
            issuer: String::from("https://as.example.org"),
            ..ServerMetadata::default()
        }
    }

    // A terminology server holds no patient record, so a `patient/` scope
    // writes nothing here and the capability that promises them is withheld.
    #[test]
    fn permission_patient_is_never_claimed() {
        let claimed = capabilities(&metadata(), &[]);
        assert!(
            !claimed.iter().any(|c| c == "permission-patient"),
            "{claimed:?}"
        );
        assert!(claimed.iter().any(|c| c == "permission-v1"));
    }

    // Additional capabilities are listed as `[auth] capabilities` names them
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    #[test]
    fn a_configured_capability_is_added_once() {
        let added = [String::from("launch-ehr"), String::from("permission-v1")];
        let claimed = capabilities(&metadata(), &added);
        assert_eq!(
            claimed.iter().filter(|c| *c == "permission-v1").count(),
            1,
            "{claimed:?}"
        );
        assert!(claimed.iter().any(|c| c == "launch-ehr"));
    }

    // The server SHALL support every scope it lists, so the issuer's own list
    // is never copied (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    #[test]
    fn scopes_supported_is_the_configured_list_alone() {
        let with_issuer_scopes = ServerMetadata {
            scopes_supported: Some(vec![String::from("patient/*.rs")]),
            ..metadata()
        };
        let none = document(
            &with_issuer_scopes,
            "https://as.example.org/token",
            &[],
            &[],
        );
        assert!(none.get("scopes_supported").is_none(), "{none}");
        let listed = [String::from("system/CodeSystem.cud")];
        let some = document(
            &with_issuer_scopes,
            "https://as.example.org/token",
            &listed,
            &[],
        );
        assert_eq!(
            some["scopes_supported"],
            serde_json::json!(["system/CodeSystem.cud"])
        );
    }
}
