<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# labcodeset

The reader for the Nederlandse Labcodeset publication. Hand-written; the
publication's own XML schema (`Nederlandse Labcodeset v6.xsd`, shipped in
every release zip) is the authority for the elements and attributes.

- A release is a zip holding one `labconcepts-<date>.xml` document (plus the
  schema, release notes, and documentation); the reader takes the document or
  the directory holding it.
- The model is a flat list of laboratory concepts over LOINC, each with its
  Dutch translation, SNOMED CT materials, an outcome list (a SNOMED refset or
  an ordinal value set of the publication), and UCUM units; the publication's
  unit table, material table, ordinal value sets, and nominal refsets sit
  beside the concepts.
- An element or attribute the schema does not define is an error, never
  skipped: dropped content would be a silent wrong answer downstream.
- The publication is licensed by Nictiz and never committed; fixtures are
  shaped like the document with invented content.
