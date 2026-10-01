# The FHIR model

The FHIR layer of FerroTERM is generated, not hand-written, and it is generated
in another repository. This page explains what that means for a contributor.

<!-- toc -->

## Why the FHIR layer is generated

HL7 publishes the whole FHIR type system and every operation as machine-readable
`StructureDefinition` and `OperationDefinition` resources, in versioned packages.
Rust modules are generated per version from those packages, so each version's
operation surface is correct by construction: a parameter that R5 adds appears
in the R5 module because the R5 package declares it.

## Where it comes from

The model is the [`fhir-types`](https://crates.io/crates/fhir-types) crate. The
[FerroBRIDGE](https://github.com/rubentalstra/FerroBRIDGE) repository vendors
the pinned HL7 packages, runs the generator, and publishes the crate.
FerroTERM depends on it from crates.io, at the version `docs/VERSIONS.md` pins
and the root `Cargo.toml` requires. No HL7 package is vendored in this
repository.

## The rules for a contributor

- **Consume the generated types directly.** Never re-model or re-serialize
  FHIR by hand, and never shadow a generated shape with a local type, an
  adapter layer, or a placeholder value.
- **A shape that is wrong or missing is fixed upstream.** Open it on the
  FerroBRIDGE tracker; the fix is a generator change there, released as a new
  version of the crate, then taken here.
- **Taking a new release moves three things in one change:** the requirement in
  the root `Cargo.toml`, the pin row in `docs/VERSIONS.md`, and `Cargo.lock`.
  `scripts/checks/versions.sh` fails when they disagree, so the served model
  and the recorded pin cannot drift apart. Dependabot opens that pull request
  by itself when a release lands.

The generator design follows the sibling project
[FerroEHR](https://github.com/FerroHEALTH/FerroEHR), which generates its openEHR
model from vendored machine-readable specs the same way.
