// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ECL description, concept and member filters, and the history supplements.

use ferroterm_testkit::snomed::{ANIMAL, CAT, CODES_MAP, DOG, FISH, Item, PETS, TOP, sctid};
use sct_ecl::eval::EvalError;

use super::{c, provider, refused, same_as, set};

#[test]
#[expect(clippy::too_many_lines, reason = "one case per description filter")]
fn description_filters_match_one_description_per_concept() {
    let (_dir, p) = provider();
    let animals = c(ANIMAL);
    assert_eq!(
        set(&p, &format!("< {animals} {{{{ term = \"kat\" }}}}")),
        [CAT],
        "a Dutch synonym"
    );
    assert_eq!(
        set(&p, &format!("< {animals} {{{{ term = \"ca\" }}}}")),
        [CAT],
        "a word prefix"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = (\"kat\" \"hond\") }}}}")
        ),
        [CAT, DOG],
        "any of a set"
    );
    assert_eq!(
        set(
            &p,
            &format!("<< {animals} {{{{ term = \"dier\", language = nl }}}}")
        ),
        [ANIMAL]
    );
    assert!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", language = nl }}}}")
        )
        .is_empty(),
        "one description must match every filter"
    );
    assert_eq!(
        set(&p, &format!("< {animals} {{{{ term = wild:\"*oes\" }}}}")),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"poes\", dialect = nl-nl (accept) }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"poes\", dialect = nl-nl (prefer) }}}}")
        ),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"kat\", dialect = nl-nl (prefer) }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"kat\", dialectId = 31000146106 }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"kat\", dialect = en-gb }}}}")
        ),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", type = fsn }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", typeId = 900000000000003001 }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", type = (syn fsn) }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", type = def }}}}")
        ),
        Vec::<u32>::new()
    );
    assert!(
        set(&p, &format!("< {animals} {{{{ term = \"moggy\" }}}}")).is_empty(),
        "inactive descriptions do not match by default"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"moggy\", active = false }}}}")
        ),
        [CAT]
    );
    assert_eq!(
        set(&p, &format!("< {animals} {{{{ language = nl }}}}")),
        [CAT, DOG],
        "no term: the store is scanned"
    );
    assert_eq!(
        set(&p, &format!("< {animals} {{{{ term != \"cat\" }}}}")),
        [CAT, DOG],
        "some description is not the term"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ D term = \"kat\" }}}} {{{{ D term = \"cat\" }}}}")
        ),
        [CAT],
        "two constraints, two descriptions"
    );
    assert!(matches!(
        refused(&p, "* {{ dialect = xx-yy }}"),
        EvalError::UnknownDialect(_)
    ));
    assert!(matches!(
        refused(&p, "* {{ moduleId = 900000000000207008 }}"),
        EvalError::Unsupported(_)
    ));
}

#[test]
fn concept_filters_read_the_concept_rows() {
    let (_dir, p) = provider();
    assert_eq!(set(&p, "* {{ C active = false }}"), [FISH]);
    assert_eq!(set(&p, "* {{ C active = 0 }}"), [FISH]);
    assert_eq!(set(&p, "* {{ C active != true }}"), [FISH]);
    assert_eq!(set(&p, "* {{ C active = true }}").len(), 20);
    assert_eq!(
        set(
            &p,
            &format!("<< {} {{{{ C definitionStatus = defined }}}}", c(ANIMAL))
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!("<< {} {{{{ C definitionStatus = primitive }}}}", c(ANIMAL))
        ),
        [ANIMAL]
    );
    assert_eq!(
        set(
            &p,
            &format!("<< {} {{{{ C definitionStatus != primitive }}}}", c(ANIMAL))
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "<< {} {{{{ C definitionStatusId = 900000000000073002 }}}}",
                c(ANIMAL)
            )
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "<< {} {{{{ C definitionStatusId = (900000000000073002 900000000000074008) }}}}",
                c(ANIMAL)
            )
        ),
        [ANIMAL, CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "<< {} {{{{ C moduleId = {} }}}}",
                c(ANIMAL),
                sctid(Item::raw(99))
            )
        ),
        [ANIMAL, CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "<< {} {{{{ C moduleId != {} }}}}",
                c(ANIMAL),
                sctid(Item::raw(99))
            )
        ),
        Vec::<u32>::new()
    );
    assert_eq!(set(&p, "* {{ C effectiveTime = \"20260101\" }}").len(), 21);
    assert_eq!(
        set(&p, "* {{ C effectiveTime >= \"20270101\" }}"),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, "* {{ C effectiveTime != (\"20250101\" \"20260101\") }}").len(),
        0
    );
    assert_eq!(
        set(&p, "* {{ C definitionStatus = defined, active = true }}"),
        [CAT, DOG]
    );
}

#[test]
fn member_filters_read_the_fields_of_the_rows() {
    let (_dir, p) = provider();
    let map = c(CODES_MAP);
    assert_eq!(
        set(&p, &format!("^ {map} {{{{ M mapTarget = \"C01\" }}}}")),
        [CAT]
    );
    assert_eq!(
        set(&p, &format!("^ {map} {{{{ M mapTarget = wild:\"D*\" }}}}")),
        [DOG]
    );
    assert_eq!(
        set(&p, &format!("^ {map} {{{{ M mapTarget != \"C01\" }}}}")),
        [DOG]
    );
    assert_eq!(
        set(&p, &format!("^ {map} {{{{ M mapGroup = #1 }}}}")),
        [CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("^ {map} {{{{ M mapGroup > #1 }}}}")),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(
            &p,
            &format!("^ {map} {{{{ M mapGroup = #1, mapTarget = \"D01\" }}}}")
        ),
        [DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!("^ {map} {{{{ M effectiveTime = \"20260101\" }}}}")
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!("^ {map} {{{{ M effectiveTime < \"20260101\" }}}}")
        ),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(
            &p,
            &format!("^ {map} {{{{ M moduleId = {} }}}}", sctid(Item::raw(99)))
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("^ {map} {{{{ M active = 1 }}}}")),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!("^ {} {{{{ M targetComponentId = {} }}}}", same_as(), c(CAT))
        ),
        [FISH]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "^ {} {{{{ M targetComponentId = << {} }}}}",
                same_as(),
                c(ANIMAL)
            )
        ),
        [FISH]
    );
    assert_eq!(
        set(
            &p,
            &format!("^ {} {{{{ M targetComponentId = {} }}}}", same_as(), c(DOG))
        ),
        Vec::<u32>::new()
    );
    assert!(
        set(&p, &format!("^ {map} {{{{ M nosuch = \"x\" }}}}")).is_empty(),
        "an unknown field matches no row"
    );
    assert!(
        matches!(
            refused(&p, &format!("^ {map} {{{{ M active = 0 }}}}")),
            EvalError::Unsupported(_)
        ),
        "the tables hold active members only"
    );
}

/// ECL 2.3, the active filter's wildcard: `active = *` admits either status
/// and `active != *` neither, in each of the three filters.
#[test]
fn the_active_wildcard_admits_either_status() {
    let (_dir, p) = provider();
    let animals = c(ANIMAL);
    assert_eq!(
        set(&p, "* {{ C active = * }}"),
        set(&p, "*"),
        "a concept filter on either status keeps every concept"
    );
    assert_eq!(
        set(&p, "* {{ C active != * }}"),
        Vec::<u32>::new(),
        "no concept has neither status"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"moggy\", active = * }}}}")
        ),
        [CAT],
        "an inactive description matches under either status"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", active = * }}}}")
        ),
        [CAT],
        "an active one too"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} {{{{ term = \"cat\", active != * }}}}")
        ),
        Vec::<u32>::new(),
        "no description has neither status"
    );
    assert_eq!(
        set(&p, &format!("^ {} {{{{ M active != * }}}}", c(PETS))),
        Vec::<u32>::new(),
        "no member has neither status"
    );
    assert!(
        matches!(
            refused(&p, &format!("^ {} {{{{ M active = * }}}}", c(PETS))),
            EvalError::Unsupported(_)
        ),
        "either status needs the inactive members the tables do not hold"
    );
}

#[test]
fn history_supplements_add_the_inactive_concepts_associated_to_the_set() {
    let (_dir, p) = provider();
    // ECL, "History supplements": the members of the association reference
    // sets whose targetComponentId is in the set are added.
    assert_eq!(
        set(&p, &format!("<< {} {{{{ + HISTORY }}}}", c(CAT))),
        [CAT, FISH]
    );
    assert_eq!(
        set(&p, &format!("<< {} {{{{ + HISTORY-MIN }}}}", c(CAT))),
        [CAT, FISH]
    );
    assert_eq!(
        set(&p, &format!("<< {} {{{{ + HISTORY-MOD }}}}", c(CAT))),
        [CAT, FISH]
    );
    assert_eq!(
        set(&p, &format!("<< {} {{{{ + HISTORY-MAX }}}}", c(CAT))),
        [CAT, FISH]
    );
    assert_eq!(
        set(
            &p,
            &format!("<< {} {{{{ + HISTORY ( {} ) }}}}", c(CAT), same_as())
        ),
        [CAT, FISH]
    );
    assert_eq!(
        set(
            &p,
            &format!("<< {} {{{{ + HISTORY ( {} ) }}}}", c(CAT), c(PETS))
        ),
        [CAT],
        "a set without a target field adds nothing"
    );
    assert_eq!(
        set(&p, &format!("<< {} {{{{ + HISTORY }}}}", c(DOG))),
        [DOG, FISH],
        "the fish is REPLACED BY the dog, so the dog's history holds it"
    );
    assert_eq!(
        set(&p, &format!("< {} {{{{ + HISTORY }}}}", c(ANIMAL))),
        [CAT, DOG, FISH]
    );
}

#[test]
fn alternate_identifiers_resolve_through_the_scheme_alias() {
    let (_dir, p) = provider();
    assert_eq!(set(&p, "ZOO#cat-1"), [CAT]);
    assert_eq!(
        set(&p, "zoo#dog-1"),
        [DOG],
        "the alias is compared without case"
    );
    assert_eq!(set(&p, ">> ZOO#cat-1"), [TOP, ANIMAL, CAT]);
    assert_eq!(set(&p, "\"ZOO#cat-1\" |the cat|"), [CAT]);
    assert!(matches!(
        refused(&p, "ZOO#cat-9"),
        EvalError::UnknownIdentifier { code, .. } if code == "cat-9"
    ));
    assert!(matches!(
        refused(&p, "NOPE#cat-1"),
        EvalError::UnknownScheme(_)
    ));
}
