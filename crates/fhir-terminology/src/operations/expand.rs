// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet/$expand` in the terms every served FHIR version shares.
//!
//! The operation pages: <https://hl7.org/fhir/R4B/valueset-operation-expand.html>
//! and <https://hl7.org/fhir/R5/valueset-operation-expand.html>. The value set
//! is inline, stored, or implicit; its compose is pinned by the version
//! parameters, expanded as bitmap algebra, paged, and the page is read into
//! neutral items with their designations; the wire layer of each version
//! renders the `ValueSet` resource with its `expansion`.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::{OperationError, Sources};
use crate::compose::{Compose, Expansion, Include, Item, Options};
use crate::provider::{Designation, Property, PropertyValue};
use crate::valueset::model::{ModelError, ValueSetModel};
use crate::valueset::negotiation::{Negotiation, canonicals};
use crate::valueset::store::Resolver;
use crate::versioned::Versioned;

/// The most concepts an unpaged expansion returns before it is `too-costly`.
///
/// No specification names the limit; the ecosystem's servers refuse whole
/// expansions of the large systems, and a client pages with `count`.
pub const EXPANSION_LIMIT: u64 = 1000;

/// The input of `$expand`: the union of the parameters the served versions
/// declare.
#[derive(Debug, Default, Clone)]
pub struct ExpandInput {
    /// The names of parameters the version declares that the server does not
    /// implement; a request naming one is refused, never absorbed.
    pub unsupported: Vec<String>,
    /// The value set URL (`url`).
    pub url: Option<String>,
    /// The value set version (`valueSetVersion`).
    pub value_set_version: Option<String>,
    /// The inline `valueSet`, converted by the wire layer of its version.
    pub inline_value_set: Option<Result<ValueSetModel, ModelError>>,
    /// Whether `context` or `contextDirection` was given; not supported.
    pub context: bool,
    /// Whether `date` was given; not supported.
    pub date: bool,
    /// The text filter.
    pub filter: Option<String>,
    /// The offset into the expansion.
    pub offset: Option<i64>,
    /// The page size.
    pub count: Option<i64>,
    /// `includeDesignations`.
    pub include_designations: Option<bool>,
    /// `designation`: the languages or `system|code` uses to include.
    pub designation: Vec<String>,
    /// `includeDefinition`.
    pub include_definition: Option<bool>,
    /// `activeOnly`.
    pub active_only: Option<bool>,
    /// `excludeNested`.
    pub exclude_nested: Option<bool>,
    /// `excludeNotForUI`.
    pub exclude_not_for_ui: Option<bool>,
    /// `excludePostCoordinated`.
    pub exclude_post_coordinated: Option<bool>,
    /// The language of the displays (a BCP 47 range list).
    pub display_language: Option<String>,
    /// `exclude-system` canonicals.
    pub exclude_system: Vec<String>,
    /// `system-version` canonicals.
    pub system_version: Vec<String>,
    /// `check-system-version` canonicals.
    pub check_system_version: Vec<String>,
    /// `force-system-version` canonicals.
    pub force_system_version: Vec<String>,
    /// `default-valueset-version` canonicals: the version of a value set
    /// reference that names none (pre-adopted from R6).
    pub default_valueset_version: Vec<String>,
    /// `check-valueset-version` canonicals: a version a differing value set
    /// reference is refused against (pre-adopted from R6).
    pub check_valueset_version: Vec<String>,
    /// `force-valueset-version` canonicals: a version that overrides the value
    /// set reference's (pre-adopted from R6).
    pub force_valueset_version: Vec<String>,
    /// `property` (R5): the properties to include on each concept.
    pub property: Vec<String>,
    /// `useSupplement` (R5): the supplements to apply.
    pub use_supplement: Vec<String>,
    /// `handle-unclosed-expansion` (R6): whether the client handles an
    /// unclosed expansion. `false` refuses one
    /// (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
    pub handle_unclosed_expansion: Option<bool>,
}

/// One echoed `expansion.parameter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionParameter {
    /// The parameter name.
    pub name: String,
    /// The value.
    pub value: ParameterValue,
}

/// The value of an echoed expansion parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterValue {
    /// A `string`.
    String(String),
    /// A `boolean`.
    Boolean(bool),
    /// An `integer`.
    Integer(i64),
    /// A `code`.
    Code(String),
    /// A `uri`.
    Uri(String),
}

/// One concept of the expansion page.
#[expect(
    clippy::struct_field_names,
    reason = "`contains.contains` is the FHIR element's own name"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contains {
    /// The code system URI.
    pub system: String,
    /// The code system version served.
    pub version: String,
    /// The code.
    pub code: String,
    /// The display in the requested language.
    pub display: Option<String>,
    /// Whether the concept is abstract (not selectable).
    pub abstract_concept: bool,
    /// Whether the concept is inactive.
    pub inactive: bool,
    /// The designations asked for.
    pub designations: Vec<Designation>,
    /// The properties asked for with `property`
    /// (<https://hl7.org/fhir/R5/valueset-operation-expand.html>).
    pub properties: Vec<Property>,
    /// The entry's children, when the expansion carries the system's hierarchy
    /// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>).
    pub contains: Vec<Contains>,
}

/// One property the expansion returns on its concepts, for
/// `expansion.property` (<https://hl7.org/fhir/R5/valueset.html>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionProperty {
    /// The property code, as `contains.property.code` names it.
    pub code: String,
    /// The formal URI of the property, when the code system declares one.
    pub uri: Option<String>,
}

/// The outcome of `$expand`.
#[derive(Debug, Clone)]
pub struct ExpansionOutcome {
    /// The value set expanded (its definition, for the resource).
    pub model: Arc<ValueSetModel>,
    /// Whether the compose is to be returned (`includeDefinition`).
    pub include_definition: bool,
    /// The expansion identifier (`urn:uuid:…`).
    pub identifier: String,
    /// The timestamp of the expansion.
    pub timestamp: String,
    /// The size of the whole expansion.
    pub total: u64,
    /// The offset, when the client paged.
    pub offset: Option<u64>,
    /// The parameters echoed, in order.
    pub parameters: Vec<ExpansionParameter>,
    /// The page.
    pub contains: Vec<Contains>,
    /// The properties the concepts carry, each once, in order of appearance.
    pub properties: Vec<ExpansionProperty>,
    /// Whether the value set admits codes the expansion does not list, which
    /// the resource states with `valueset-unclosed`
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes).
    pub unclosed: bool,
    /// Why the expansion is unclosed, one reason per cause, for
    /// `valueset-unclosed-reason`.
    pub unclosed_reasons: Vec<String>,
}

/// Runs `$expand`.
///
/// # Errors
///
/// Returns [`OperationError`] for a value set that cannot be found or is
/// invalid, a parameter the operation does not support, a version pin the
/// value set contradicts, a negative page, an unpaged expansion beyond
/// [`EXPANSION_LIMIT`], or a provider failure.
pub fn expand(
    sources: &Sources<'_>,
    input: &ExpandInput,
) -> Result<ExpansionOutcome, OperationError> {
    crate::language::check(input.display_language.as_deref())?;
    refuse_unsupported(input)?;
    let negotiation = negotiation(input);
    let (url, version) = match input.url.as_deref() {
        Some(url) => {
            let (url, version) = negotiation.value_set(url, input.value_set_version.as_deref())?;
            (Some(url), version)
        }
        None => (None, input.value_set_version.clone()),
    };
    let model = sources.value_set(
        input.inline_value_set.clone(),
        url.as_deref(),
        version.as_deref(),
    )?;
    let input = &with_defaults(input, &model);
    let compose = pinned_compose(&model.compose, input, &negotiation);
    let options = options(input)?;
    let mut wanted = input.use_supplement.clone();
    wanted.extend(model.supplements.iter().cloned());
    let used_supplements = sources.registry.applied_supplements(&wanted)?;
    let registry = sources.with_supplements(&wanted)?;
    let sources = &Sources {
        registry: &registry,
        ..*sources
    };
    let resolver = Resolver::new(sources.registry, sources.value_sets)
        .with_negotiation(&negotiation)
        .with_contained(&model.contained)
        .with_language(options.language.as_deref());
    resolver.note_open_systems(&model.compose);
    let inline = input.inline_value_set.is_some().then_some(&model.compose);
    let expansion = resolver
        .expand_compose(&model.canonical(), &compose, &options)
        .map_err(|error| OperationError::of_compose(error, inline))?;
    // The check runs on the version the include resolved to, never on the one
    // the value set wrote. A value set may name a pattern (`1.x.x`), and a
    // version that resolves to nothing is unknown rather than checked, so the
    // resolution has to answer first (#506).
    for used in &expansion.versions {
        negotiation.check_system(&used.url, &used.version)?;
    }
    let used_value_sets = resolver.used_value_sets();
    let open_systems = resolver.open_systems();
    if expansion.unclosed && input.handle_unclosed_expansion == Some(false) {
        return Err(OperationError::NotSupported(format!(
            "the expansion of `{}` is unclosed, and `handle-unclosed-expansion` is false",
            model.canonical()
        )));
    }
    if options.count.is_none() && expansion.total > EXPANSION_LIMIT {
        return Err(OperationError::TooCostly(format!(
            "the expansion of `{}` has {} concepts; page it with `count` (and `offset`) to fetch it",
            model.canonical(),
            expansion.total
        )));
    }
    let contains = contains(sources, &expansion, input)?;
    let properties = expansion_properties(sources, &contains)?;
    // The message says what is wrong; the conversion error adds nothing to it.
    let Ok(offset) = u64::try_from(expansion.offset) else {
        return Err(OperationError::Invalid(String::from(
            "`offset` is too large",
        )));
    };
    Ok(ExpansionOutcome {
        include_definition: input.include_definition.unwrap_or(false),
        identifier: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        timestamp: crate::clock::now().to_string(),
        total: expansion.total,
        offset: (input.offset.is_some() || input.count.is_some()).then_some(offset),
        parameters: {
            let mut out = parameters(
                input,
                &expansion,
                &used_value_sets,
                &used_supplements,
                &open_systems,
            );
            out.extend(warnings(sources, &model, &expansion, &used_value_sets));
            out
        },
        contains,
        properties,
        unclosed_reasons: unclosed_reasons(&expansion),
        unclosed: expansion.unclosed,
        model,
    })
}

/// Why the expansion is unclosed: one reason per fragment it drew on, then
/// what each system said for itself.
///
/// The R6 Notes name both among the reasons a server returns an unclosed
/// expansion ("e.g. fragment, post-coordinated concepts, code systems with a
/// grammar, size",
/// <https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
// NOTE: no published `StructureDefinition` defines `valueset-unclosed-reason`, so the
// wording is the ecosystem suite's, its `fragment` case for a fragment and its
// `langcodes` case for a grammar (<https://github.com/HL7/fhir-tx-ecosystem-ig>).
fn unclosed_reasons(expansion: &Expansion) -> Vec<String> {
    if !expansion.unclosed {
        return Vec::new();
    }
    expansion
        .fragments
        .iter()
        .map(|fragment| {
            format!(
                "This extension is based on a fragment of the code system {}",
                fragment.url
            )
        })
        .chain(expansion.unclosed_reasons.iter().cloned())
        .collect()
}

/// The request with the value set's default expansion parameters filled in
/// where the client named none (`valueset-expansion-parameter`,
/// <https://hl7.org/fhir/R4B/extension-valueset-expansion-parameter.html>).
fn with_defaults(input: &ExpandInput, model: &ValueSetModel) -> ExpandInput {
    let mut input = input.clone();
    for default in &model.expansion_parameters {
        match default.name.as_str() {
            "displayLanguage" if input.display_language.is_none() => {
                input.display_language = Some(default.value.clone());
            }
            "activeOnly" if input.active_only.is_none() => {
                input.active_only = Some(default.value == "true");
            }
            "excludeNested" if input.exclude_nested.is_none() => {
                input.exclude_nested = Some(default.value == "true");
            }
            "includeDesignations" if input.include_designations.is_none() => {
                input.include_designations = Some(default.value == "true");
            }
            _ => {}
        }
    }
    input
}

fn refuse_unsupported(input: &ExpandInput) -> Result<(), OperationError> {
    if let Some(name) = input.unsupported.first() {
        return Err(OperationError::NotSupported(format!(
            "`{name}` is not supported by this server"
        )));
    }
    if input.context {
        return Err(OperationError::NotSupported(String::from(
            "`context` and `contextDirection` are not supported; name the value set with `url` or `valueSet`",
        )));
    }
    if input.date {
        return Err(OperationError::NotSupported(String::from(
            "`date` is not supported: expansions are generated from the versions served now",
        )));
    }
    Ok(())
}

fn options(input: &ExpandInput) -> Result<Options, OperationError> {
    let non_negative = |value: Option<i64>, name: &str| -> Result<Option<usize>, OperationError> {
        value
            .map(|v| {
                let Ok(value) = usize::try_from(v) else {
                    return Err(OperationError::Invalid(format!(
                        "`{name}` must not be negative"
                    )));
                };
                Ok(value)
            })
            .transpose()
    };
    Ok(Options {
        active_only: input.active_only.unwrap_or(false),
        exclude_nested: input.exclude_nested.unwrap_or(false),
        exclude_not_for_ui: input.exclude_not_for_ui.unwrap_or(false),
        exclude_post_coordinated: input.exclude_post_coordinated.unwrap_or(false),
        text: input.filter.clone(),
        language: input.display_language.clone(),
        offset: non_negative(input.offset, "offset")?.unwrap_or(0),
        count: non_negative(input.count, "count")?,
    })
}

/// The version negotiation the request asks for.
fn negotiation(input: &ExpandInput) -> Negotiation {
    Negotiation::new(
        &input.system_version,
        &input.check_system_version,
        &input.force_system_version,
        &input.default_valueset_version,
        &input.check_valueset_version,
        &input.force_valueset_version,
    )
}

/// `compose` less the `exclude-system` includes, with its systems at their
/// negotiated versions.
fn pinned_compose(compose: &Compose, input: &ExpandInput, negotiation: &Negotiation) -> Compose {
    let excluded = canonicals(&input.exclude_system);
    let keep = |include: &Include| -> bool {
        let Some(system) = &include.system else {
            return true;
        };
        !excluded.iter().any(|(url, version)| {
            *url == system.url
                && version
                    .as_ref()
                    .is_none_or(|v| Some(v) == system.version.as_ref())
        })
    };
    let kept = Compose {
        include: compose
            .include
            .iter()
            .filter(|i| keep(i))
            .cloned()
            .collect(),
        exclude: compose
            .exclude
            .iter()
            .filter(|i| keep(i))
            .cloned()
            .collect(),
        inactive: compose.inactive,
    };
    negotiation.pin_lenient(&kept)
}

/// Every parameter that controlled the expansion, echoed, then the code system
/// versions used (`used-codesystem`).
///
/// `open_systems` are the systems the value sets selected from without naming a
/// version, which is where a `system-version` rule takes effect: it "specifies
/// a version to use for a system, if the value set does not specify which one
/// to use" (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), and
/// `ValueSet.expansion.parameter` states "a parameter that controlled the
/// expansion process" (<https://hl7.org/fhir/R4B/valueset.html>).
fn parameters(
    input: &ExpandInput,
    expansion: &Expansion,
    used_value_sets: &[String],
    used_supplements: &[String],
    open_systems: &BTreeSet<String>,
) -> Vec<ExpansionParameter> {
    let mut out = Vec::new();
    let mut push = |name: &str, value: ParameterValue| {
        out.push(ExpansionParameter {
            name: name.to_owned(),
            value,
        });
    };
    if let Some(filter) = &input.filter {
        push("filter", ParameterValue::String(filter.clone()));
    }
    for (name, flag) in [
        ("activeOnly", input.active_only),
        ("excludeNested", input.exclude_nested),
        ("includeDesignations", input.include_designations),
        ("includeDefinition", input.include_definition),
        ("excludeNotForUI", input.exclude_not_for_ui),
        ("excludePostCoordinated", input.exclude_post_coordinated),
        ("handle-unclosed-expansion", input.handle_unclosed_expansion),
    ] {
        if let Some(flag) = flag {
            push(name, ParameterValue::Boolean(flag));
        }
    }
    for (name, number) in [("offset", input.offset), ("count", input.count)] {
        if let Some(number) = number {
            push(name, ParameterValue::Integer(number));
        }
    }
    if let Some(language) = &input.display_language {
        push("displayLanguage", ParameterValue::Code(language.clone()));
    }
    for designation in &input.designation {
        push("designation", ParameterValue::String(designation.clone()));
    }
    // A default and a check supply a version only where the value set names
    // none; a force overrides whatever it names, so it always controls.
    for (name, list) in [
        ("system-version", &input.system_version),
        ("check-system-version", &input.check_system_version),
    ] {
        for value in list {
            let url = value.split_once('|').map_or(value.as_str(), |(url, _)| url);
            if open_systems.contains(url) {
                push(name, ParameterValue::Uri(value.clone()));
            }
        }
    }
    for (name, list) in [
        ("exclude-system", &input.exclude_system),
        ("force-system-version", &input.force_system_version),
        ("default-valueset-version", &input.default_valueset_version),
        ("check-valueset-version", &input.check_valueset_version),
        ("force-valueset-version", &input.force_valueset_version),
    ] {
        for value in list {
            push(name, ParameterValue::Uri(value.clone()));
        }
    }
    for used in &expansion.versions {
        push(
            "used-codesystem",
            ParameterValue::Uri(canonical(&used.url, &used.version)),
        );
    }
    // NOTE: the supplements an expansion drew on, versioned, the ecosystem's
    // `used-supplement`; the request's own `useSupplement` need carry no
    // version (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
    for used in used_supplements {
        push("used-supplement", ParameterValue::Uri(used.clone()));
    }
    // NOTE: the fragments an expansion drew on, beside the `used-*` family the
    // ecosystem requires (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>,
    // "all versions of code systems used (in 'used-*')").
    for used in &expansion.fragments {
        push(
            "used-fragment",
            ParameterValue::Uri(canonical(&used.url, &used.version)),
        );
    }
    // NOTE: the value sets an expansion drew on, the ecosystem's `used-valueset`
    // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, `$expand parameters`).
    for used in used_value_sets {
        push("used-valueset", ParameterValue::Uri(used.clone()));
    }
    out
}

fn contains(
    sources: &Sources<'_>,
    expansion: &Expansion,
    input: &ExpandInput,
) -> Result<Vec<Contains>, OperationError> {
    let include_designations = input.include_designations.unwrap_or(false);
    let wanted: Vec<&str> = input.designation.iter().map(String::as_str).collect();
    let mut out: Vec<(usize, Contains)> = Vec::with_capacity(expansion.items.len());
    for item in &expansion.items {
        let designations = if include_designations {
            designations_of(sources, item, &wanted)?
        } else {
            Vec::new()
        };
        // NOTE: an inactive concept carries its status property unasked, the
        // ecosystem's shape ("in expansions, the status property SHALL be populated",
        // <https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
        let mut asked = input.property.clone();
        if item.inactive && !asked.iter().any(|p| p == "status" || p == "*") {
            asked.push(String::from("status"));
        }
        let mut properties = if asked.is_empty() {
            Vec::new()
        } else {
            properties_of(sources, item, &asked)?
        };
        // NOTE: "the status property may also be used to indicate that a concept
        // is inactive" (<https://hl7.org/fhir/R5/codesystem.html#defined-props>),
        // so a system stating only `inactive` still answers a status.
        if item.inactive && !properties.iter().any(|p| p.code == "status") {
            properties.push(Property {
                code: String::from("status"),
                value: PropertyValue::Code(String::from("inactive")),
                ..Property::default()
            });
        }
        out.push((
            item.depth,
            Contains {
                system: item.system.clone(),
                version: item.version.clone(),
                code: item.code.clone(),
                display: item.display.clone(),
                abstract_concept: item.abstract_concept,
                inactive: item.inactive,
                designations,
                properties,
                contains: Vec::new(),
            },
        ));
    }
    Ok(nest(out))
}

/// The pre-order page as a tree: an entry deeper than the one before it
/// becomes its child. A flat page (every depth `0`) comes back unchanged.
fn nest(page: Vec<(usize, Contains)>) -> Vec<Contains> {
    let mut entries = page.into_iter().peekable();
    level(&mut entries, 0)
}

/// The entries of `entries` that belong at `depth` or deeper, as one level of
/// the tree; each entry takes the deeper entries that follow it as its own.
fn level(
    entries: &mut std::iter::Peekable<std::vec::IntoIter<(usize, Contains)>>,
    depth: usize,
) -> Vec<Contains> {
    let mut out = Vec::new();
    while entries.peek().is_some_and(|(at, _)| *at >= depth) {
        let Some((_, mut entry)) = entries.next() else {
            break;
        };
        entry.contains = level(entries, depth.saturating_add(1));
        out.push(entry);
    }
    out
}

/// The designations of an item the client asked for: by language, or by
/// `system|code` use, or every one when none is named.
fn designations_of(
    sources: &Sources<'_>,
    item: &Item,
    wanted: &[&str],
) -> Result<Vec<Designation>, OperationError> {
    let resolved = sources
        .registry
        .resolve(&item.system, Some(&item.version))?;
    let Some(located) = resolved.provider.locate(&item.code)? else {
        return Ok(Vec::new());
    };
    let selected = |d: &Designation| {
        if wanted.is_empty() {
            return true;
        }
        wanted.iter().any(|w| match w.split_once('|') {
            // NOTE: `urn:ietf:bcp:47|<lang>` names a language, as the parameter's
            // definition spells it (<https://hl7.org/fhir/R5/valueset-operation-expand.html>).
            Some(("urn:ietf:bcp:47", language)) => d.language.as_deref() == Some(language),
            Some((system, code)) => d
                .use_
                .as_ref()
                .is_some_and(|u| u.system == system && u.code == code),
            None => d.language.as_deref() == Some(*w),
        })
    };
    let mut designations: Vec<Designation> = resolved
        .provider
        .designations(located.concept, None)?
        .into_iter()
        .filter(selected)
        .collect();
    // A `displayLanguage` that lands on a designation makes that designation
    // the display, and the system's own display then has nowhere else to go.
    // It moves into the designation list, marked preferred for the language
    // the system states, and the one that became the display is not repeated,
    // so the answer carries each translation once
    // (<https://hl7.org/fhir/R5/valueset-operation-expand.html>,
    // `displayLanguage`).
    let base = resolved.provider.display(located.concept, None)?;
    if let Some(base) = base
        && item.display.as_ref().is_none_or(|shown| *shown != base)
    {
        designations.retain(|d| Some(&d.value) != item.display.as_ref());
        let promoted = super::lookup::display_designation(resolved.provider.as_ref(), None, &base);
        if selected(&promoted) {
            // First, because it is the display the system itself states and
            // the rest of the list are alternates to it.
            designations.insert(0, promoted);
        }
    }
    Ok(designations)
}

/// The `warning-<standing>` parameters for every resource the expansion drew
/// on whose standing warrants one: the value set expanded, the value sets it
/// referenced, and the code system versions used.
///
/// The ecosystem asks an expansion to state the standing of what it drew on
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>); a resource the
/// server cannot resolve back to a model contributes nothing.
fn warnings(
    sources: &Sources<'_>,
    model: &ValueSetModel,
    expansion: &Expansion,
    used_value_sets: &[String],
) -> Vec<ExpansionParameter> {
    let mut out = Vec::new();
    let mut note = |word: Option<&'static str>, canonical: String| {
        if let Some(word) = word {
            out.push(ExpansionParameter {
                name: format!("warning-{word}"),
                value: ParameterValue::Uri(canonical),
            });
        }
    };
    // NOTE: a value set's own `status` states where it sits in its own
    // lifecycle, so only `structuredefinition-standards-status` warns here
    // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
    let standing_of = |value_set: &ValueSetModel| crate::provider::Standing {
        standards_status: value_set.standards_status.clone(),
        ..crate::provider::Standing::default()
    };
    note(super::standing_word(&standing_of(model)), model.canonical());
    for used in used_value_sets {
        if let Some(referenced) = sources.value_sets.resolve(used, None) {
            note(
                super::standing_word(&standing_of(&referenced)),
                used.clone(),
            );
        }
    }
    for used in &expansion.versions {
        if let Ok(resolved) = sources.registry.resolve(&used.url, Some(&used.version)) {
            note(
                super::standing_word(&resolved.provider.standing()),
                canonical(&used.url, &used.version),
            );
        }
    }
    out
}

/// `url|version`, the canonical of a code system version; `url` alone for a
/// versionless system.
fn canonical(url: &str, version: &str) -> String {
    if version.is_empty() {
        url.to_owned()
    } else {
        format!("{url}|{version}")
    }
}

/// The properties of an item the client asked for: every one for `*`, else
/// those whose code or declared URI is named.
fn properties_of(
    sources: &Sources<'_>,
    item: &Item,
    wanted: &[String],
) -> Result<Vec<Property>, OperationError> {
    let resolved = sources
        .registry
        .resolve(&item.system, Some(&item.version))?;
    let Some(located) = resolved.provider.locate(&item.code)? else {
        return Ok(Vec::new());
    };
    let all = wanted.iter().any(|w| w == "*");
    let declaration = resolved.provider.declaration();
    let asked = |code: &str| {
        all || wanted.iter().any(|w| {
            w == code
                || declaration
                    .properties
                    .iter()
                    .any(|p| p.code == code && p.uri.as_deref() == Some(w.as_str()))
        })
    };
    let mut out = Vec::new();
    // NOTE: `definition` is a concept property the specification itself defines
    // (<https://hl7.org/fhir/6.0.0-ballot5/codesystem.html#defined-props>), which
    // a system states as its own element rather than through `properties`.
    if asked("definition")
        && let Some(definition) = resolved.provider.definition(located.concept)?
    {
        out.push(Property {
            code: String::from("definition"),
            value: PropertyValue::String(definition),
            ..Property::default()
        });
    }
    out.extend(
        resolved
            .provider
            .properties(located.concept)?
            .into_iter()
            .filter(|p| asked(&p.code)),
    );
    Ok(out)
}

/// The distinct properties the page carries, with the URI the code system
/// declares for each, for `expansion.property`.
fn expansion_properties(
    sources: &Sources<'_>,
    contains: &[Contains],
) -> Result<Vec<ExpansionProperty>, OperationError> {
    let mut out: Vec<ExpansionProperty> = Vec::new();
    declared(sources, contains, &mut out)?;
    Ok(out)
}

/// Adds to `out` the properties `entries` and their children carry, each once,
/// in order of appearance.
fn declared(
    sources: &Sources<'_>,
    entries: &[Contains],
    out: &mut Vec<ExpansionProperty>,
) -> Result<(), OperationError> {
    for entry in entries {
        for property in &entry.properties {
            if out.iter().any(|p| p.code == property.code) {
                continue;
            }
            let resolved = sources
                .registry
                .resolve(&entry.system, Some(&entry.version))?;
            let uri = resolved
                .provider
                .declaration()
                .properties
                .iter()
                .find(|p| p.code == property.code)
                .and_then(|p| p.uri.clone())
                .or_else(|| crate::provider::defined_property_uri(&property.code));
            out.push(ExpansionProperty {
                code: property.code.clone(),
                uri,
            });
        }
        declared(sources, &entry.contains, out)?;
    }
    Ok(())
}
