// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The write routes behind FerroSMART: one `[auth.issuer.<name>]` table naming
//! its `issuer` alone, whose RFC 9068 tokens are read from their standard
//! claims. FerroSMART's registry decides which apps reach this server, through
//! the `aud` it writes (RFC 8707 §2), so the table lists no client.

use http::StatusCode;
use serde_json::{Value, json};

use super::{SigningKey, challenge, claims, issuer, put_colours};
use crate::fixture::{self, Server, SmartSetup};

/// A server trusting the issuer at `issuer` with nothing but its identifier
/// and the audience every test token names.
async fn behind_ferrosmart(issuer: &str) -> Server {
    Server::start_persisting_with_smart_setup(SmartSetup::for_issuer(issuer)).await
}

/// `claims` with each of `set` written over it.
fn with(mut claims: Value, set: &[(&str, Value)]) -> Value {
    if let Some(object) = claims.as_object_mut() {
        for (name, value) in set {
            object.insert((*name).to_owned(), value.clone());
        }
    }
    claims
}

/// A practitioner's token, issued to the app `client` under `scope`: its `sub`
/// names the person and `fhirUser` their resource (SMART App Launch 2.2.0,
/// Scopes and Launch Context, §fhirUser).
fn practitioner(issuer: &str, client: &str, scope: &str) -> Value {
    with(
        claims(issuer, scope, 300),
        &[
            ("sub", json!("practitioner-1")),
            ("client_id", json!(client)),
            ("fhirUser", json!("Practitioner/practitioner-1")),
        ],
    )
}

#[tokio::test]
async fn a_practitioner_through_an_app_writes_under_a_user_scope() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = behind_ferrosmart(&mock.uri()).await;

    let token = key.sign(&practitioner(&mock.uri(), "an-app", "user/CodeSystem.cu"));
    let response = put_colours(&server, Some(&token)).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "{}",
        challenge(&response)
    );
}

// SMART App Launch 2.2.0, Backend Services: the client authenticates as
// itself, so its token's `sub` is its `client_id` (RFC 9068 §2.2).
#[tokio::test]
async fn a_backend_services_client_writes_under_a_system_scope() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = behind_ferrosmart(&mock.uri()).await;

    let token = key.sign(&claims(&mock.uri(), "system/CodeSystem.cu", 300));
    let response = put_colours(&server, Some(&token)).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "{}",
        challenge(&response)
    );
}

// RFC 9068 §4: the resource server refuses a token whose `aud` does not name
// it, which is how an app FerroSMART did not register for this server is held
// off.
#[tokio::test]
async fn a_token_for_another_resource_server_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = behind_ferrosmart(&mock.uri()).await;

    let elsewhere = with(
        practitioner(&mock.uri(), "an-app", "user/CodeSystem.cu"),
        &[("aud", json!("https://ehr.example.org/fhir"))],
    );
    let response = put_colours(&server, Some(&key.sign(&elsewhere))).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{sent}");
    assert!(sent.contains("error=\"invalid_token\""), "{sent}");
}

// A terminology resource sits in no patient's compartment
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>), so
// the patient, the patient's representative and an actor acting for the
// patient (RFC 8693 §4.1) are admitted as callers and refused the write.
#[tokio::test]
async fn a_patient_and_whoever_acts_for_them_are_refused_the_write() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = behind_ferrosmart(&mock.uri()).await;

    let patient = with(
        claims(&mock.uri(), "patient/CodeSystem.cud patient/*.*", 300),
        &[
            ("sub", json!("patient-1")),
            ("client_id", json!("a-patient-app")),
            ("fhirUser", json!("Patient/patient-1")),
            ("patient", json!("patient-1")),
        ],
    );
    let representative = with(
        patient.clone(),
        &[
            ("sub", json!("parent-1")),
            ("fhirUser", json!("RelatedPerson/parent-1")),
        ],
    );
    let actor = with(patient.clone(), &[("act", json!({"sub": "parent-1"}))]);
    for (who, claims) in [
        ("the patient", patient),
        ("the representative", representative),
        ("the actor", actor),
    ] {
        let response = put_colours(&server, Some(&key.sign(&claims))).await;
        let sent = challenge(&response);
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{who}: {sent}");
        assert!(
            sent.contains("error=\"insufficient_scope\""),
            "{who} is admitted as a caller and refused the scope: {sent}"
        );
        let (_, body) = fixture::json(response).await;
        assert_eq!(body["issue"][0]["code"], "forbidden", "{who}");
    }
}
