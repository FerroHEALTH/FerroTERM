// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `CodeSystem/$subsumes` in the terms every served FHIR version shares.
//!
//! Two codes (or two codings) of one system, and the subsumption outcome of
//! the provider or its hierarchy
//! (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).

use concept_graph::subsumption::Outcome;

use super::{CodingRef, Invocation, OperationError, locate, resolve};
use crate::registry::Registry;

/// The input of `$subsumes`: `codeA` and `codeB` with `system`, or `codingA`
/// and `codingB`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubsumesInput {
    /// The first code.
    pub code_a: Option<String>,
    /// The second code.
    pub code_b: Option<String>,
    /// The code system URI.
    pub system: Option<String>,
    /// The code system version.
    pub version: Option<String>,
    /// The first coding, instead of `codeA`.
    pub coding_a: Option<CodingRef>,
    /// The second coding, instead of `codeB`.
    pub coding_b: Option<CodingRef>,
}

/// Runs `$subsumes`.
///
/// # Errors
///
/// Returns [`OperationError`] for a missing or mixed pair, codings of two
/// systems or versions, an unknown system, version, or code, a system
/// without subsumption, or a provider failure.
pub fn subsumes(
    registry: &Registry,
    invocation: &Invocation,
    input: &SubsumesInput,
) -> Result<Outcome, OperationError> {
    let codes = (input.code_a.as_deref(), input.code_b.as_deref());
    let codings = (&input.coding_a, &input.coding_b);
    let mut system = input.system.as_deref();
    let mut version = input.version.as_deref();
    let (a, b) = match (codes, codings) {
        ((Some(a), Some(b)), (None, None)) => (a, b),
        ((None, None), (Some(coding_a), Some(coding_b))) => {
            for other in [coding_a.system.as_deref(), coding_b.system.as_deref()]
                .into_iter()
                .flatten()
            {
                match system {
                    Some(tested) if tested != other => {
                        return Err(OperationError::NotSupported(format!(
                            "coding system `{other}` differs from the tested system `{tested}`"
                        )));
                    }
                    Some(_) => {}
                    None => system = Some(other),
                }
            }
            if let (Some(va), Some(vb)) = (&coding_a.version, &coding_b.version)
                && va != vb
            {
                return Err(OperationError::Invalid(String::from(
                    "`codingA` and `codingB` name different versions",
                )));
            }
            version = version
                .or(coding_a.version.as_deref())
                .or(coding_b.version.as_deref());
            let a = coding_a.code.as_deref().ok_or_else(|| {
                OperationError::Required(String::from("`codingA.code` is required"))
            })?;
            let b = coding_b.code.as_deref().ok_or_else(|| {
                OperationError::Required(String::from("`codingB.code` is required"))
            })?;
            (a, b)
        }
        ((None, None), (None, None)) => {
            return Err(OperationError::Required(String::from(
                "provide `codeA` and `codeB`, or `codingA` and `codingB`",
            )));
        }
        _ => {
            return Err(OperationError::Invalid(String::from(
                "provide both codes as `codeA` and `codeB`, or both as `codingA` and `codingB`",
            )));
        }
    };
    let resolved = resolve(registry, invocation, system, version)?;
    let provider = &resolved.provider;
    let a = locate(provider, a)?.concept;
    let b = locate(provider, b)?.concept;
    if let Some(outcome) = provider.subsumes(a, b)? {
        return Ok(outcome);
    }
    let hierarchy = provider.hierarchy().ok_or_else(|| {
        OperationError::NotSupported(format!(
            "code system `{}` declares no subsumption",
            provider.identity().url
        ))
    })?;
    Ok(hierarchy.subsumes(a, b))
}
