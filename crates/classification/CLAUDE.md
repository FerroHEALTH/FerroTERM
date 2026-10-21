<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# classification

The readers for statistical classifications: ClaML (ISO 13120; WHO ICD-10 and
its national translations, ICPC-2) and the NCHS ICD-10-CM tabular release.
Hand-written; the FHIR ICD page (<https://hl7.org/fhir/R4B/icd.html>) and the
ClaML DTD are the authority.

- Both readers produce one `Classification` model (chapters, blocks,
  categories, and subcategories, each with one parent, labelled and annotated
  by rubric kind); `ferroterm-build` turns that model into the served
  artifacts, so nothing downstream knows which file format a system came in.
- Codes carry the period the FHIR ICD page requires (`A00.0`); a source that
  omits it gets it inserted after the third character when the three-character
  category is an ancestor of the code. A code on another axis (the ICD-O
  morphology class `M953` under `M`) keeps the spelling the document gives it.
- The releases are licence-gated or bring-your-own and never committed;
  fixtures are shaped like the files, with invented content.
