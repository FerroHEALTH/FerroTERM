// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the `$lookup` properties of a concept say beside their values.

use super::SnomedProvider;
use crate::provider::{Property, PropertyValue, error::ProviderError};
use crate::stored::storage;

/// Adds `property` unless `out` holds the same code and value.
///
/// A value a concept carries in more than one role group is one value of its
/// property, so it is listed once; no specification governs this: our own
/// design.
pub(super) fn push_once(out: &mut Vec<Property>, property: Property) {
    if !out
        .iter()
        .any(|held| held.code == property.code && held.value == property.value)
    {
        out.push(property);
    }
}

/// Each property whose value is a concept of the edition gets that concept's
/// display as its `description`, "Human Readable representation of the
/// property value (e.g. display for a code)"
/// (`OperationDefinition/CodeSystem-lookup`, `property.description`).
pub(super) fn describe(
    provider: &SnomedProvider,
    out: &mut [Property],
) -> Result<(), ProviderError> {
    for property in out.iter_mut() {
        let PropertyValue::Code(code) = &property.value else {
            continue;
        };
        let Some(ordinal) = provider.stored().store.ordinal(code).map_err(storage)? else {
            continue;
        };
        property.description = provider.choose_display(ordinal, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::push_once;
    use crate::provider::{Property, PropertyValue};

    #[test]
    fn a_value_repeated_in_another_role_group_is_listed_once() {
        let site = |code: &str| Property {
            code: String::from("363698007"),
            value: PropertyValue::Code(code.to_owned()),
            ..Property::default()
        };
        let mut out = Vec::new();
        push_once(&mut out, site("10200004"));
        push_once(&mut out, site("10200004"));
        push_once(&mut out, site("64033007"));
        assert_eq!(out, [site("10200004"), site("64033007")]);
    }
}
