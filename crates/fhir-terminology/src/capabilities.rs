// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `TerminologyCapabilities` from the loaded providers' declarations
//! (<https://hl7.org/fhir/R4B/terminologycapabilities.html>,
//! <https://hl7.org/fhir/R5/terminologycapabilities.html>).
//!
//! The neutral [`Summary`] is read off the registry once; each FHIR version
//! renders it into its generated type, and the R5-only `codeSystem.content`
//! is a generated difference, not a hand-written conditional.

use crate::artifact::Source;
use crate::filter::FilterOperator;
use crate::provider::{Capability, ContentMode, Declaration, FilterDefinition, ImplicitForm};
use crate::registry::Registry;

/// One filter a version supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterSupport {
    /// The filter code.
    pub code: String,
    /// The operators.
    pub operators: Vec<FilterOperator>,
}

/// One code system version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSummary {
    /// The version string.
    pub code: String,
    /// Whether it is the default.
    pub is_default: bool,
    /// `TerminologyCapabilities.codeSystem.version.compositional`: "If the
    /// compositional grammar defined by the code system is supported"
    /// (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>),
    /// a statement about this server, not about the code system.
    pub compositional: bool,
    /// The designation languages.
    pub languages: Vec<String>,
    /// The filters, one entry per code, the generic one first.
    pub filters: Vec<FilterSupport>,
    /// The `$lookup` properties.
    pub properties: Vec<String>,
    /// The artifact this version was read from, when the server read one.
    ///
    /// An artifact holds one code system version, so the declaration belongs
    /// here and not on the system: two SNOMED CT editions share one
    /// `codeSystem` entry and come from two artifacts.
    pub artifact: Option<Source>,
    /// The implicit value set forms this version resolves.
    pub implicit_forms: Vec<ImplicitForm>,
}

/// The URL of the extension on `TerminologyCapabilities.codeSystem.version`
/// declaring one implicit value set form the version resolves.
///
/// No FHIR/SNOMED spec governs this: our own design. No element of
/// `TerminologyCapabilities` states which implicit value sets a server
/// resolves (<https://hl7.org/fhir/R5/terminologycapabilities.html>), so an
/// extension carries it, one per form.
pub const IMPLICIT_EXTENSION: &str =
    "https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set";

/// The URL of the sub-extension carrying the form's URL template (`valueString`).
pub const IMPLICIT_PATTERN: &str = "pattern";

/// The URL of the sub-extension carrying what the template's placeholders take
/// (`valueCode`: `none`, `code`, or `expression`).
pub const IMPLICIT_ARGUMENT: &str = "argument";

/// One code system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSummary {
    /// The system URI.
    pub url: String,
    /// The versions, sorted.
    pub versions: Vec<VersionSummary>,
    /// The content mode of the served versions.
    pub content: ContentMode,
    /// Whether subsumption is supported.
    pub subsumption: bool,
}

/// What the server can do, version-neutral.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// The code systems, sorted by URI.
    pub systems: Vec<SystemSummary>,
}

/// The generic filters every provider answers on `concept` and `code`.
const GENERIC_OPERATORS: [FilterOperator; 5] = [
    FilterOperator::Equal,
    FilterOperator::In,
    FilterOperator::NotIn,
    FilterOperator::Regex,
    FilterOperator::Exists,
];

/// The hierarchy operators a provider with subsumption answers.
const HIERARCHY_OPERATORS: [FilterOperator; 6] = [
    FilterOperator::IsA,
    FilterOperator::DescendentOf,
    FilterOperator::IsNotA,
    FilterOperator::Generalizes,
    FilterOperator::ChildOf,
    FilterOperator::DescendentLeaf,
];

/// The filter code the statement carries the generic operators under.
const GENERIC_FILTER: &str = "concept";

/// The filter codes the engine answers for every provider: `evaluate` reads
/// `concept` and `code` as the code itself, so a declared entry under either
/// states the generic operators beside its own.
const GENERIC_FILTERS: [&str; 2] = ["concept", "code"];

/// The filters one version declares: the generic entry first, then the
/// provider's own, one entry per filter code.
///
/// `TerminologyCapabilities.codeSystem.version.filter` is "Filter Properties
/// supported" with `code` 1..1 and `op` 1..\* "Operations supported for the
/// property"
/// (<https://hl7.org/fhir/R5/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.filter.op>,
/// the same element in R4 4.0.1, R4B 4.3.0, R5 5.0.0, and the R6 ballot), so
/// one code carries one operator list. No invariant makes the codes distinct,
/// so merging into the union is our own design: the engine answers the generic
/// set and the provider's own for the same code, and a client can send either.
fn declared_filters(declaration: &Declaration) -> Vec<FilterSupport> {
    let mut generic = GENERIC_OPERATORS.to_vec();
    if declaration.capabilities.contains(&Capability::Subsumption) {
        generic.extend(HIERARCHY_OPERATORS);
    }
    let mut filters = vec![FilterSupport {
        code: String::from(GENERIC_FILTER),
        operators: generic.clone(),
    }];
    for declared in &declaration.filters {
        merge(&mut filters, declared, &generic);
    }
    filters
}

/// Adds `declared` to `filters`, into the entry its code already has.
///
/// A new entry under a filter code the engine answers generically starts from
/// `generic`, which `evaluate` answers there whatever the system declares.
fn merge(
    filters: &mut Vec<FilterSupport>,
    declared: &FilterDefinition,
    generic: &[FilterOperator],
) {
    for support in &mut *filters {
        if support.code == declared.code {
            add(&mut support.operators, &declared.operators);
            return;
        }
    }
    let mut operators = if GENERIC_FILTERS.contains(&declared.code.as_str()) {
        generic.to_vec()
    } else {
        Vec::new()
    };
    add(&mut operators, &declared.operators);
    filters.push(FilterSupport {
        code: declared.code.clone(),
        operators,
    });
}

/// Appends the operators of `added` that `operators` does not already carry.
fn add(operators: &mut Vec<FilterOperator>, added: &[FilterOperator]) {
    for operator in added {
        if !operators.contains(operator) {
            operators.push(*operator);
        }
    }
}

/// The operators one filter entry states on an R4-family surface.
///
/// `filter-operator` 4.0.1 and 4.3.0 define nine operators, and `child-of` and
/// `descendent-leaf` arrive in 5.0.0.
/// `ValueSet.compose.include.filter.op` is required-bound to the version's own
/// value set (`filter-operator|4.3.0`,
/// <https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.include.filter.op>),
/// so a client taking either operator from an R4 or R4B statement would write
/// a resource that version refuses.
fn r4_family_operators(operators: &[FilterOperator]) -> Vec<&'static str> {
    operators
        .iter()
        .filter(|operator| !operator.r5_only())
        .map(|operator| operator.code())
        .collect()
}

/// The operators one filter entry states on an R5-family surface: every
/// operator the engine evaluates.
///
/// `filter-operator` 6.0.0-ballot5 adds a twelfth, `property-value-of`
/// (<https://hl7.org/fhir/6.0.0-ballot5/codesystem-filter-operator.html>),
/// which the engine does not evaluate, so the R6 statement stays at eleven.
fn r5_family_operators(operators: &[FilterOperator]) -> Vec<&'static str> {
    operators.iter().map(|operator| operator.code()).collect()
}

impl Summary {
    /// Reads the summary off `registry`.
    #[must_use]
    pub fn of(registry: &Registry) -> Self {
        let mut systems = Vec::new();
        for url in registry.systems() {
            let default = registry.default_version(url);
            let mut versions = Vec::new();
            let mut content = ContentMode::NotPresent;
            let mut subsumption = false;
            for provider in registry.versions(url) {
                let identity = provider.identity();
                let declaration = provider.declaration();
                content = declaration.content;
                subsumption |= declaration.capabilities.contains(&Capability::Subsumption);
                versions.push(VersionSummary {
                    code: identity.version.clone(),
                    is_default: default == Some(identity.version.as_str()),
                    compositional: declaration.compositional.supported(),
                    languages: declaration.languages.clone(),
                    filters: declared_filters(declaration),
                    properties: declaration
                        .properties
                        .iter()
                        .map(|property| property.code.clone())
                        .collect(),
                    artifact: provider.artifact().cloned(),
                    implicit_forms: declaration.implicit_forms.clone(),
                });
            }
            systems.push(SystemSummary {
                url: url.to_owned(),
                versions,
                content,
                subsumption,
            });
        }
        Self { systems }
    }
}

// NOTE: an extension carries "additional information that is not part of the
// basic definition" (<https://hl7.org/fhir/R4B/extensibility.html>), and every
// version's `codeSystem.version` is a `BackboneElement` that admits one.
macro_rules! artifact_extension {
    ($module:ident) => {
        /// The artifact declaration of one FHIR version.
        mod $module {
            use fhir_types::$module::extension::{Extension, ExtensionValue};

            use crate::artifact::{SOURCE_EXTENSION, SOURCE_NAME, SOURCE_RELEASE, Source};

            /// The artifact extension of `source`; nothing when the version was
            /// read from no artifact.
            pub(super) fn declaration(source: Option<&Source>) -> Vec<Extension> {
                source
                    .map(|source| Extension {
                        url: SOURCE_EXTENSION.to_owned(),
                        extension: source
                            .name
                            .iter()
                            .map(|name| sub(SOURCE_NAME, name))
                            .chain(std::iter::once(sub(SOURCE_RELEASE, &source.release)))
                            .collect(),
                        ..Default::default()
                    })
                    .into_iter()
                    .collect()
            }

            /// One sub-extension with a `string` value.
            fn sub(url: &str, value: &str) -> Extension {
                Extension {
                    url: url.to_owned(),
                    value: Some(ExtensionValue::String(value.into())),
                    ..Default::default()
                }
            }
        }
    };
}

artifact_extension!(r4);
artifact_extension!(r4b);
artifact_extension!(r5);
artifact_extension!(r6);

macro_rules! implicit_extension {
    ($module:ident) => {
        /// The implicit value set declaration of one FHIR version.
        pub(super) mod $module {
            use fhir_types::$module::extension::{Extension, ExtensionValue};

            use super::super::{IMPLICIT_ARGUMENT, IMPLICIT_EXTENSION, IMPLICIT_PATTERN};
            use crate::provider::ImplicitForm;

            /// One extension per form, in declaration order.
            pub(in crate::capabilities) fn declaration(forms: &[ImplicitForm]) -> Vec<Extension> {
                forms
                    .iter()
                    .map(|form| Extension {
                        url: IMPLICIT_EXTENSION.to_owned(),
                        extension: vec![
                            Extension {
                                url: IMPLICIT_PATTERN.to_owned(),
                                value: Some(ExtensionValue::String(form.pattern.as_str().into())),
                                ..Default::default()
                            },
                            Extension {
                                url: IMPLICIT_ARGUMENT.to_owned(),
                                value: Some(ExtensionValue::Code(form.argument.code().into())),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    })
                    .collect()
            }
        }
    };
}

/// The implicit value set extensions, one module per FHIR version.
mod implicit {
    implicit_extension!(r4);
    implicit_extension!(r4b);
    implicit_extension!(r5);
    implicit_extension!(r6);
}

/// The extensions of one `codeSystem.version` entry in the version `$module`:
/// the artifact first, then one per implicit value set form.
macro_rules! version_extensions {
    ($module:ident, $version:expr) => {{
        let mut extensions = self::$module::declaration($version.artifact.as_ref());
        extensions.extend(self::implicit::$module::declaration(
            &$version.implicit_forms,
        ));
        extensions
    }};
}

// NOTE: <https://hl7.org/fhir/6.0.0-ballot5/terminologycapabilities.html>: R6 differs from
// R5 only in an optional `codeSystem.content` and `version.value` for `version.code`, so
// one macro with a flavour arm renders both.
macro_rules! r5_family_capabilities {
    ($module:ident, $name:ident, $flavour:ident) => {
        impl Summary {
            /// The resource of the version; `date` is the statement's `dateTime`.
            #[must_use]
            pub fn $name(
                &self,
                date: &str,
            ) -> fhir_types::$module::terminology_capabilities::TerminologyCapabilities {
                use fhir_types::$module::terminology_capabilities::{
                    TerminologyCapabilities, TerminologyCapabilitiesCodeSystem,
                    TerminologyCapabilitiesCodeSystemVersion,
                    TerminologyCapabilitiesCodeSystemVersionFilter, TerminologyCapabilitiesExpansion,
                    TerminologyCapabilitiesExpansionParameter,
                };
                let version_entry = |version: &VersionSummary| {
                    let mut entry = TerminologyCapabilitiesCodeSystemVersion {
                        extension: version_extensions!($module, version),
                        is_default: Some(version.is_default.into()),
                        compositional: Some(version.compositional.into()),
                        language: common_languages(&version.languages)
                            .into_iter()
                            .map(Into::into)
                            .collect(),
                        filter: version
                            .filters
                            .iter()
                            .filter_map(|filter| {
                                let ops = r5_family_operators(&filter.operators);
                                // `filter.op` is 1..*, so an entry left with
                                // none is left out.
                                (!ops.is_empty()).then(|| {
                                    TerminologyCapabilitiesCodeSystemVersionFilter {
                                        code: filter.code.as_str().into(),
                                        op: ops.into_iter().map(Into::into).collect(),
                                        ..Default::default()
                                    }
                                })
                            })
                            .collect(),
                        property: version
                            .properties
                            .iter()
                            .map(|p| p.as_str().into())
                            .collect(),
                        ..Default::default()
                    };
                    // NOTE: a FHIR `string` is never empty (<https://hl7.org/fhir/R4B/datatypes.html#string>),
                    // so a system without a version declares no `version.code`.
                    if !version.code.is_empty() {
                        r5_family_capabilities!(@version_code $flavour, entry, version.code.as_str().into());
                    }
                    entry
                };
                TerminologyCapabilities {
                    status: "active".into(),
                    date: date.into(),
                    kind: "instance".into(),
                    code_system: self
                        .systems
                        .iter()
                        .map(|system| TerminologyCapabilitiesCodeSystem {
                            uri: Some(system.url.as_str().into()),
                            version: system.versions.iter().map(version_entry).collect(),
                            content: r5_family_capabilities!(@content $flavour, system.content.code().into()),
                            subsumption: Some(system.subsumption.into()),
                            ..Default::default()
                        })
                        .collect(),
                    expansion: Some(TerminologyCapabilitiesExpansion {
                        hierarchical: Some(false.into()),
                        paging: Some(true.into()),
                        incomplete: Some(false.into()),
                        text_filter: Some(TEXT_FILTER.into()),
                        parameter: expansion_parameters(
                            &fhir_types::$module::operations::value_set_expand::VALUE_SET_EXPAND,
                        )
                        .into_iter()
                        .map(|name| TerminologyCapabilitiesExpansionParameter {
                            name: name.into(),
                            ..Default::default()
                        })
                        .collect(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }
        }
    };
    (@content r5, $value:expr) => {
        $value
    };
    (@content r6, $value:expr) => {
        Some($value)
    };
    (@version_code r5, $entry:ident, $value:expr) => {
        $entry.code = Some($value);
    };
    (@version_code r6, $entry:ident, $value:expr) => {
        $entry.value = Some($value);
    };
}

r5_family_capabilities!(r5, to_r5, r5);
r5_family_capabilities!(r6, to_r6, r6);

// NOTE: R4 (4.0.1) and R4B (4.3.0) declare the same TerminologyCapabilities
// elements this render fills, so one macro produces both methods.
macro_rules! terminology_capabilities {
    ($module:ident, $name:ident) => {
        impl Summary {
            /// The resource of the version; `date` is the statement's `dateTime`.
            #[must_use]
            pub fn $name(
                &self,
                date: &str,
            ) -> fhir_types::$module::terminology_capabilities::TerminologyCapabilities {
                use fhir_types::$module::terminology_capabilities::{
                    TerminologyCapabilities, TerminologyCapabilitiesCodeSystem,
                    TerminologyCapabilitiesCodeSystemVersion,
                    TerminologyCapabilitiesCodeSystemVersionFilter,
                    TerminologyCapabilitiesExpansion, TerminologyCapabilitiesExpansionParameter,
                };
                TerminologyCapabilities {
                    status: "active".into(),
                    date: date.into(),
                    kind: "instance".into(),
                    code_system: self
                        .systems
                        .iter()
                        .map(|system| TerminologyCapabilitiesCodeSystem {
                            uri: Some(system.url.as_str().into()),
                            version: system
                                .versions
                                .iter()
                                .map(|version| TerminologyCapabilitiesCodeSystemVersion {
                                    extension: version_extensions!($module, version),
                                    code: (!version.code.is_empty())
                                        .then(|| version.code.as_str().into()),
                                    is_default: Some(version.is_default.into()),
                                    compositional: Some(version.compositional.into()),
                                    language: common_languages(&version.languages)
                                        .into_iter()
                                        .map(Into::into)
                                        .collect(),
                                    filter: version
                                        .filters
                                        .iter()
                                        .filter_map(|filter| {
                                            let ops = r4_family_operators(&filter.operators);
                                            // `filter.op` is 1..*, so an entry
                                            // left with none is left out.
                                            (!ops.is_empty()).then(|| {
                                                TerminologyCapabilitiesCodeSystemVersionFilter {
                                                    code: filter.code.as_str().into(),
                                                    op: ops.into_iter().map(Into::into).collect(),
                                                    ..Default::default()
                                                }
                                            })
                                        })
                                        .collect(),
                                    property: version
                                        .properties
                                        .iter()
                                        .map(|p| p.as_str().into())
                                        .collect(),
                                    ..Default::default()
                                })
                                .collect(),
                            subsumption: Some(system.subsumption.into()),
                            ..Default::default()
                        })
                        .collect(),
                    expansion: Some(TerminologyCapabilitiesExpansion {
                        hierarchical: Some(false.into()),
                        paging: Some(true.into()),
                        incomplete: Some(false.into()),
                        text_filter: Some(TEXT_FILTER.into()),
                        parameter: expansion_parameters(
                            &fhir_types::$module::operations::value_set_expand::VALUE_SET_EXPAND,
                        )
                        .into_iter()
                        .map(|name| TerminologyCapabilitiesExpansionParameter {
                            name: name.into(),
                            ..Default::default()
                        })
                        .collect(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }
        }
    };
}

terminology_capabilities!(r4, to_r4);
terminology_capabilities!(r4b, to_r4b);

/// The `$expand` parameters a version advertises in `expansion.parameter`:
/// the names of [`EXPANSION_PARAMETERS`] its generated `ValueSet/$expand`
/// contract declares as `in` parameters, plus the resource parameters, sorted.
/// A version never advertises a parameter its endpoint refuses.
fn expansion_parameters(operation: &fhir_types::operation::Operation) -> Vec<&'static str> {
    let mut names: Vec<&str> = operation
        .parameters
        .iter()
        .filter(|parameter| {
            parameter.usage == fhir_types::operation::ParameterUse::In
                && EXPANSION_PARAMETERS.contains(&parameter.name)
        })
        .map(|parameter| parameter.name)
        .chain(RESOURCE_PARAMETERS)
        .collect();
    names.sort_unstable();
    names
}

/// The `$expand` parameters the terminology ecosystem carries as resources in
/// the request `Parameters`, outside every version's `OperationDefinition`
/// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
const RESOURCE_PARAMETERS: [&str; 1] = ["tx-resource"];

/// The `$expand` parameters the server evaluates, for
/// `expansion.parameter`; `tx-resource` is the terminology ecosystem's
/// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
pub const EXPANSION_PARAMETERS: [&str; 17] = [
    "activeOnly",
    "check-system-version",
    "count",
    "designation",
    "displayLanguage",
    "exclude-system",
    "excludeNested",
    "excludeNotForUI",
    "excludePostCoordinated",
    "filter",
    "force-system-version",
    "includeDefinition",
    "includeDesignations",
    "offset",
    "property",
    "system-version",
    "tx-resource",
];

/// `expansion.textFilter`: how the `filter` parameter matches.
const TEXT_FILTER: &str = "Each word of the filter is a prefix that must match the start of a word \
in one designation of the concept, in any order. Matching ignores case and diacritics. No wild cards.";

/// The codes of the FHIR `CommonLanguages` value set
/// (<http://hl7.org/fhir/ValueSet/languages>, R4B 4.3.0).
const COMMON_LANGUAGES: [&str; 56] = [
    "ar", "bn", "cs", "da", "de", "de-AT", "de-CH", "de-DE", "el", "en", "en-AU", "en-CA", "en-GB",
    "en-IN", "en-NZ", "en-SG", "en-US", "es", "es-AR", "es-ES", "es-UY", "fi", "fr", "fr-BE",
    "fr-CH", "fr-FR", "fy", "fy-NL", "hi", "hr", "it", "it-CH", "it-IT", "ja", "ko", "nl", "nl-BE",
    "nl-NL", "no", "no-NO", "pa", "pl", "pt", "pt-BR", "ru", "ru-RU", "sr", "sr-RS", "sv", "sv-SE",
    "te", "zh", "zh-CN", "zh-HK", "zh-SG", "zh-TW",
];

/// The designation languages as `CommonLanguages` codes: a tag in the value
/// set as it is, another by its primary subtag when that is, the rest left
/// out, without repeats.
///
/// R4B binds `TerminologyCapabilities.codeSystem.version.language` to
/// nothing (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html>),
/// but R5 binds it to `CommonLanguages` (required) and the FHIR validator
/// converts the resource to R5 before reading it, so a tag outside the set
/// fails the terminology test runner. `$lookup` designations keep every tag.
#[must_use]
pub fn common_languages(tags: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for tag in tags {
        let primary = tag.split(['-', '_']).next().unwrap_or(tag);
        let code = COMMON_LANGUAGES
            .iter()
            .find(|c| c.eq_ignore_ascii_case(tag))
            .or_else(|| {
                COMMON_LANGUAGES
                    .iter()
                    .find(|c| c.eq_ignore_ascii_case(primary))
            });
        if let Some(code) = code
            && !out.contains(code)
        {
            out.push(code);
        }
    }
    out
}

#[cfg(test)]
mod common_language_tests {
    use super::common_languages;

    #[test]
    fn common_languages_fold_to_the_value_set() {
        let tags: Vec<String> = ["en", "nl-NL", "ar-JO", "cs-CZ", "et-EE", "EN-gb", "zh-CN"]
            .iter()
            .map(|t| (*t).to_owned())
            .collect();
        assert_eq!(
            common_languages(&tags),
            ["en", "nl-NL", "ar", "cs", "en-GB", "zh-CN"],
            "ar-JO and cs-CZ fold to their primary subtag; et has no code"
        );
    }
}
