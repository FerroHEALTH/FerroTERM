// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An expression that parses prints and parses back to the same tree.
//!
//! `proptest` asserts this over generated trees; this asserts it over the
//! trees arbitrary text produces, which reach corners a generator does not.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(tree) = sct_ecl::parse(text) else {
        return;
    };
    let printed = tree.to_string();
    match sct_ecl::parse(&printed) {
        Ok(again) => assert_eq!(again, tree, "printing `{printed}` lost the tree"),
        Err(error) => panic!("the printed form `{printed}` does not parse: {error}"),
    }
});
