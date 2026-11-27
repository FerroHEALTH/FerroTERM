// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary text as a terminology syndication feed.
//!
//! The synchronisation service reads the Atom feed (RFC 4287) a terminology
//! service publishes on the internet, with the NCTS Atom Syndication Format
//! extensions; a malformed or hostile document is a typed `ParseError`, never
//! a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _feed = terminology_syndication::parse::feed(text);
});
