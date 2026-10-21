// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a loaded structure holds in memory, counted allocation by allocation.
//!
//! A served edition's resident memory is the sum of the structures it loaded
//! plus what the allocator and the process keep on top, and a total says
//! nothing about which structure is which. Every structure here answers
//! `size_in_bytes`, so the accounting is added up from the structures rather
//! than inferred from the total. No FHIR or SNOMED CT specification governs
//! this: our own design.
//!
//! What a figure counts is the heap the structure's own allocations hold, at
//! their capacity rather than their length, because a vector that reserved
//! more than it filled holds the pages it reserved. What it does not count is
//! the allocator's own per-allocation header and the slack a `BTreeMap` node
//! carries; both belong to the residual a caller names against a measured
//! process footprint.

use std::mem::size_of;

use roaring::RoaringBitmap;

/// The bytes one container of a roaring bitmap costs beside its values.
///
// NOTE: roaring 0.11.5 stores `Container { key: u16, store: Store }` in a
// vector, and `Store`'s widest variant is a `Vec` triple, so a container is a
// 24-byte vector, an 8-byte tag, and the key in its padding.
const CONTAINER: usize = 40;

/// The heap bytes `set` holds.
///
/// Read through `RoaringBitmap::statistics`, whose per-container byte counts
/// are the serialized widths rather than the in-memory ones: an array
/// container stores `u16` values and is counted at `u32`, and a bitset
/// container is counted in bits. Both are converted here.
#[must_use]
#[expect(
    clippy::integer_division,
    reason = "both counts are exact multiples of their divisor, so no remainder exists to lose"
)]
pub fn bitmap(set: &RoaringBitmap) -> usize {
    let stats = set.statistics();
    let containers = usize::try_from(stats.n_containers).unwrap_or(usize::MAX);
    let arrays = usize::try_from(stats.n_bytes_array_containers).unwrap_or(usize::MAX) / 2;
    let bitsets = usize::try_from(stats.n_bytes_bitset_containers).unwrap_or(usize::MAX) / 8;
    let runs = usize::try_from(stats.n_bytes_run_containers).unwrap_or(usize::MAX);
    arrays
        .saturating_add(bitsets)
        .saturating_add(runs)
        .saturating_add(containers.saturating_mul(CONTAINER))
}

/// The heap bytes a list of bitmaps holds, the list's own vector included.
#[must_use]
pub fn bitmaps(sets: &[RoaringBitmap]) -> usize {
    let own = sets.len().saturating_mul(size_of::<RoaringBitmap>());
    sets.iter()
        .fold(own, |total, set| total.saturating_add(bitmap(set)))
}

/// The heap bytes a vector holds, its unfilled capacity included.
#[must_use]
pub fn vector<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(size_of::<T>())
}

/// The heap bytes a vector of strings holds, the text included.
#[must_use]
pub fn strings(values: &Vec<String>) -> usize {
    values.iter().fold(vector(values), |total, text| {
        total.saturating_add(text.capacity())
    })
}

/// The heap bytes the entries of a map hold, without the text or bitmaps in
/// them.
///
/// A `BTreeMap` packs its entries into nodes, so this is the entries at their
/// own width; the slack of a partly filled node is left to the residual.
#[must_use]
pub fn entries<K, V>(len: usize) -> usize {
    len.saturating_mul(size_of::<K>().saturating_add(size_of::<V>()))
}

#[cfg(test)]
mod tests {
    use roaring::RoaringBitmap;

    use super::{bitmap, bitmaps, entries, strings, vector};

    #[test]
    fn an_empty_bitmap_holds_nothing() {
        assert_eq!(bitmap(&RoaringBitmap::new()), 0);
    }

    #[test]
    fn a_sparse_bitmap_costs_two_bytes_a_value_and_one_container() {
        let set: RoaringBitmap = (0..100).collect();
        // One array container of at least 100 u16 values, plus its header.
        assert!(bitmap(&set) >= 200 + 40, "{}", bitmap(&set));
        assert!(bitmap(&set) < 1024, "{}", bitmap(&set));
    }

    #[test]
    fn a_dense_bitmap_costs_a_bitset_container() {
        let set: RoaringBitmap = (0..20_000).collect();
        // 1024 u64 words is 8 KiB, whatever the cardinality above the array
        // limit, plus the container header.
        assert_eq!(bitmap(&set), 8192 + 40);
    }

    #[test]
    fn a_list_of_bitmaps_carries_its_own_vector() {
        let sets = vec![RoaringBitmap::new(), RoaringBitmap::new()];
        assert_eq!(bitmaps(&sets), 2 * size_of::<RoaringBitmap>());
    }

    #[test]
    fn a_vector_is_counted_at_its_capacity() {
        let mut values: Vec<u32> = Vec::with_capacity(16);
        values.push(1);
        assert_eq!(vector(&values), 64);
    }

    #[test]
    fn strings_carry_their_text() {
        let values = vec![String::from("abcd")];
        assert_eq!(strings(&values), vector(&values) + 4);
    }

    #[test]
    fn map_entries_are_counted_at_their_own_width() {
        assert_eq!(entries::<u64, u32>(10), 10 * (8 + 4));
    }
}
