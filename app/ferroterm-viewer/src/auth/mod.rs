// SPDX-License-Identifier: BUSL-1.1
//! Who is signed in, as the page knows it.
//!
//! The console signs a reader in on its server, at an OpenID Provider, and
//! keeps the session there (`crate::session`, `crate::oidc`): the browser
//! holds one opaque `HttpOnly` cookie and never a token. What the page is
//! given is an [`Operator`]: the reader's name and the scopes their token
//! grants, so a control the server would refuse is never drawn. The server
//! enforces the same scopes itself on every write.

pub(crate) mod scopes;

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::scopes::Letter;
use crate::locale::{Msg, tr};

/// The signed-in reader, as the page is told of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operator {
    /// Their name, as the provider gave it.
    pub name: Option<String>,
    /// The scopes their access token grants.
    pub scopes: Vec<String>,
}

/// The signed-in reader, for every screen to consult.
///
/// It is a context newtype so the type is unambiguous, and a signal because a
/// write the server refuses with `401` ends what the page believes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Session(pub(crate) RwSignal<Option<Operator>>);

impl Session {
    /// A session for `operator`, the reader the server rendered the page for.
    pub(crate) fn new(operator: Option<Operator>) -> Self {
        Self(RwSignal::new(operator))
    }

    /// Whether a reader is signed in, read reactively.
    pub(crate) fn signed_in(&self) -> bool {
        self.0.with(Option::is_some)
    }

    /// The name of whoever is signed in, read reactively.
    pub(crate) fn who(&self) -> Option<String> {
        self.0
            .with(|held| held.as_ref().and_then(|operator| operator.name.clone()))
    }

    /// Whether the reader's token opens `letter` on `resource_type`.
    ///
    /// This is what decides whether a control is drawn. The server enforces
    /// the same scopes itself, so a reader who gets past this is still refused
    /// there; the point is not to offer what would be refused.
    pub(crate) fn can(&self, resource_type: &str, letter: Letter) -> bool {
        self.0.with(|held| {
            held.as_ref()
                .is_some_and(|operator| scopes::can_any(&operator.scopes, resource_type, letter))
        })
    }

    /// What the reader may change, as the line under their name says it,
    /// read reactively.
    pub(crate) fn standing(&self) -> String {
        self.0.with(|held| {
            held.as_ref()
                .map_or_else(read_only, |operator| standing(&operator.scopes))
        })
    }

    /// Forgets the reader, which a `401` from the server means: their session
    /// on the console's server has ended, and they sign in again.
    pub(crate) fn release(&self) {
        self.0.set(None);
    }
}

/// The line under the name of a reader whose scopes open no write.
fn read_only() -> String {
    tr(Msg::ReadOnlyAccountHasNoEditing)
}

/// What `granted` lets a reader change, as one line under their name: the
/// three resource types the console writes, each one a create or an update
/// opens (SMART App Launch 2.x, scopes and launch context).
pub(crate) fn standing(granted: &[String]) -> String {
    let edits = |resource_type: &str| {
        scopes::can_any(granted, resource_type, Letter::Create)
            || scopes::can_any(granted, resource_type, Letter::Update)
    };
    match (edits("CodeSystem"), edits("ValueSet"), edits("ConceptMap")) {
        (true, true, true) => tr(Msg::MayEditCodesystemValuesetConceptmap),
        (true, true, false) => tr(Msg::MayEditCodesystemValueset),
        (true, false, true) => tr(Msg::MayEditCodesystemConceptmap),
        (false, true, true) => tr(Msg::MayEditValuesetConceptmap),
        (true, false, false) => tr(Msg::MayEditCodesystem),
        (false, true, false) => tr(Msg::MayEditValueset),
        (false, false, true) => tr(Msg::MayEditConceptmap),
        (false, false, false) => read_only(),
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::{Operator, Session, read_only, standing};
    use crate::auth::scopes::Letter;

    #[test]
    fn the_standing_names_what_the_scopes_open_and_says_read_only_otherwise() {
        let granted = |scopes: &[&str]| scopes.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            "May edit CodeSystem, ValueSet and ConceptMap",
            standing(&granted(&[
                "user/CodeSystem.cud",
                "user/ValueSet.cud",
                "user/ConceptMap.cud"
            ]))
        );
        assert_eq!(
            "May edit ValueSet",
            standing(&granted(&["openid", "system/ValueSet.u"]))
        );
        assert_eq!(read_only(), standing(&granted(&["openid", "fhirUser"])));
        assert_eq!(read_only(), standing(&granted(&["user/CodeSystem.rs"])));
    }

    #[test]
    fn a_control_is_drawn_for_the_scopes_the_token_grants_and_no_other() {
        let owner = Owner::new();
        owner.set();
        let session = Session::new(Some(Operator {
            name: Some(String::from("Synthetic Reader")),
            scopes: vec![String::from("openid"), String::from("user/ValueSet.cud")],
        }));
        assert!(session.signed_in());
        assert_eq!(Some(String::from("Synthetic Reader")), session.who());
        assert!(session.can("ValueSet", Letter::Create));
        assert!(
            !session.can("CodeSystem", Letter::Create),
            "a type not granted"
        );
        session.release();
        assert!(!session.signed_in(), "a 401 ends what the page believes");
        assert!(!session.can("ValueSet", Letter::Create));
    }
}
