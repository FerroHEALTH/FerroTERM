<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# terminology-syndication

The terminology syndication client. Hand-written; the Atom Syndication Format
(RFC 4287, <https://www.rfc-editor.org/rfc/rfc4287>) and the NCTS extensions
Ontoserver documents
(<https://www.ontoserver.csiro.au/docs/6.22.5/syndication.html>) are the
authority. No FHIR or SNOMED CT specification governs the feed dialect; the
crate layout is our own design.

- Modules: `model` (the typed feed, the category terms, the checksum, the link
  relations, the SNOMED release a version URI names), `parse` (the
  namespace-aware Atom reader), `select` (the subscription, the holdings, and
  the reasons an entry is left behind), `download` (the streamed,
  digest-verified fetch), `source` (the `Source` seam).
- **Code-system-neutral and country-neutral.** No operator, country, or
  national service is named anywhere in `src/`. A service's feed address, its
  authentication, and any fix-up it needs live in its own add-on crate.
- **Nothing is dropped in silence.** A category term the model does not name is
  kept as `CategoryTerm::Other`, and every entry a run leaves behind carries a
  `SkipReason` a run record can print.
- **A download is verified before it counts.** The bytes stream to
  `<destination>.part`; the destination appears only after the advertised
  digest matches, and a mismatch removes the partial file.

## The add-on contract

An add-on implements six methods, `name`, `feed_url`, `yields`, `client`,
`listing_auth`, and `download_auth`, and takes `list` and `fetch` as the trait
provides them. It overrides either of those only when its service departs from
the dialect, and documents that. The trait is object-safe, so a caller holds
its configured add-ons in one `Vec<Box<dyn Source>>`.

Listing and download authenticate separately because the services differ: some
challenge the listing itself, some serve the listing openly and challenge only
the download, some challenge neither, and some take affiliate credentials. An
add-on obtains and refreshes credentials inside the two auth methods, which are
called once per request, so a token that expires mid-run is renewed without the
caller knowing tokens exist. Credentials come from the deployment's own
configuration; this crate reads no file and no environment variable.

## Fixtures

- `tests/fixtures/*.xml` are synthetic Atom documents with invented
  identifiers, written to cover every category term the model names, both
  digest algorithms, the quoted `atom:source` block, and the malformed values
  the parser refuses.
- `vendor/feeds/*.xml` are three public listings, vendored verbatim by
  `scripts/vendor/syndication-feeds.sh` and stamped in
  `vendor/feeds/PROVENANCE.md`. They carry listing metadata and no terminology
  content. Never hand-edit one: change the fetcher and re-run it, by hand and
  never in CI. A corpus assertion that stops holding after a refresh means the
  listing moved, so read the new document before re-pinning the number.
