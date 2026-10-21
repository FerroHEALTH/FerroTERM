// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Computing the digests the tests advertise, independently of the crate.

use core::fmt::Write as _;

use sha2::Digest as _;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").expect("writing into a String cannot fail");
        out
    })
}

/// The lowercase hexadecimal SHA-256 of `body`.
pub(crate) fn sha256_of(body: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(body.as_bytes());
    hex(&hasher.finalize())
}

/// The lowercase hexadecimal MD5 of `body`.
pub(crate) fn md5_of(body: &str) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(body.as_bytes());
    hex(&hasher.finalize())
}
