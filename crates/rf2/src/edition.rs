// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! Edition identification from the module dependency reference set.
//!
//! A release assembles modules; each module dependency member says that its
//! module, at `sourceEffectiveTime`, depends on the referenced module at
//! `targetEffectiveTime`. The edition is identified by a module, and its
//! version URI is `http://snomed.info/sct/[moduleId]/version/[YYYYMMDD]`
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard>).

use std::collections::{BTreeMap, BTreeSet};

use crate::constants;
use crate::id::{ConceptId, ModuleId};
use crate::refset::ModuleDependencyMember;
use crate::time::EffectiveTime;

/// The edition could not be identified.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditionError {
    /// No active module dependency member was given.
    #[error("no active module dependency members")]
    NoDependencies,
    /// A member names a target module that is not a concept identifier.
    #[error("module dependency member {member} names {target}, which is not a concept identifier")]
    MalformedTarget {
        /// The member.
        member: String,
        /// The offending target.
        target: String,
    },
    /// Several modules depend on others without being depended on, and none
    /// of them carries the release date.
    #[error("several root modules and none carries the release date {release}: {roots:?}")]
    AmbiguousRoot {
        /// The release date.
        release: String,
        /// The candidate modules.
        roots: Vec<String>,
    },
}

/// The identified edition of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edition {
    /// The edition's module: the root module carrying the release date.
    pub module: ModuleId,
    /// Other root modules the release ships beside the edition module (a
    /// sibling such as a mapping module), which the edition URI does not cover.
    pub sibling_roots: Vec<ModuleId>,
    /// The edition's version.
    pub effective_time: EffectiveTime,
    /// Every module in the release with the version it is at.
    pub modules: BTreeMap<ModuleId, EffectiveTime>,
}

impl Edition {
    /// Identifies the edition from the active module dependency members.
    ///
    /// The root modules are those that depend on others and are depended on
    /// by none. An extension's focus module is a root, so a root carrying a
    /// namespace identifies the edition; with several such roots the one whose
    /// `sourceEffectiveTime` equals `release` does, and the others are
    /// reported as `sibling_roots`.
    ///
    /// A release with no namespaced root is the International Edition, whose
    /// module the URI Standard names as
    /// `900000000000207008 |SNOMED CT core module|` (§2 SNOMED CT URI Space,
    /// Table 2.1, and the §3 table)
    /// (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard>).
    /// The graph alone does not give that answer: the International release
    /// ships `449080006 |SNOMED CT to ICD-10 rule-based mapping module|`,
    /// which depends on the core module and on the model component module and
    /// is depended on by neither, so it is the only root and it is not the
    /// edition.
    ///
    /// The specifications define an edition by its focus module, the module
    /// dependent on every other module
    /// (<https://docs.snomed.org/snomed-ct-practical-guides/snomed-ct-extension-guide/4-logical-design/4.4-editions>),
    /// and give no consumer-side rule for a release with several namespaced
    /// roots; the date rule is our own design for packages such as the NL
    /// edition, which ships the mapping module as a sibling root.
    ///
    /// # Errors
    ///
    /// Returns [`EditionError`] when no members are given, a target is not a
    /// concept identifier, or the root is ambiguous.
    pub fn identify(
        members: &[ModuleDependencyMember],
        release: EffectiveTime,
    ) -> Result<Self, EditionError> {
        let active: Vec<&ModuleDependencyMember> =
            members.iter().filter(|m| m.member.active).collect();
        if active.is_empty() {
            return Err(EditionError::NoDependencies);
        }
        let mut modules: BTreeMap<ModuleId, EffectiveTime> = BTreeMap::new();
        let mut depended_on: BTreeSet<ModuleId> = BTreeSet::new();
        for member in &active {
            let source = member.member.module_id;
            modules
                .entry(source)
                .and_modify(|t| *t = (*t).max(member.source_effective_time))
                .or_insert(member.source_effective_time);
            // The error names the member and its target; the id error adds nothing.
            let Ok(target) =
                ConceptId::try_from(member.member.referenced_component_id).map(ModuleId::from)
            else {
                return Err(EditionError::MalformedTarget {
                    member: member.member.id.to_string(),
                    target: member.member.referenced_component_id.to_string(),
                });
            };
            depended_on.insert(target);
            modules
                .entry(target)
                .and_modify(|t| *t = (*t).max(member.target_effective_time))
                .or_insert(member.target_effective_time);
        }
        let roots: Vec<ModuleId> = modules
            .keys()
            .filter(|m| !depended_on.contains(*m))
            .copied()
            .collect();
        // A namespaced root is an extension's focus module. Without one the
        // release is the International Edition, which the URI Standard names
        // by its core module rather than by the graph.
        let namespaced: Vec<ModuleId> = roots
            .iter()
            .filter(|m| m.concept().sctid().namespace().is_some())
            .copied()
            .collect();
        let module = match namespaced.as_slice() {
            [only] => *only,
            [] => {
                let core = ModuleId::from(constants::CORE_MODULE);
                if modules.contains_key(&core) {
                    core
                } else {
                    match roots.as_slice() {
                        [only] => *only,
                        many => {
                            return Err(EditionError::AmbiguousRoot {
                                release: release.compact(),
                                roots: many.iter().map(ToString::to_string).collect(),
                            });
                        }
                    }
                }
            }
            many => *many
                .iter()
                .find(|m| modules.get(*m) == Some(&release))
                .ok_or_else(|| EditionError::AmbiguousRoot {
                    release: release.compact(),
                    roots: many.iter().map(ToString::to_string).collect(),
                })?,
        };
        let effective_time = modules.get(&module).copied().unwrap_or(release);
        let sibling_roots = roots.into_iter().filter(|m| *m != module).collect();
        Ok(Self {
            module,
            sibling_roots,
            effective_time,
            modules,
        })
    }

    /// The edition URI, `http://snomed.info/sct/[moduleId]`.
    #[must_use]
    pub fn edition_uri(&self) -> String {
        format!("http://snomed.info/sct/{}", self.module)
    }

    /// The version URI, `http://snomed.info/sct/[moduleId]/version/[YYYYMMDD]`.
    #[must_use]
    pub fn version_uri(&self) -> String {
        format!(
            "{}/version/{}",
            self.edition_uri(),
            self.effective_time.compact()
        )
    }
}
