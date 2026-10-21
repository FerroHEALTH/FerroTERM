// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The transitive closure of the is-a hierarchy.
//!
//! Computed once, offline, by a bitset sweep in topological order: a node's
//! ancestor set is the union of its parents' ancestor sets plus the parents
//! themselves; descendants are the same sweep over the transposed adjacency.
//! A cycle in the is-a edges is a defect in the input and is refused.
//! Roaring bitmaps keep the sets small and iterate in sorted order, which
//! keeps every consumer deterministic.

use roaring::RoaringBitmap;

use crate::csr::{Csr, CsrError};
use crate::ordinal::{Ordinal, to_usize};

/// A failure while computing the closure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClosureError {
    /// The is-a edges contain a cycle through these ordinals.
    #[error("the is-a hierarchy has a cycle through {} node(s), for example {first}", .members.len())]
    Cycle {
        /// The first node found on a cycle.
        first: Ordinal,
        /// Every node not reachable from the roots by a topological sweep.
        members: Vec<Ordinal>,
    },
    /// The adjacency is inconsistent.
    #[error(transparent)]
    Csr(#[from] CsrError),
}

/// The ancestor and descendant sets of every node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    ancestors: Vec<RoaringBitmap>,
    descendants: Vec<RoaringBitmap>,
}

static EMPTY: std::sync::LazyLock<RoaringBitmap> = std::sync::LazyLock::new(RoaringBitmap::new);

impl Closure {
    /// Computes the closure of `is_a`, whose edges point from child to parent.
    ///
    /// # Errors
    ///
    /// Returns [`ClosureError::Cycle`] when the edges are not acyclic.
    pub fn compute(is_a: &Csr) -> Result<Self, ClosureError> {
        let parents = is_a;
        let children = is_a.transpose()?;
        let order = topological_order(parents, &children)?;
        let nodes = to_usize(parents.nodes());
        Ok(Self {
            ancestors: sweep(parents, order.iter(), nodes),
            descendants: sweep(&children, order.iter().rev(), nodes),
        })
    }

    /// Reassembles a closure from its two bitmap lists.
    #[must_use]
    pub fn from_parts(ancestors: Vec<RoaringBitmap>, descendants: Vec<RoaringBitmap>) -> Self {
        Self {
            ancestors,
            descendants,
        }
    }

    /// The heap bytes the two bitmap lists hold ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        crate::footprint::bitmaps(&self.ancestors)
            .saturating_add(crate::footprint::bitmaps(&self.descendants))
    }

    /// The heap bytes the ancestor sets alone hold.
    ///
    /// Subsumption reads the ancestors and an expansion reads the
    /// descendants, so the two halves are reported apart.
    #[must_use]
    pub fn ancestors_size_in_bytes(&self) -> usize {
        crate::footprint::bitmaps(&self.ancestors)
    }

    /// The number of nodes.
    #[must_use]
    pub fn nodes(&self) -> u32 {
        u32::try_from(self.ancestors.len()).unwrap_or(u32::MAX)
    }

    /// The proper ancestors of `node` (ECL `>`); empty for an unknown node.
    #[must_use]
    pub fn ancestors(&self, node: Ordinal) -> &RoaringBitmap {
        self.ancestors
            .get(node.as_usize())
            .unwrap_or_else(|| &*EMPTY)
    }

    /// The proper descendants of `node` (ECL `<`); empty for an unknown node.
    #[must_use]
    pub fn descendants(&self, node: Ordinal) -> &RoaringBitmap {
        self.descendants
            .get(node.as_usize())
            .unwrap_or_else(|| &*EMPTY)
    }

    /// `node` and its descendants (ECL `<<`).
    #[must_use]
    pub fn descendants_or_self(&self, node: Ordinal) -> RoaringBitmap {
        let mut set = self.descendants(node).clone();
        set.insert(node.index());
        set
    }

    /// `node` and its ancestors (ECL `>>`).
    #[must_use]
    pub fn ancestors_or_self(&self, node: Ordinal) -> RoaringBitmap {
        let mut set = self.ancestors(node).clone();
        set.insert(node.index());
        set
    }

    /// Whether `ancestor` is a proper ancestor of `node`.
    #[must_use]
    pub fn is_ancestor(&self, ancestor: Ordinal, node: Ordinal) -> bool {
        self.ancestors(node).contains(ancestor.index())
    }

    /// The ancestor bitmaps, in ordinal order.
    #[must_use]
    pub fn ancestor_sets(&self) -> &[RoaringBitmap] {
        &self.ancestors
    }

    /// The descendant bitmaps, in ordinal order.
    #[must_use]
    pub fn descendant_sets(&self) -> &[RoaringBitmap] {
        &self.descendants
    }
}

/// The set each node of `edges` reaches, swept in `order`: a node's set is its
/// neighbours plus the sets they already reached.
///
/// `order` visits a node only after every neighbour it reads, so one pass fills
/// every set.
fn sweep<'a>(
    edges: &Csr,
    order: impl Iterator<Item = &'a Ordinal>,
    nodes: usize,
) -> Vec<RoaringBitmap> {
    let mut sets: Vec<RoaringBitmap> = vec![RoaringBitmap::new(); nodes];
    for node in order {
        let mut set = RoaringBitmap::new();
        for neighbour in edges.neighbours(*node) {
            set.insert(*neighbour);
            if let Some(reached) = sets.get(to_usize(*neighbour)) {
                set |= reached;
            }
        }
        if let Some(slot) = sets.get_mut(node.as_usize()) {
            *slot = set;
        }
    }
    sets
}

/// Kahn's algorithm over child-to-parent edges: parents before children.
fn topological_order(parents: &Csr, children: &Csr) -> Result<Vec<Ordinal>, ClosureError> {
    let nodes = parents.nodes();
    let mut remaining: Vec<u32> = (0..nodes)
        .map(|n| u32::try_from(parents.neighbours(Ordinal::new(n)).len()).unwrap_or(u32::MAX))
        .collect();
    let mut ready: Vec<Ordinal> = (0..nodes)
        .filter(|n| remaining.get(to_usize(*n)) == Some(&0))
        .map(Ordinal::new)
        .collect();
    let mut order = Vec::with_capacity(to_usize(nodes));
    while let Some(node) = ready.pop() {
        order.push(node);
        for child in children.neighbours(node) {
            if let Some(count) = remaining.get_mut(to_usize(*child)) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ready.push(Ordinal::new(*child));
                }
            }
        }
    }
    if order.len() != to_usize(nodes) {
        let members: Vec<Ordinal> = (0..nodes)
            .filter(|n| remaining.get(to_usize(*n)).is_some_and(|c| *c > 0))
            .map(Ordinal::new)
            .collect();
        let first = members.first().copied().unwrap_or(Ordinal::new(0));
        return Err(ClosureError::Cycle { first, members });
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::{Closure, ClosureError};
    use crate::csr::Csr;
    use crate::ordinal::Ordinal;

    fn o(i: u32) -> Ordinal {
        Ordinal::new(i)
    }

    /// 0 is the root; 1 and 2 are its children; 3 is a child of both 1 and 2.
    fn diamond() -> Closure {
        let is_a = Csr::build(4, [(o(1), o(0)), (o(2), o(0)), (o(3), o(1)), (o(3), o(2))])
            .expect("builds");
        Closure::compute(&is_a).expect("acyclic")
    }

    #[test]
    fn ancestors_and_descendants_are_transitive_and_inverse() {
        let closure = diamond();
        assert_eq!(
            closure.ancestors(o(3)).iter().collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            closure.descendants(o(0)).iter().collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            closure.descendants(o(1)).iter().collect::<Vec<_>>(),
            vec![3]
        );
        assert!(closure.ancestors(o(0)).is_empty());
        assert!(closure.descendants(o(3)).is_empty());
        assert!(closure.is_ancestor(o(0), o(3)));
        assert!(!closure.is_ancestor(o(3), o(0)));
        assert!(closure.descendants_or_self(o(3)).contains(3));
        assert_eq!(closure.ancestors_or_self(o(3)).len(), 4);
    }

    #[test]
    fn a_cycle_is_refused() {
        let is_a = Csr::build(3, [(o(0), o(1)), (o(1), o(2)), (o(2), o(0))]).expect("builds");
        match Closure::compute(&is_a) {
            Err(ClosureError::Cycle { members, .. }) => assert_eq!(members.len(), 3),
            other => panic!("expected a cycle, got {other:?}"),
        }
    }

    #[test]
    fn unknown_nodes_have_empty_sets() {
        let closure = diamond();
        assert!(closure.ancestors(o(99)).is_empty());
        assert!(closure.descendants(o(99)).is_empty());
    }
}
