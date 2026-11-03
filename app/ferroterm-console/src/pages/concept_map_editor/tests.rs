// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept map editor's tests.

use super::{apply, relationship_choices};
use crate::authoring::concept_map::MapDraft;
use crate::fhir::version::FhirVersion;
use crate::posted::Posted;

#[test]
fn each_version_offers_its_own_relationship_codes() {
    assert!(
        relationship_choices(&ferrohealth_ui::i18n::Locale::default(), FhirVersion::R4)
            .iter()
            .any(|(code, _)| *code == "narrower")
    );
    assert!(
        relationship_choices(&ferrohealth_ui::i18n::Locale::default(), FhirVersion::R5)
            .iter()
            .all(|(code, _)| *code != "narrower")
    );
}

#[test]
fn a_post_rebuilds_the_groups_and_adds_the_target_asked_for() {
    let posted = Posted::parse(
        b"url=http%3A%2F%2Fx%2Fcm&status=draft&group.g0.source=http%3A%2F%2Fx%2Fa&group.g0.target=http%3A%2F%2Fx%2Fb\
          &element.e1.group=g0&element.e1.code=A&target.t2.element=e1&target.t2.code=B&target.t2.relationship=equivalent\
          &intent=add-target%3Ae1",
    );
    let mut draft = MapDraft::new();
    apply(&posted, &MapDraft::new(), &mut draft);
    assert_eq!(1, draft.groups.len());
    assert_eq!("A", draft.groups[0].elements[0].code);
    assert_eq!(2, draft.groups[0].elements[0].targets.len());
    assert_eq!(
        "equivalent",
        draft.groups[0].elements[0].targets[0].relationship
    );
}
