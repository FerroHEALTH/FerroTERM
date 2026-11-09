// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The callers of a `patients` issuer, the patient's representative among
//! them, on the write routes.

use ferrohealth_smart::kind::IssuerKind;
use http::StatusCode;
use serde_json::{Value, json};

use super::{SigningKey, challenge, claims, issuer, put_colours};
use crate::fixture::{self, Server, SmartSetup};

/// A server whose one issuer signs `patients` tokens, with the profile key
/// `act_represents_patient` set to `represents`.
async fn patients_server(issuer: &str, represents: Option<bool>) -> Server {
    let mut setup = SmartSetup::for_issuer(issuer);
    let entry = setup.issuer_mut();
    entry.kind = Some(IssuerKind::Patients);
    entry.act_represents_patient = represents;
    Server::start_persisting_with_smart_setup(setup).await
}

/// A patient's token under a patient-context grant, its `sub` the record
/// subject and not the client, so the profile does not read it as app-only.
fn patient_claims(issuer: &str) -> Value {
    with(
        claims(issuer, "patient/CodeSystem.cud patient/*.*", 300),
        &[("sub", json!("patient-1")), ("patient", json!("patient-1"))],
    )
}

/// That token carrying an RFC 8693 §4.1 `act` naming the person acting for
/// the patient.
fn representative_claims(issuer: &str) -> Value {
    with(
        patient_claims(issuer),
        &[("act", json!({"sub": "parent-1"}))],
    )
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

// A terminology resource sits in no patient's compartment
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>), so
// the patient and the representative are both refused the write.
#[tokio::test]
async fn a_patient_representative_is_classified_and_refused_the_write() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = patients_server(&mock.uri(), Some(true)).await;

    for (who, claims) in [
        ("the patient", patient_claims(&mock.uri())),
        ("the representative", representative_claims(&mock.uri())),
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

// ferrohealth-smart 0.14: a `patients` token naming an actor is the
// patient's representative only under `act_represents_patient`, and is
// refused as no caller of the issuer otherwise.
#[tokio::test]
async fn a_patient_token_naming_an_actor_is_refused_without_act_represents_patient() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = patients_server(&mock.uri(), None).await;

    let response = put_colours(
        &server,
        Some(&key.sign(&representative_claims(&mock.uri()))),
    )
    .await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{sent}");
    assert!(sent.contains("error=\"invalid_token\""), "{sent}");

    let response = put_colours(&server, Some(&key.sign(&patient_claims(&mock.uri())))).await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "the patient without an actor is still a caller of the issuer"
    );
}
