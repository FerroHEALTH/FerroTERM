// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Dense ordinals: the graph's own keys.
//!
//! A loader assigns every concept of a code system version a dense ordinal
//! and every relationship type an edge kind; the graph never sees a native
//! code. Both are `u32`, which bounds a version at about four billion
//! concepts and keeps roaring bitmaps at their native width.

use std::fmt;

/// The position of a concept in a code system version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ordinal(u32);

impl Ordinal {
    /// Wraps a position.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The position.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }

    /// The position as a `usize`, for indexing.
    #[must_use]
    pub fn as_usize(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

impl fmt::Display for Ordinal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// The kind of an edge: the ordinal of its relationship type concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeKind(u32);

impl EdgeKind {
    /// Wraps a kind.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The kind.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kind#{}", self.0)
    }
}

/// `u32` to `usize` without a lossy cast.
#[must_use]
pub fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}
