// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ECL evaluator over the synthetic edition: one case per construct, with
//! the expected set from the ECL specification's definitions
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>,
//! Appendix D, the quick reference, unless a test cites another section).

use ferroterm_testkit::snomed::{
    ANIMAL, CAT, CODES_MAP, COVERING, DOG, FISH, FUR, GB_LANGUAGE_REFSET, HISTORICAL, ICD10_MAP,
    Item, LEGS, MODULE_CONCEPT, MODULE_DEPENDENCY, NL_LANGUAGE_REFSET, PETS, SAME_AS_SCTID, SCHEME,
    TOP, item, sctid,
};
use fhir_terminology::snomed::SnomedProvider;
use sct_ecl::eval::{EvalError, evaluate};

fn provider() -> (tempfile::TempDir, SnomedProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    (dir, provider)
}

/// The code of a fixture concept.
fn c(ordinal: u32) -> String {
    sctid(item(ordinal))
}

/// The published code of the SAME AS association reference set.
fn same_as() -> &'static str {
    SAME_AS_SCTID
}

fn set(provider: &SnomedProvider, text: &str) -> Vec<u32> {
    let tree = sct_ecl::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    evaluate(provider, &tree)
        .unwrap_or_else(|e| panic!("{text}: {e}"))
        .iter()
        .collect()
}

fn refused(provider: &SnomedProvider, text: &str) -> EvalError {
    let tree = sct_ecl::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    evaluate(provider, &tree).expect_err("refused")
}

#[test]
fn the_hierarchy_operators_read_the_closure_and_the_adjacency() {
    let (_dir, p) = provider();
    assert_eq!(set(&p, &c(CAT)), [CAT], "self");
    assert_eq!(
        set(&p, &format!("< {}", c(ANIMAL))),
        [CAT, DOG],
        "descendantOf"
    );
    assert_eq!(
        set(&p, &format!("<< {}", c(ANIMAL))),
        [ANIMAL, CAT, DOG],
        "descendantOrSelfOf"
    );
    assert_eq!(
        set(&p, &format!("<! {}", c(TOP))),
        [
            ANIMAL,
            FUR,
            COVERING,
            LEGS,
            PETS,
            CODES_MAP,
            HISTORICAL,
            SCHEME,
            MODULE_DEPENDENCY,
            MODULE_CONCEPT,
            ICD10_MAP,
            GB_LANGUAGE_REFSET,
            NL_LANGUAGE_REFSET
        ],
        "childOf"
    );
    assert_eq!(
        set(&p, &format!("<<! {}", c(ANIMAL))),
        [ANIMAL, CAT, DOG],
        "childOrSelfOf"
    );
    assert_eq!(
        set(&p, &format!("> {}", c(CAT))),
        [TOP, ANIMAL],
        "ancestorOf"
    );
    assert_eq!(
        set(&p, &format!(">> {}", c(CAT))),
        [TOP, ANIMAL, CAT],
        "ancestorOrSelfOf"
    );
    assert_eq!(set(&p, &format!(">! {}", c(CAT))), [ANIMAL], "parentOf");
    assert_eq!(
        set(&p, &format!(">>! {}", c(CAT))),
        [ANIMAL, CAT],
        "parentOrSelfOf"
    );
    assert_eq!(
        set(&p, &format!("!!> ( << {} )", c(ANIMAL))),
        [ANIMAL],
        "top: no ancestor in the set"
    );
    assert_eq!(
        set(&p, &format!("!!< ( << {} )", c(ANIMAL))),
        [CAT, DOG],
        "bottom: no descendant in the set"
    );
    assert_eq!(set(&p, "*").len(), 21, "every concept, active and inactive");
    assert_eq!(
        set(&p, "< *").len(),
        19,
        "everything under a root; the top and the fish are roots"
    );
    assert_eq!(set(&p, "!!> *"), [TOP, FISH], "the roots");
    assert_eq!(
        set(&p, "!!< *").len(),
        18,
        "the leaves: the fish has no child either"
    );
    assert_eq!(set(&p, "<< *").len(), 21);
}

#[test]
fn member_of_reads_the_tables_and_selects_fields() {
    let (_dir, p) = provider();
    assert_eq!(set(&p, &format!("^ {}", c(PETS))), [CAT, DOG]);
    assert_eq!(
        set(&p, &format!("^ {}", same_as())),
        [FISH],
        "the referenced components"
    );
    assert_eq!(
        set(&p, &format!("^ [targetComponentId] {}", same_as())),
        [CAT],
        "a field selection returns that field's values"
    );
    assert_eq!(
        set(
            &p,
            &format!("^ [referencedComponentId, targetComponentId] {}", same_as())
        ),
        [CAT, FISH]
    );
    assert_eq!(set(&p, &format!("^ [*] {}", same_as())), [CAT, FISH]);
    assert!(
        set(&p, &format!("^ [mapTarget] {}", c(CODES_MAP))).is_empty(),
        "a string field yields no concepts"
    );
    assert_eq!(
        set(&p, &format!("< ^ {}", c(PETS))).len(),
        0,
        "descendants of the members"
    );
    assert_eq!(
        set(&p, &format!(">> ^ {}", c(PETS))),
        [TOP, ANIMAL, CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("^ ( {} OR {} )", c(PETS), same_as())),
        [CAT, DOG, FISH],
        "a nested constraint names the reference sets"
    );
    assert!(matches!(
        refused(&p, &format!("^ {}", c(FUR))),
        EvalError::NotAReferenceSet(_)
    ));
    assert!(matches!(
        refused(&p, &format!("^ [nope] {}", same_as())),
        EvalError::UnknownField { field, .. } if field == "nope"
    ));
}

#[test]
fn compound_constraints_are_set_algebra() {
    let (_dir, p) = provider();
    assert_eq!(
        set(&p, &format!("< {} AND ^ {}", c(ANIMAL), c(PETS))),
        [CAT, DOG]
    );
    assert_eq!(set(&p, &format!("{} OR {}", c(CAT), c(FUR))), [CAT, FUR]);
    assert_eq!(set(&p, &format!("< {} MINUS {}", c(ANIMAL), c(CAT))), [DOG]);
    assert!(
        set(&p, &format!("({} , {}) AND {}", c(CAT), c(DOG), c(CAT))).is_empty(),
        "a comma is a conjunction"
    );
    assert_eq!(
        set(&p, &format!("({} OR {}) AND {}", c(CAT), c(DOG), c(CAT))),
        [CAT]
    );
}

#[test]
#[expect(clippy::too_many_lines, reason = "one case per cardinality form")]
fn refinements_match_attribute_rows_with_their_cardinalities() {
    let (_dir, p) = provider();
    let animals = c(ANIMAL);
    assert_eq!(
        set(&p, &format!("< {animals} : {} = {}", c(COVERING), c(FUR))),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} : {} = << {}", c(COVERING), c(TOP))
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {} = {}", c(COVERING), c(CAT))),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, &format!("< {animals} : * = {}", c(FUR))),
        [CAT, DOG],
        "any attribute"
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {} = *", c(COVERING))),
        [CAT, DOG],
        "any value"
    );
    assert!(
        set(&p, &format!("< {animals} : {} != {}", c(COVERING), c(FUR))).is_empty(),
        "no other value"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} : [0..0] {} = {}", c(COVERING), c(CAT))
        ),
        [CAT, DOG],
        "zero occurrences"
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} : [0..0] {} = {}", c(COVERING), c(FUR))
        ),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, &format!("< {animals} : [2..*] * = *")),
        [CAT, DOG],
        "two attributes each"
    );
    assert_eq!(
        set(&p, &format!("< {animals} : [3..*] * = *")),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, &format!("<< {} : [0..0] {} = *", c(TOP), c(COVERING))).len(),
        18,
        "everything under the top without a covering"
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {} = #4", c(LEGS))),
        [CAT, DOG],
        "a concrete number"
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {} > #3", c(LEGS))),
        [CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {} >= #5", c(LEGS))),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {} != #5", c(LEGS))),
        [CAT, DOG]
    );
    assert!(
        set(&p, &format!("< {animals} : {} = \"four\"", c(LEGS))).is_empty(),
        "a number is not a string"
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "< {animals} : {} = {} AND {} = #4",
                c(COVERING),
                c(FUR),
                c(LEGS)
            )
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "< {animals} : {} = {} OR {} = #9",
                c(COVERING),
                c(FUR),
                c(LEGS)
            )
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "< {animals} : ( {} = {} ) , {} = #4",
                c(COVERING),
                c(FUR),
                c(LEGS)
            )
        ),
        [CAT, DOG],
        "a nested attribute set"
    );
}

#[test]
fn attribute_groups_hold_within_one_role_group() {
    let (_dir, p) = provider();
    let animals = c(ANIMAL);
    // ECL 2.2, "Attribute groups": the attributes of a group must be in the
    // same relationship group; the cat states both in group 1, the dog splits
    // them over two groups.
    assert_eq!(
        set(
            &p,
            &format!(
                "< {animals} : {{ {} = {}, {} = #4 }}",
                c(COVERING),
                c(FUR),
                c(LEGS)
            )
        ),
        [CAT]
    );
    assert_eq!(
        set(
            &p,
            &format!("< {animals} : {{ {} = {} }}", c(COVERING), c(FUR))
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("< {animals} : [1..1] {{ * = * }}")),
        [CAT],
        "one group"
    );
    assert_eq!(
        set(&p, &format!("< {animals} : [2..2] {{ * = * }}")),
        [DOG],
        "two groups"
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "< {animals} : [0..0] {{ {} = #4, {} = {} }}",
                c(LEGS),
                c(COVERING),
                c(FUR)
            )
        ),
        [DOG]
    );
    assert_eq!(
        set(
            &p,
            &format!(
                "< {animals} : {{ {} = {} }} OR {{ {} = #4 }}",
                c(COVERING),
                c(FUR),
                c(LEGS)
            )
        ),
        [CAT, DOG]
    );
    assert_eq!(
        set(&p, &format!("< {animals} : {{ [2..*] * = * }}")),
        [CAT],
        "attribute cardinality inside a group counts that group's rows"
    );
}

#[test]
fn reverse_and_dotted_attributes_walk_to_the_values() {
    let (_dir, p) = provider();
    assert_eq!(
        set(&p, &format!("* : R {} = {}", c(COVERING), c(CAT))),
        [FUR],
        "the reverse flag"
    );
    assert_eq!(
        set(&p, &format!("* : R {} = < {}", c(COVERING), c(ANIMAL))),
        [FUR]
    );
    assert_eq!(
        set(&p, &format!("* : [2..2] R {} = *", c(COVERING))),
        [FUR],
        "two sources"
    );
    assert_eq!(
        set(&p, &format!("* : [3..*] R {} = *", c(COVERING))),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, &format!("< {} . {}", c(ANIMAL), c(COVERING))),
        [FUR],
        "dotted"
    );
    assert_eq!(
        set(&p, &format!("{} . *", c(CAT))),
        [FUR],
        "numbers are not concepts"
    );
    assert_eq!(
        set(&p, &format!("{} . {}", c(FUR), c(COVERING))),
        Vec::<u32>::new()
    );
    assert_eq!(
        set(&p, &format!("( < {} . {} ) . *", c(ANIMAL), c(COVERING))).len(),
        0,
        "the fur has no attributes"
    );
}

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

#[test]
fn an_unknown_concept_is_a_typed_error_not_an_empty_set() {
    let (_dir, p) = provider();
    assert!(matches!(
        refused(&p, "<< 999999999"),
        EvalError::UnknownConcept(_)
    ));
    assert!(matches!(
        refused(&p, &format!("< {} : 999999999 = *", c(ANIMAL))),
        EvalError::UnknownConcept(_)
    ));
    assert!(matches!(
        refused(
            &p,
            &format!("< {} : {} = 999999999", c(ANIMAL), c(COVERING))
        ),
        EvalError::UnknownConcept(_)
    ));
}

mod invariants {
    //! The set-algebra invariants of ECL over the synthetic edition
    //! (`proptest`): `<< X` holds `X`, `< X` and `> X` are inverse, a
    //! conjunction is within each operand, an exclusion is disjoint from what
    //! it removes.

    use ferroterm_testkit::snomed::{
        ANIMAL, CAT, CODES_MAP, COVERING, DOG, FISH, FUR, LEGS, PETS, TOP, item, sctid,
    };
    use fhir_terminology::snomed::SnomedProvider;
    use proptest::prelude::*;
    use roaring::RoaringBitmap;
    use sct_ecl::eval::evaluate;

    fn eval(provider: &SnomedProvider, text: &str) -> RoaringBitmap {
        let tree = sct_ecl::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        evaluate(provider, &tree).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn concept() -> impl Strategy<Value = u32> {
        prop::sample::select(vec![
            TOP, ANIMAL, CAT, DOG, FISH, FUR, COVERING, LEGS, PETS, CODES_MAP,
        ])
    }

    fn operand() -> impl Strategy<Value = String> {
        (
            concept(),
            prop::sample::select(vec!["", "< ", "<< ", "> ", ">> ", "<! ", ">! "]),
        )
            .prop_map(|(c, op)| format!("{op}{}", sctid(item(c))))
    }

    proptest! {
        #[test]
        fn the_algebra_holds(x in concept(), y in concept(), a in operand(), b in operand()) {
            let dir = tempfile::tempdir().expect("tempdir");
            ferroterm_testkit::snomed::write(dir.path()).expect("writes");
            let p = SnomedProvider::open(dir.path(), "en").expect("opens");
            let code = |c: u32| sctid(item(c));
            let or_self = eval(&p, &format!("<< {}", code(x)));
            prop_assert!(or_self.contains(x));
            let below = eval(&p, &format!("< {}", code(x)));
            let above = eval(&p, &format!("> {}", code(x)));
            prop_assert!(below.is_disjoint(&above));
            let above_y = eval(&p, &format!("> {}", code(y)));
            prop_assert_eq!(
                below.contains(y),
                above_y.contains(x),
                "descendants and ancestors are inverse"
            );
            let set_a = eval(&p, &a);
            let set_b = eval(&p, &b);
            let both = eval(&p, &format!("{a} AND {b}"));
            prop_assert!(both.is_subset(&set_a) && both.is_subset(&set_b));
            prop_assert_eq!(&both, &(&set_a & &set_b));
            let either = eval(&p, &format!("{a} OR {b}"));
            prop_assert_eq!(&either, &(&set_a | &set_b));
            let minus = eval(&p, &format!("{a} MINUS {b}"));
            prop_assert!(minus.is_disjoint(&set_b) && minus.is_subset(&set_a));
        }
    }
}
