// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Opening a side file for a structure to be assembled from.
//!
//! Every structure of a built artifact is read once, from front to back, into
//! the form it is served from. Reading the file into a `Vec` first puts the
//! whole file in memory beside the structure it becomes, and the allocator
//! does not hand those pages back the moment the `Vec` drops (#322), so a
//! reader assembles from a buffered file instead. No FHIR or SNOMED CT
//! specification governs this: our own design.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// How much of a side file a reader holds at a time.
///
/// Wide enough that a structure of hundreds of megabytes is assembled in a few
/// thousand reads, small enough that the buffer is never the figure a resident
/// measurement reports.
pub const BUFFER_BYTES: usize = 1 << 20;

/// Opens `path` buffered, for a structure to be assembled from.
///
/// # Errors
///
/// Returns the I/O error from opening the file.
pub fn buffered(path: &Path) -> std::io::Result<BufReader<File>> {
    Ok(BufReader::with_capacity(BUFFER_BYTES, File::open(path)?))
}
