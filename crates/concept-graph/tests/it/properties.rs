// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Property tests: the closure agrees with brute-force reachability on random
//! acyclic graphs, and the set operations keep their algebraic identities.

use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_graph::subsumption::{Outcome, subsumes};
use proptest::prelude::*;

/// A random DAG over `n` nodes: every edge points from a higher ordinal
/// (child) to a lower one (parent), so it is acyclic by construction.
fn dag() -> impl Strategy<Value = (u32, Vec<(Ordinal, Ordinal)>)> {
    (2_u32..24).prop_flat_map(|n| {
        let edges =
            prop::collection::vec((1..n, 0..n), 0..(to_usize(n) * 3)).prop_map(move |pairs| {
                pairs
                    .into_iter()
                    .filter(|(child, parent)| parent < child)
                    .map(|(child, parent)| (Ordinal::new(child), Ordinal::new(parent)))
                    .collect::<Vec<_>>()
            });
        (Just(n), edges)
    })
}

/// A `u32` ordinal as a slice index.
fn to_usize(value: u32) -> usize {
    usize::try_from(value).expect("a u32 fits usize")
}

/// Reachability by a plain depth-first walk over the parent lists.
fn brute_force_ancestors(n: u32, edges: &[(Ordinal, Ordinal)], node: u32) -> Vec<u32> {
    let mut seen = vec![false; to_usize(n)];
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        for (child, parent) in edges {
            let slot = to_usize(parent.index());
            if child.index() == current && seen.get(slot) == Some(&false) {
                if let Some(flag) = seen.get_mut(slot) {
                    *flag = true;
                }
                stack.push(parent.index());
            }
        }
    }
    (0..n)
        .filter(|i| seen.get(to_usize(*i)) == Some(&true))
        .collect()
}

proptest! {
    #[test]
    fn closure_equals_brute_force_reachability((n, edges) in dag()) {
        let is_a = Csr::build(n, edges.iter().copied()).expect("in range");
        let closure = Closure::compute(&is_a).expect("acyclic by construction");
        for node in 0..n {
            let expected = brute_force_ancestors(n, &edges, node);
            let actual: Vec<u32> = closure.ancestors(Ordinal::new(node)).iter().collect();
            prop_assert_eq!(actual, expected);
        }
    }

    #[test]
    fn ancestors_and_descendants_are_inverse((n, edges) in dag()) {
        let is_a = Csr::build(n, edges.iter().copied()).expect("in range");
        let closure = Closure::compute(&is_a).expect("acyclic by construction");
        for a in 0..n {
            for b in 0..n {
                let a_above_b = closure.ancestors(Ordinal::new(b)).contains(a);
                let b_below_a = closure.descendants(Ordinal::new(a)).contains(b);
                prop_assert_eq!(a_above_b, b_below_a);
            }
            prop_assert!(closure.descendants_or_self(Ordinal::new(a)).contains(a));
            prop_assert!(closure.ancestors_or_self(Ordinal::new(a)).contains(a));
            prop_assert!(!closure.ancestors(Ordinal::new(a)).contains(a));
        }
    }

    #[test]
    fn subsumption_is_antisymmetric_and_reflexive((n, edges) in dag()) {
        let is_a = Csr::build(n, edges.iter().copied()).expect("in range");
        let closure = Closure::compute(&is_a).expect("acyclic by construction");
        for a in 0..n {
            prop_assert_eq!(subsumes(&closure, Ordinal::new(a), Ordinal::new(a)), Outcome::Equivalent);
            for b in 0..n {
                let forward = subsumes(&closure, Ordinal::new(a), Ordinal::new(b));
                let backward = subsumes(&closure, Ordinal::new(b), Ordinal::new(a));
                let mirrored = match forward {
                    Outcome::Subsumes => Outcome::SubsumedBy,
                    Outcome::SubsumedBy => Outcome::Subsumes,
                    other => other,
                };
                prop_assert_eq!(backward, mirrored);
            }
        }
    }

    #[test]
    fn the_layout_round_trips_any_hierarchy((n, edges) in dag()) {
        let is_a = Csr::build(n, edges.iter().copied()).expect("in range");
        let closure = Closure::compute(&is_a).expect("acyclic by construction");
        let hierarchy = Hierarchy { is_a, closure };
        let mut bytes = Vec::new();
        hierarchy.write_to(&mut bytes).expect("writes");
        let back = Hierarchy::read_from(&mut bytes.as_slice()).expect("reads");
        prop_assert_eq!(back, hierarchy);
    }
}
