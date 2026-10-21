// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Compressed sparse row adjacency.
//!
//! One `Csr` holds the out-edges of one edge kind in one direction: an
//! offsets array of `node_count + 1` entries and a targets array, so the
//! neighbours of ordinal `n` are `targets[offsets[n]..offsets[n + 1]]`.
//! Targets are sorted and deduplicated within each row.

use crate::ordinal::{Ordinal, to_usize};

/// A failure while building adjacency.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CsrError {
    /// An edge names an ordinal at or beyond the node count.
    #[error("ordinal {ordinal} is out of range for {nodes} nodes")]
    OutOfRange {
        /// The offending ordinal.
        ordinal: Ordinal,
        /// The node count.
        nodes: u32,
    },
    /// The offsets array is not monotone or does not end at the targets length.
    #[error("offsets are inconsistent with {targets} targets")]
    Offsets {
        /// The targets length.
        targets: usize,
    },
}

/// Compressed sparse row adjacency for one edge kind and direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Csr {
    offsets: Vec<u32>,
    targets: Vec<u32>,
}

impl Csr {
    /// Builds adjacency over `nodes` ordinals from `(from, to)` pairs.
    ///
    /// Duplicate pairs collapse; each row comes out sorted.
    ///
    /// # Errors
    ///
    /// Returns [`CsrError::OutOfRange`] when an edge names an ordinal at or
    /// beyond `nodes`, and [`CsrError::Offsets`] when the edge count exceeds `u32`.
    pub fn build(
        nodes: u32,
        edges: impl IntoIterator<Item = (Ordinal, Ordinal)>,
    ) -> Result<Self, CsrError> {
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        for (from, to) in edges {
            for ordinal in [from, to] {
                if ordinal.index() >= nodes {
                    return Err(CsrError::OutOfRange { ordinal, nodes });
                }
            }
            pairs.push((from.index(), to.index()));
        }
        pairs.sort_unstable();
        pairs.dedup();
        // A target count past u32::MAX is inconsistent with the offsets; the
        // conversion error adds nothing to that.
        let count = |targets: &Vec<u32>| {
            let Ok(count) = u32::try_from(targets.len()) else {
                return Err(CsrError::Offsets {
                    targets: targets.len(),
                });
            };
            Ok(count)
        };
        let mut offsets = Vec::with_capacity(to_usize(nodes).saturating_add(1));
        let mut targets = Vec::with_capacity(pairs.len());
        let mut cursor = 0_usize;
        for node in 0..nodes {
            offsets.push(count(&targets)?);
            while let Some((from, to)) = pairs.get(cursor).copied() {
                if from != node {
                    break;
                }
                targets.push(to);
                cursor = cursor.saturating_add(1);
            }
        }
        offsets.push(count(&targets)?);
        Ok(Self { offsets, targets })
    }

    /// Reassembles adjacency from its two arrays, checking their consistency.
    ///
    /// # Errors
    ///
    /// Returns [`CsrError::Offsets`] when `offsets` is empty, not monotone, or
    /// does not end at `targets.len()`.
    pub fn from_parts(offsets: Vec<u32>, targets: Vec<u32>) -> Result<Self, CsrError> {
        let consistent = offsets.first() == Some(&0)
            && offsets.windows(2).all(|w| w.first() <= w.get(1))
            && offsets.last().copied().map(to_usize) == Some(targets.len());
        if !consistent {
            return Err(CsrError::Offsets {
                targets: targets.len(),
            });
        }
        Ok(Self { offsets, targets })
    }

    /// The number of nodes.
    #[must_use]
    pub fn nodes(&self) -> u32 {
        u32::try_from(self.offsets.len().saturating_sub(1)).unwrap_or(u32::MAX)
    }

    /// The number of edges.
    #[must_use]
    pub fn edges(&self) -> usize {
        self.targets.len()
    }

    /// The heap bytes this adjacency holds
    /// ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        crate::footprint::vector(&self.offsets)
            .saturating_add(crate::footprint::vector(&self.targets))
    }

    /// The neighbours of `node`, sorted; empty for an unknown node.
    #[must_use]
    pub fn neighbours(&self, node: Ordinal) -> &[u32] {
        let start = self.offsets.get(node.as_usize()).copied();
        let end = self.offsets.get(node.as_usize().saturating_add(1)).copied();
        match (start, end) {
            (Some(start), Some(end)) => self
                .targets
                .get(to_usize(start)..to_usize(end))
                .unwrap_or(&[]),
            _ => &[],
        }
    }

    /// The offsets array.
    #[must_use]
    pub fn offsets(&self) -> &[u32] {
        &self.offsets
    }

    /// The targets array.
    #[must_use]
    pub fn targets(&self) -> &[u32] {
        &self.targets
    }

    /// The same edges reversed.
    ///
    /// # Errors
    ///
    /// Returns [`CsrError`] only if this adjacency is itself inconsistent.
    pub fn transpose(&self) -> Result<Self, CsrError> {
        let nodes = self.nodes();
        let reversed = (0..nodes).flat_map(|from| {
            self.neighbours(Ordinal::new(from))
                .iter()
                .map(move |to| (Ordinal::new(*to), Ordinal::new(from)))
        });
        Self::build(nodes, reversed)
    }
}

#[cfg(test)]
mod tests {
    use super::{Csr, CsrError};
    use crate::ordinal::Ordinal;

    fn o(i: u32) -> Ordinal {
        Ordinal::new(i)
    }

    #[test]
    fn rows_are_sorted_and_deduplicated() {
        let csr = Csr::build(4, [(o(2), o(1)), (o(0), o(3)), (o(0), o(1)), (o(0), o(1))])
            .expect("builds");
        assert_eq!(csr.nodes(), 4);
        assert_eq!(csr.edges(), 3);
        assert_eq!(csr.neighbours(o(0)), &[1, 3]);
        assert!(csr.neighbours(o(1)).is_empty());
        assert_eq!(csr.neighbours(o(2)), &[1]);
        assert!(csr.neighbours(o(9)).is_empty());
        assert_eq!(csr.offsets(), &[0, 2, 2, 3, 3]);
    }

    #[test]
    fn transpose_reverses_every_edge() {
        let csr = Csr::build(3, [(o(0), o(1)), (o(0), o(2)), (o(1), o(2))]).expect("builds");
        let back = csr.transpose().expect("transposes");
        assert_eq!(back.neighbours(o(2)), &[0, 1]);
        assert_eq!(back.neighbours(o(1)), &[0]);
        assert!(back.neighbours(o(0)).is_empty());
        assert_eq!(back.transpose().expect("transposes"), csr);
    }

    #[test]
    fn out_of_range_and_inconsistent_parts_are_refused() {
        assert_eq!(
            Csr::build(2, [(o(0), o(2))]),
            Err(CsrError::OutOfRange {
                ordinal: o(2),
                nodes: 2
            })
        );
        assert!(Csr::from_parts(vec![0, 2, 1], vec![1]).is_err());
        assert!(Csr::from_parts(vec![1, 1], Vec::new()).is_err());
        assert!(Csr::from_parts(vec![0, 1], vec![0]).is_ok());
    }
}
