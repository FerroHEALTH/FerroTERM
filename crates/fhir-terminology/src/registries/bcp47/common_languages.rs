// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The common languages an expansion of all of BCP 47 returns: "Expanding all
//! language codes SHALL return the common-languages base value set, marked as
//! unclosed" (the terminology ecosystem IG at `9c2bbce`, `requirements.md`,
//! IETF Language Codes). The value set is HL7 Terminology's, vendored
//! verbatim by `scripts/vendor/tho.sh`.

use std::sync::LazyLock;

use crate::provider::error::ProviderError;

/// The canonical of the value set.
pub(super) const URL: &str = "http://terminology.hl7.org/ValueSet/Languages";

/// The vendored resource.
const RESOURCE: &str = include_str!("../../../data/tho/ValueSet-Languages.json");

/// The codes the value set enumerates, in its order.
static CODES: LazyLock<Result<Vec<String>, Unreadable>> = LazyLock::new(|| read(RESOURCE));

/// The vendored value set states no code list this module can read.
#[derive(Debug, Clone, thiserror::Error)]
#[error("the vendored common languages value set cannot be read: {0}")]
pub(super) struct Unreadable(String);

/// The `urn:ietf:bcp:47` codes `text`'s `compose.include` enumerates.
fn read(text: &str) -> Result<Vec<String>, Unreadable> {
    let resource: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Unreadable(e.to_string()))?;
    let includes = resource
        .pointer("/compose/include")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Unreadable(String::from("no compose.include")))?;
    let mut codes = Vec::new();
    for include in includes {
        if include.get("system").and_then(serde_json::Value::as_str) != Some(super::URL) {
            return Err(Unreadable(String::from(
                "an include names a system other than BCP 47",
            )));
        }
        for concept in include
            .get("concept")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let code = concept
                .get("code")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| Unreadable(String::from("a concept with no code")))?;
            codes.push(code.to_owned());
        }
    }
    Ok(codes)
}

/// The common language codes, in the value set's order.
///
/// # Errors
///
/// Returns [`ProviderError::Storage`] when the vendored value set cannot be read.
pub(super) fn codes() -> Result<&'static [String], ProviderError> {
    match &*CODES {
        Ok(codes) => Ok(codes),
        Err(unreadable) => Err(ProviderError::Storage(Box::new(unreadable.clone()))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_vendored_value_set_lists_its_codes_in_order() {
        let codes = super::codes().expect("the vendored value set reads");
        assert_eq!(codes.len(), 485);
        assert_eq!(codes.first().map(String::as_str), Some("aa"));
    }
}
