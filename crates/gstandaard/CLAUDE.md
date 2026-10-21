<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# gstandaard

The reader for the G-Standaard (Z-Index) product ladder. Hand-written; the
Z-Index file descriptions
(<https://www.z-index.nl/documentatie/bestandsbeschrijvingen>) are the
authority for every record layout and field position.

- The files are fixed-length Latin-1 records, one per line, named `BSTnnnT`;
  the reader finds them by that prefix, case-insensitively, under one
  directory. Field positions are the published ones and are named in the
  code by their Z-Index field names.
- The ladder is four code systems (GPK, PRK, HPK, artikel), each flat; the
  links between rungs are properties, never an in-system hierarchy.
- The G-Standaard is a paid subscription and is never committed; fixtures are
  records with invented codes and names at the published positions.
