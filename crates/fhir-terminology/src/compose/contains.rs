// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a compose contains one code, evaluated include by include against
//! the code itself, so a system that cannot enumerate its codes still
//! validates.

use std::sync::Arc;

use crate::language;
use crate::provider::compose::{Compose, ConceptRef, Include};
use crate::provider::{CodeSystemProvider, Concept, error::ProviderError};

use super::{ComposeError, Expander, Item, Role, well_formed};

/// What an include's own system says about a code.
enum Contained {
    /// The include names no system, so only its referenced value sets decide.
    NoSystem,
    /// The code is not in this include.
    Refused,
    /// The include contains it, as this item.
    Item(Item),
}

/// Whether `include` admits `concept`: every filter matches, or the code is one
/// of the enumerated ones.
///
/// # Errors
///
/// Returns the provider's failure when a filter cannot be evaluated.
fn admits(
    provider: &Arc<dyn CodeSystemProvider>,
    include: &Include,
    concept: Concept,
    code: &str,
) -> Result<bool, ProviderError> {
    if !include.concepts.is_empty() {
        return Ok(enumerated(provider, include, concept, code)?.is_some());
    }
    // NOTE: an expression enters a filtered selection only where a filter says
    // post-coordination is allowed (<https://hl7.org/fhir/R4B/snomedct.html>,
    // "Filter Properties", the `expressions` filter).
    if provider.is_postcoordinated(concept)
        && !include
            .filters
            .iter()
            .any(|filter| provider.admits_post_coordination(filter))
    {
        return Ok(false);
    }
    for filter in &include.filters {
        if !provider.filter_matches(concept, filter)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether an include's enumerated concepts name `code`.
///
/// A code a system spells differently from the value set (a case-insensitive
/// system, a post-coordinated expression) is compared by what it locates to,
/// because `ValueSet.compose.include.concept` "specifies a concept to be
/// included in the value set"
/// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.include.concept>)
/// and one concept is one member however either side wrote it.
fn enumerated<'i>(
    provider: &Arc<dyn CodeSystemProvider>,
    include: &'i Include,
    concept: Concept,
    code: &str,
) -> Result<Option<&'i ConceptRef>, ProviderError> {
    if let Some(found) = include.concepts.iter().find(|c| c.code == code) {
        return Ok(Some(found));
    }
    for listed in &include.concepts {
        if provider
            .locate(&listed.code)?
            .is_some_and(|located| located.concept == concept)
        {
            return Ok(Some(listed));
        }
    }
    Ok(None)
}

impl Expander<'_> {
    /// Whether `compose` contains `code` of `system` (at `version` when
    /// named), evaluated include by include against the code itself, so a
    /// system that cannot enumerate its codes still validates.
    ///
    /// The item carries the display in `language`. `compose.inactive = false`
    /// excludes an inactive code as it does in an expansion.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] for an invalid compose, an unknown system or
    /// version, or a provider failure.
    pub fn contains(
        &self,
        compose: &Compose,
        system: &str,
        version: Option<&str>,
        code: &str,
        language: Option<&str>,
    ) -> Result<Option<Item>, ComposeError> {
        let mut found = None;
        for include in &compose.include {
            if let Some(item) =
                self.include_contains(include, Role::Include, system, version, code, language)?
            {
                found = Some(item);
                break;
            }
        }
        // NOTE: the ecosystem answers an unresolvable reference by naming it
        // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), so the
        // excludes are walked whether or not an include held the code.
        for exclude in &compose.exclude {
            if self
                .include_contains(exclude, Role::Exclude, system, version, code, language)?
                .is_some()
            {
                return Ok(None);
            }
        }
        let Some(item) = found else {
            return Ok(None);
        };
        if compose.inactive == Some(false) && item.inactive {
            return Ok(None);
        }
        Ok(Some(item))
    }

    /// The item `include` admits for `code` of `system`, if any.
    fn include_contains(
        &self,
        include: &Include,
        role: Role,
        system: &str,
        version: Option<&str>,
        code: &str,
        language: Option<&str>,
    ) -> Result<Option<Item>, ComposeError> {
        well_formed(include)?;
        let mut item = match self.system_contains(include, role, system, version, code, language)? {
            Contained::Refused => return Ok(None),
            Contained::NoSystem => None,
            Contained::Item(item) => Some(item),
        };
        for url in &include.value_sets {
            let resolver = self
                .resolver
                .ok_or_else(|| ComposeError::NoResolver(url.clone()))?;
            match resolver.contains(url, system, code)? {
                Some(referenced) => {
                    if item.is_none() {
                        item = Some(referenced);
                    }
                }
                None => return Ok(None),
            }
        }
        Ok(item)
    }

    /// What the include's own system says about `code`.
    ///
    /// The include's version pattern and the subject's version are matched the
    /// way `$validate-code` matches them: a subject version inside the
    /// include's pattern (`1.0.0` in `1.x.x`) is the version checked, the
    /// include's own otherwise.
    fn system_contains(
        &self,
        include: &Include,
        role: Role,
        system: &str,
        version: Option<&str>,
        code: &str,
        language: Option<&str>,
    ) -> Result<Contained, ComposeError> {
        let Some(named) = &include.system else {
            return Ok(Contained::NoSystem);
        };
        if named.url != system {
            return Ok(Contained::Refused);
        }
        let wanted = match (named.version.as_deref(), version) {
            (Some(pattern), Some(v)) if crate::versioned::version_matches(pattern, v) => Some(v),
            (Some(pattern), _) => Some(pattern),
            (None, v) => v,
        };
        let resolved = self.registry.resolve(&named.url, wanted)?;
        let provider = &resolved.provider;
        let identity = provider.identity();
        // NOTE: an exclude's version says which version its codes are selected
        // from, so it removes the code whatever version an include contributed it at
        // (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.exclude>).
        if role == Role::Include
            && version.is_some_and(|v| !crate::versioned::version_matches(v, &identity.version))
        {
            return Ok(Contained::Refused);
        }
        let failed = |source| ComposeError::Provider {
            system: identity.url.clone(),
            source,
        };
        let Some(located) = provider.locate(code).map_err(failed)? else {
            return Ok(Contained::Refused);
        };
        if !admits(provider, include, located.concept, &located.code).map_err(failed)? {
            return Ok(Contained::Refused);
        }
        let overridden = enumerated(provider, include, located.concept, &located.code)
            .map_err(failed)?
            .and_then(|c| c.display.clone());
        let display = match overridden {
            Some(display) => Some(display),
            None => provider
                .display(
                    located.concept,
                    language::for_provider(provider.as_ref(), language).as_deref(),
                )
                .map_err(failed)?,
        };
        let status = provider.status(located.concept).map_err(failed)?;
        Ok(Contained::Item(Item {
            system: identity.url.clone(),
            version: identity.version.clone(),
            code: located.code,
            display,
            inactive: !status.active,
            abstract_concept: status.abstract_concept,
            depth: 0,
        }))
    }
}
