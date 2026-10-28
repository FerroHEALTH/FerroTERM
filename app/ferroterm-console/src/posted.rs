// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A form the reader posted to a page: a restore, or an editor's draft.
//!
//! A page that writes is a plain HTML form that posts to the page's own
//! address, so it works with no WebAssembly. The server reads the posted
//! fields and draws the page again with them in its context; the page sends
//! the write and either redirects to what it wrote or draws the refusal over
//! the draft the reader keeps. No specification governs this: our own
//! design.

#[cfg(not(target_arch = "wasm32"))]
/// The most bytes a posted form may carry: a code system's draft with every
/// concept fits well under it.
pub(crate) const LIMIT: usize = 4 * 1024 * 1024;

/// The field every write form carries naming what it asks for.
pub(crate) const INTENT: &str = "intent";

/// The fields of a posted form, in the order the browser sent them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Posted(Vec<(String, String)>);

impl Posted {
    #[cfg(not(target_arch = "wasm32"))]
    /// The fields of the `application/x-www-form-urlencoded` body `body`.
    #[must_use]
    pub(crate) fn parse(body: &[u8]) -> Self {
        Self(
            url::form_urlencoded::parse(body)
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect(),
        )
    }

    /// The first value of the field `name`, if the form carries it.
    #[must_use]
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The first value of the field `name`, trimmed, empty when the form
    /// carries none.
    #[must_use]
    pub(crate) fn text(&self, name: &str) -> String {
        self.get(name).unwrap_or_default().trim().to_owned()
    }

    /// Every field, in the order the browser sent them.
    pub(crate) fn pairs(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter().map(|(key, value)| (key, value))
    }

    /// What the form asks for: its `intent` field.
    #[must_use]
    pub(crate) fn intent(&self) -> &str {
        self.get(INTENT).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::Posted;

    #[test]
    fn a_form_keeps_every_field_in_order() {
        let posted = Posted::parse(b"intent=restore&code=a&code=b+c&empty=");
        assert_eq!("restore", posted.intent());
        assert_eq!(Some("a"), posted.get("code"));
        assert_eq!(Some(""), posted.get("empty"));
        assert_eq!(None, posted.get("missing"));
        assert_eq!("", Posted::default().intent());
        let values: Vec<&str> = posted
            .pairs()
            .filter(|(key, _)| *key == "code")
            .map(|(_, value)| value.as_str())
            .collect();
        assert_eq!(vec!["a", "b c"], values);
    }
}
