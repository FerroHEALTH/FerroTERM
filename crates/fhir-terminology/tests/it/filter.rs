// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every `filter-operator` code through the generic evaluation
//! (<https://hl7.org/fhir/R5/codesystem-filter-operator.html>).

use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{CodeSystemProvider, ProviderError};

use crate::fixture::{Fixture, codes};

fn run(
    provider: &Fixture,
    property: &str,
    op: FilterOperator,
    value: &str,
) -> Result<Vec<String>, ProviderError> {
    let filter = Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    };
    provider.filter(&filter).map(|set| codes(provider, &set))
}

#[test]
fn code_filters_select_by_code() {
    let p = Fixture::hierarchical("2025");
    assert_eq!(
        run(&p, "concept", FilterOperator::Equal, "cat").unwrap(),
        ["cat"]
    );
    assert_eq!(
        run(&p, "code", FilterOperator::In, "cat, dog").unwrap(),
        ["cat", "dog"]
    );
    assert_eq!(
        run(&p, "concept", FilterOperator::NotIn, "cat,dog,fish,kitten").unwrap(),
        ["animal", "plant", "root"]
    );
    assert_eq!(
        run(&p, "concept", FilterOperator::Regex, "^.*t$").unwrap(),
        ["cat", "plant", "root"]
    );
    assert_eq!(
        run(&p, "concept", FilterOperator::Exists, "true")
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        run(&p, "concept", FilterOperator::Exists, "false").unwrap(),
        Vec::<String>::new()
    );
    assert!(matches!(
        run(&p, "concept", FilterOperator::Exists, "maybe"),
        Err(ProviderError::InvalidFilterValue(_))
    ));
    assert!(matches!(
        run(&p, "concept", FilterOperator::Regex, "("),
        Err(ProviderError::Regex(_))
    ));
    assert!(matches!(
        run(&p, "concept", FilterOperator::Equal, "unicorn"),
        Err(ProviderError::UnknownCode(code)) if code == "unicorn"
    ));
}

#[test]
fn hierarchy_filters_follow_their_definitions() {
    let p = Fixture::hierarchical("2025");
    // is-a: descendants and self.
    assert_eq!(
        run(&p, "concept", FilterOperator::IsA, "animal").unwrap(),
        ["animal", "cat", "dog", "fish", "kitten"]
    );
    // descendent-of: descendants only.
    assert_eq!(
        run(&p, "concept", FilterOperator::DescendentOf, "animal").unwrap(),
        ["cat", "dog", "fish", "kitten"]
    );
    // is-not-a: everything outside descendants and self.
    assert_eq!(
        run(&p, "concept", FilterOperator::IsNotA, "animal").unwrap(),
        ["plant", "root"]
    );
    // generalizes: ancestors and self.
    assert_eq!(
        run(&p, "concept", FilterOperator::Generalizes, "kitten").unwrap(),
        ["animal", "cat", "kitten", "root"]
    );
    // child-of: direct children, not the index code.
    assert_eq!(
        run(&p, "concept", FilterOperator::ChildOf, "animal").unwrap(),
        ["cat", "dog", "fish"]
    );
    // descendent-leaf: descendants without children.
    assert_eq!(
        run(&p, "concept", FilterOperator::DescendentLeaf, "animal").unwrap(),
        ["dog", "fish", "kitten"]
    );
    // parent = X selects the children of X; child = X the parents.
    assert_eq!(
        run(&p, "parent", FilterOperator::Equal, "cat").unwrap(),
        ["kitten"]
    );
    assert_eq!(
        run(&p, "child", FilterOperator::Equal, "kitten").unwrap(),
        ["cat"]
    );
}

#[test]
fn property_filters_compare_values_and_refuse_undeclared_properties() {
    let p = Fixture::hierarchical("2025");
    assert_eq!(
        run(&p, "legs", FilterOperator::Equal, "4").unwrap(),
        ["cat", "dog", "kitten"]
    );
    assert_eq!(
        run(&p, "legs", FilterOperator::In, "0,4").unwrap(),
        ["cat", "dog", "fish", "kitten"]
    );
    assert_eq!(
        run(&p, "legs", FilterOperator::NotIn, "4").unwrap(),
        ["animal", "fish", "plant", "root"]
    );
    assert_eq!(
        run(&p, "legs", FilterOperator::Regex, "^[1-9]").unwrap(),
        ["cat", "dog", "kitten"]
    );
    assert_eq!(
        run(&p, "legs", FilterOperator::Exists, "false").unwrap(),
        ["animal", "plant", "root"]
    );
    assert_eq!(
        run(&p, "kingdom", FilterOperator::Equal, "animal").unwrap(),
        ["cat", "dog", "fish", "kitten"]
    );
    assert!(matches!(
        run(&p, "colour", FilterOperator::Equal, "red"),
        Err(ProviderError::UnsupportedFilter { property, operator }) if property == "colour" && operator == "="
    ));
    assert!(matches!(
        run(&p, "legs", FilterOperator::IsA, "4"),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
}

#[test]
fn a_system_without_a_hierarchy_refuses_the_hierarchy_operators() {
    let flat = Fixture::flat();
    for op in FilterOperator::ALL
        .into_iter()
        .filter(|op| op.hierarchical())
    {
        assert!(
            matches!(
                run(&flat, "concept", op, "animal"),
                Err(ProviderError::UnsupportedFilter { .. })
            ),
            "{}",
            op.code()
        );
    }
    assert!(matches!(
        run(&flat, "parent", FilterOperator::Equal, "animal"),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
    assert_eq!(
        run(&flat, "concept", FilterOperator::Equal, "cat").unwrap(),
        ["cat"]
    );
}
