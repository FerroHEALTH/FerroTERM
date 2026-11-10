// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nationale Terminologie Server (NTS) source add-on.
//!
//! The NTS is the Dutch national terminology service, run by Nictiz
//! (<https://www.nictiz.nl/publicaties/nationale-terminologie-server-handleiding-voor-nieuwe-gebruikers/>).
//! It is an Ontoserver deployment, so its downloadable content is announced in
//! the Atom syndication dialect `terminology-syndication` reads, and the whole
//! feed, including the listing itself, sits behind an OAuth 2 bearer
//! challenge.
//!
//! This crate is the service's identity and nothing else: where the feed is
//! and the `[nts]` endpoint table a deployment starts from ([`config`]), the
//! one content correction the service needs ([`fixup`]), and the
//! [`source::NtsSource`] that implements
//! `terminology_syndication::source::Source` over the family outbound client,
//! which obtains the token and keeps it fresh. Reading the feed, deciding what
//! a run takes, and fetching a verified content item all belong to that crate
//! and are not repeated here.
//!
//! The base URL is the `[nts]` table's `url`, so a second Ontoserver
//! deployment with the same authentication shape is this add-on pointed
//! elsewhere.
#![doc(test(attr(deny(warnings))))]

pub mod config;
pub mod fixup;
pub mod source;
