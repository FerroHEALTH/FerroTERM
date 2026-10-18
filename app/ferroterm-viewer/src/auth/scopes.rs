//! What the scopes a token carries let the reader do.
//!
//! The grammar is `<compartment>/<resource>.<permissions>`, the permissions a
//! subset of `cruds` taken in order, with the version 1 forms `.read`,
//! `.write`, and `.*` mapping to `.rs`, `.cud`, and `.cruds`
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
//!
//! The server enforces every one of these itself; this reads them so a control
//! the server would refuse is never drawn. A shown control the server refuses
//! is a worse answer than no control, and a hidden one the server would have
//! allowed is a viewer that lies about the reader's rights.

/// The compartment an interactive reader's grant sits in.
///
/// A person signing in through the browser is the `user` compartment;
/// `system` is a client authorized in its own right, which the viewer never
/// is. Both are read, because a deployment may mint either for the same
/// person.
pub(crate) const USER: &str = "user";

/// The compartment a client authorized in its own right sits in.
pub(crate) const SYSTEM: &str = "system";

/// The compartments a granted scope may open a control under.
const COMPARTMENTS: [&str; 2] = [USER, SYSTEM];

/// The permission letters, in the order the specification fixes them.
const CRUDS: &str = "cruds";

/// The FHIR interaction a control performs, as a SMART permission letter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Letter {
    /// Type-level create, the `c` letter.
    Create,
    /// Instance-level update, the `u` letter.
    Update,
    /// Instance-level delete, the `d` letter.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "no screen deletes a resource, as a published code is retired in place; the tests read the letter"
        )
    )]
    Delete,
}

impl Letter {
    /// The version 2 permission letter.
    pub(crate) const fn letter(self) -> char {
        match self {
            Self::Create => 'c',
            Self::Update => 'u',
            Self::Delete => 'd',
        }
    }
}

/// Whether `granted` lets the reader perform `letter` on `resource_type`.
///
/// `compartment` is the one the control belongs to, so a caller asks the
/// question it means rather than trusting whichever compartment the token
/// happens to carry.
pub(crate) fn can(
    granted: &[String],
    compartment: &str,
    resource_type: &str,
    letter: Letter,
) -> bool {
    granted
        .iter()
        .any(|scope| grants(scope, compartment, resource_type, letter))
}

/// Whether the reader may perform `letter` on `resource_type` in any
/// compartment the viewer honours.
pub(crate) fn can_any(granted: &[String], resource_type: &str, letter: Letter) -> bool {
    COMPARTMENTS
        .iter()
        .any(|compartment| can(granted, compartment, resource_type, letter))
}

/// Whether one granted scope string permits `letter` on `resource_type`.
///
/// A scope narrowed by search parameters (`user/CodeSystem.cud?url=…`) opens
/// nothing: the viewer cannot evaluate the restriction, and the server refuses
/// such a scope outright, so drawing a control for it would invite a refusal
/// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>,
/// §Finer-grained resource constraints using search parameters).
fn grants(granted: &str, compartment: &str, resource_type: &str, letter: Letter) -> bool {
    let Some((held, rest)) = granted.trim().split_once('/') else {
        return false;
    };
    // NOTE: `patient/` narrows a grant to one patient's compartment, and a
    // terminology server holds no patient record, so the scope selects nothing
    // (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
    if held != compartment || !COMPARTMENTS.contains(&held) || rest.contains('?') {
        return false;
    }
    let Some((requested, permissions)) = rest.rsplit_once('.') else {
        return false;
    };
    if requested != "*" && requested != resource_type {
        return false;
    }
    permits(permissions, letter)
}

/// Whether the permission part of a scope covers `letter`.
fn permits(permissions: &str, letter: Letter) -> bool {
    match permissions {
        // The version 1 forms, with the mapping the specification states:
        // `.write` is `.cud` and `.*` is `.cruds`, and `.read` is `.rs`, which
        // carries none of the three letters asked about here.
        "*" | "write" => true,
        "read" => false,
        letters => in_order(letters) && letters.contains(letter.letter()),
    }
}

/// Whether `letters` is a subset of `cruds` taken in order.
///
/// The specification fixes the order, so `.sr` names no permission.
fn in_order(letters: &str) -> bool {
    if letters.is_empty() {
        return false;
    }
    let mut remaining = CRUDS.chars();
    letters
        .chars()
        .all(|letter| remaining.any(|known| known == letter))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resource types the server's write routes cover.
    const WRITTEN: [&str; 3] = ["CodeSystem", "ValueSet", "ConceptMap"];

    /// The three write permissions.
    const WRITES: [Letter; 3] = [Letter::Create, Letter::Update, Letter::Delete];

    /// The scopes a token grants, from the `scope` member of the token
    /// response: RFC 6749 §3.3 makes it a space-delimited string.
    fn granted(scope: Option<&str>) -> Vec<String> {
        scope
            .map(|text| text.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// The scopes of `text`, as a token response carries them.
    fn scopes(text: &str) -> Vec<String> {
        granted(Some(text))
    }

    #[test]
    fn a_version_two_scope_opens_the_letter_it_carries() {
        for (scope, letter, expected) in [
            ("user/CodeSystem.c", Letter::Create, true),
            ("user/CodeSystem.c", Letter::Update, false),
            ("user/CodeSystem.cud", Letter::Delete, true),
            ("user/CodeSystem.cruds", Letter::Update, true),
            ("user/CodeSystem.rs", Letter::Create, false),
            ("user/*.cud", Letter::Create, true),
            ("user/ValueSet.cud", Letter::Create, false),
            ("user/CodeSystem.sr", Letter::Create, false),
            ("user/CodeSystem.", Letter::Create, false),
            ("user/CodeSystem", Letter::Create, false),
            ("CodeSystem.cud", Letter::Create, false),
            ("openid", Letter::Create, false),
        ] {
            assert_eq!(
                can(&scopes(scope), USER, "CodeSystem", letter),
                expected,
                "{scope} for {letter:?}"
            );
        }
    }

    #[test]
    fn the_version_one_forms_map_to_the_version_two_letters() {
        assert!(can(
            &scopes("user/CodeSystem.write"),
            USER,
            "CodeSystem",
            Letter::Update
        ));
        assert!(can(
            &scopes("user/CodeSystem.*"),
            USER,
            "CodeSystem",
            Letter::Delete
        ));
        assert!(!can(
            &scopes("user/CodeSystem.read"),
            USER,
            "CodeSystem",
            Letter::Create
        ));
        assert!(can(
            &scopes("user/*.write"),
            USER,
            "ConceptMap",
            Letter::Create
        ));
    }

    #[test]
    fn every_written_type_and_letter_reads_the_same_way() {
        for resource_type in WRITTEN {
            for letter in WRITES {
                let held = scopes(&format!("user/{resource_type}.cud openid fhirUser"));
                assert!(
                    can(&held, USER, resource_type, letter),
                    "user/{resource_type}.cud for {letter:?}"
                );
                assert!(
                    !can(&held, SYSTEM, resource_type, letter),
                    "a user grant is not a system grant: {letter:?}"
                );
                assert!(
                    can_any(&held, resource_type, letter),
                    "the control asks either compartment"
                );
            }
        }
    }

    #[test]
    fn a_patient_scope_opens_nothing_on_a_terminology_server() {
        for resource_type in WRITTEN {
            for letter in WRITES {
                for scope in [
                    format!("patient/{resource_type}.cud"),
                    format!("patient/{resource_type}.write"),
                    String::from("patient/*.*"),
                ] {
                    assert!(
                        !can_any(&scopes(&scope), resource_type, letter),
                        "{scope} for {letter:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_scope_narrowed_by_a_search_parameter_opens_nothing() {
        // The server refuses it, so a control drawn for it would invite a
        // refusal the reader cannot act on.
        let held = scopes("user/CodeSystem.cud?url=http://terminology.example/x");
        assert!(!can_any(&held, "CodeSystem", Letter::Create));
    }

    #[test]
    fn a_reader_with_no_write_scope_sees_no_control() {
        let held = scopes("openid fhirUser user/CodeSystem.rs");
        for resource_type in WRITTEN {
            for letter in WRITES {
                assert!(
                    !can_any(&held, resource_type, letter),
                    "{resource_type} for {letter:?}"
                );
            }
        }
    }

    #[test]
    fn the_permission_letters_are_a_subset_of_cruds_in_order() {
        for letters in ["c", "cu", "cud", "cruds", "rs", "ds"] {
            assert!(in_order(letters), "{letters}");
        }
        for letters in ["", "sr", "dc", "cc", "x", "cudx"] {
            assert!(!in_order(letters), "{letters}");
        }
    }

    #[test]
    fn a_token_that_granted_no_scope_reads_as_an_empty_grant() {
        assert_eq!(granted(None), Vec::<String>::new());
        assert_eq!(
            granted(Some("  user/ValueSet.cud   openid ")),
            vec![String::from("user/ValueSet.cud"), String::from("openid")],
            "RFC 6749 section 3.3 delimits the grant with spaces"
        );
    }
}
