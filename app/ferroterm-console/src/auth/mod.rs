// SPDX-FileCopyrightText: Cadasto B.V.
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

use serde::{Deserialize, Serialize};

use ferrohealth_ui::i18n::Locale;

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

/// The signed-in reader of the request, for every screen to consult.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Session(Option<Operator>);

impl Session {
    /// A session for `operator`, the reader the server rendered the page for.
    pub(crate) fn new(operator: Option<Operator>) -> Self {
        Self(operator)
    }

    /// Whether a reader is signed in.
    pub(crate) fn signed_in(&self) -> bool {
        self.0.is_some()
    }

    /// The name of whoever is signed in.
    pub(crate) fn who(&self) -> Option<String> {
        self.0.as_ref().and_then(|operator| operator.name.clone())
    }

    /// Whether the reader's token opens `letter` on `resource_type`.
    ///
    /// This is what decides whether a control is drawn. The server enforces
    /// the same scopes itself, so a reader who gets past this is still refused
    /// there; the point is not to offer what would be refused.
    pub(crate) fn can(&self, resource_type: &str, letter: Letter) -> bool {
        self.0
            .as_ref()
            .is_some_and(|operator| scopes::can_any(&operator.scopes, resource_type, letter))
    }

    /// Whether the reader's token carries the product scope `scope` as it
    /// stands, such as the sync service's `__ferroterm.sync`.
    pub(crate) fn holds(&self, scope: &str) -> bool {
        self.0
            .as_ref()
            .is_some_and(|operator| operator.scopes.iter().any(|granted| granted == scope))
    }

    /// What the reader may change, as the line under their name says it.
    pub(crate) fn standing(&self, locale: &Locale) -> String {
        self.0.as_ref().map_or_else(
            || read_only(locale),
            |operator| standing(locale, &operator.scopes),
        )
    }
}

/// The line under the name of a reader whose scopes open no write.
fn read_only(locale: &Locale) -> String {
    tr(locale, Msg::ReadOnlyAccountHasNoEditing)
}

/// What `granted` lets a reader change, as one line under their name: the
/// three resource types the console writes, each one a create or an update
/// opens (SMART App Launch 2.x, scopes and launch context).
pub(crate) fn standing(locale: &Locale, granted: &[String]) -> String {
    let edits = |resource_type: &str| {
        scopes::can_any(granted, resource_type, Letter::Create)
            || scopes::can_any(granted, resource_type, Letter::Update)
    };
    match (edits("CodeSystem"), edits("ValueSet"), edits("ConceptMap")) {
        (true, true, true) => tr(locale, Msg::MayEditCodesystemValuesetConceptmap),
        (true, true, false) => tr(locale, Msg::MayEditCodesystemValueset),
        (true, false, true) => tr(locale, Msg::MayEditCodesystemConceptmap),
        (false, true, true) => tr(locale, Msg::MayEditValuesetConceptmap),
        (true, false, false) => tr(locale, Msg::MayEditCodesystem),
        (false, true, false) => tr(locale, Msg::MayEditValueset),
        (false, false, true) => tr(locale, Msg::MayEditConceptmap),
        (false, false, false) => read_only(locale),
    }
}

/// Why the sign-in page is shown, as its `state` parameter names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInState {
    /// The reader signed out of the console.
    SignedOut,
    /// The session ended after its idle timeout.
    Expired,
    /// The session ended at its absolute timeout.
    ExpiredAbsolute,
    /// The console holds as many sessions as it may.
    Full,
    /// The provider refused the sign-in.
    Refused,
    /// The provider's answer failed the console's checks.
    Rejected,
    /// The console could not reach the provider.
    Unreachable,
    /// The console offers no sign-in.
    Unavailable,
}

impl SignInState {
    /// Every state, in the order the page knows them.
    pub const ALL: [Self; 8] = [
        Self::SignedOut,
        Self::Expired,
        Self::ExpiredAbsolute,
        Self::Full,
        Self::Refused,
        Self::Rejected,
        Self::Unreachable,
        Self::Unavailable,
    ];

    /// The `state` parameter that names it.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SignedOut => "signed-out",
            Self::Expired => "expired",
            Self::ExpiredAbsolute => "expired-absolute",
            Self::Full => "full",
            Self::Refused => "refused",
            Self::Rejected => "rejected",
            Self::Unreachable => "unreachable",
            Self::Unavailable => "unavailable",
        }
    }

    /// The state `key` names, or `None` for one the page does not know.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.key() == key)
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::i18n::Locale;

    use super::{Operator, Session, read_only, standing};
    use crate::auth::scopes::Letter;

    #[test]
    fn the_standing_names_what_the_scopes_open_and_says_read_only_otherwise() {
        let granted = |scopes: &[&str]| scopes.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            "May edit CodeSystem, ValueSet and ConceptMap",
            standing(
                &Locale::english(),
                &granted(&[
                    "user/CodeSystem.cud",
                    "user/ValueSet.cud",
                    "user/ConceptMap.cud"
                ])
            )
        );
        assert_eq!(
            "May edit ValueSet",
            standing(
                &Locale::english(),
                &granted(&["openid", "system/ValueSet.u"])
            )
        );
        assert_eq!(
            read_only(&Locale::english()),
            standing(&Locale::english(), &granted(&["openid", "fhirUser"]))
        );
        assert_eq!(
            read_only(&Locale::english()),
            standing(&Locale::english(), &granted(&["user/CodeSystem.rs"]))
        );
    }

    #[test]
    fn a_control_is_drawn_for_the_scopes_the_token_grants_and_no_other() {
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
        assert!(!Session::new(None).signed_in(), "a visitor");
        assert!(!Session::new(None).can("ValueSet", Letter::Create));
    }
}
