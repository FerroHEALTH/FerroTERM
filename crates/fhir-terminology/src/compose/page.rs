// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One page of an expansion: the inactive and abstract concepts left out, the
//! nesting of the hierarchy, and the items materialized with their displays.

use super::{ComposeError, Gathered, Item, Options, Page, STATUS_SCAN_LIMIT, Selection};
use crate::language;
use crate::provider::compose::Compose;
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::{CodeSystemProvider, Concept, ConceptSet, Hierarchy, error::ProviderError};

/// The page `options` asks for, over the order the compose stated.
///
/// A nested expansion pages over its pre-order flattening, so `count` and
/// `offset` mean the same thing they do in a flat one.
pub(super) fn paged(
    compose: &Compose,
    options: &Options,
    gathered: &Gathered,
) -> Result<Page, ComposeError> {
    let mut page = Page {
        items: Vec::new(),
        nested: false,
    };
    let mut skip = options.offset;
    let mut remaining = options.count.unwrap_or(usize::MAX);
    for key in &gathered.order {
        if remaining == 0 {
            break;
        }
        let (url, version) = key;
        let Some(selection) = gathered.selections.get(key) else {
            continue;
        };
        let len = usize::try_from(selection.set.len()).unwrap_or(usize::MAX);
        if skip >= len {
            skip = skip.saturating_sub(len);
            continue;
        }
        let wanted = skip.saturating_add(remaining);
        let cut: Vec<(u32, usize)> = match nests(compose, options)
            .then(|| preorder(selection, wanted))
            .flatten()
        {
            Some(tree) => {
                page.nested = true;
                tree.into_iter().skip(skip).take(remaining).collect()
            }
            None => selection
                .ordered()
                .map(|index| (index, 0))
                .skip(skip)
                .take(remaining)
                .collect(),
        };
        for (index, depth) in cut {
            page.items
                .push(materialize(url, version, selection, index, depth, options)?);
        }
        remaining = remaining.saturating_sub(len.saturating_sub(skip));
        skip = 0;
    }
    Ok(page)
}

/// Removes the inactive concepts from `selection`: concept by concept for a
/// small set, by the provider's inactive set for a large one.
pub(super) fn drop_inactive(url: &str, selection: &mut Selection) -> Result<(), ComposeError> {
    let failed = |source: ProviderError| ComposeError::Provider {
        system: url.to_owned(),
        source,
    };
    if selection.set.len() > STATUS_SCAN_LIMIT {
        match selection.provider.inactive() {
            Ok(inactive) => {
                selection.set -= &inactive;
                return Ok(());
            }
            Err(ProviderError::NotEnumerable) => {}
            Err(source) => return Err(failed(source)),
        }
    }
    let mut inactive = ConceptSet::new();
    for index in &selection.set {
        if !selection
            .provider
            .status(Concept::new(index))
            .map_err(failed)?
            .active
        {
            inactive.insert(index);
        }
    }
    selection.set -= inactive;
    Ok(())
}

/// Drops the abstract (`notSelectable`) groupers from `selection`: by a status
/// scan for a small set, by the provider's set for a large one.
pub(super) fn drop_not_for_ui(url: &str, selection: &mut Selection) -> Result<(), ComposeError> {
    let failed = |source: ProviderError| ComposeError::Provider {
        system: url.to_owned(),
        source,
    };
    if selection.set.len() > STATUS_SCAN_LIMIT {
        match selection.provider.not_for_ui() {
            Ok(abstract_concepts) => {
                selection.set -= &abstract_concepts;
                return Ok(());
            }
            Err(ProviderError::NotEnumerable) => {}
            Err(source) => return Err(failed(source)),
        }
    }
    let mut abstract_concepts = ConceptSet::new();
    for index in &selection.set {
        if selection
            .provider
            .status(Concept::new(index))
            .map_err(failed)?
            .abstract_concept
        {
            abstract_concepts.insert(index);
        }
    }
    selection.set -= abstract_concepts;
    Ok(())
}

/// Whether `compose` expands as the system's own hierarchy: one include of one
/// system, selected by the whole system or by a hierarchical filter, so the
/// selection is a set of subtrees rather than a list somebody enumerated.
///
/// A `ValueSet.expansion.contains` may nest its children
/// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>);
/// the ecosystem's `parameters-expand-*-hierarchy` cases pin which composes do.
pub(super) fn nests(compose: &Compose, options: &Options) -> bool {
    if options.exclude_nested || !compose.exclude.is_empty() {
        return false;
    }
    let [include] = compose.include.as_slice() else {
        return false;
    };
    if include.system.is_none() || !include.concepts.is_empty() || !include.value_sets.is_empty() {
        return false;
    }
    let hierarchical = |filter: &Filter| {
        matches!(
            filter.op,
            FilterOperator::IsA | FilterOperator::DescendentOf
        )
    };
    if !include.filters.iter().all(hierarchical) {
        return false;
    }
    // NOTE: a text search over a whole system returns scattered matches with no root
    // to hang them from and the ecosystem's `search-all-yes` case wants them flat; the
    // same search inside an `is-a` include keeps the subtree (its `search-filter-yes`).
    include.filters.iter().any(hierarchical)
        || options
            .text
            .as_deref()
            .is_none_or(|text| text.trim().is_empty())
}

/// The first `wanted` members of `selection` in pre-order with their depths:
/// each root (a member no other member subsumes) followed by its children, in
/// the provider's concept order.
///
/// The walk stops at `wanted`, so a page costs the page rather than the
/// selection. A root has no parent in the selection, so no root lies inside
/// another root's subtree: visiting the selection in order and walking from
/// each root as it is reached emits the same sequence as walking every root
/// after finding them all.
///
/// `None` when the provider has no hierarchy to walk.
pub(super) fn preorder(selection: &Selection, wanted: usize) -> Option<Vec<(u32, usize)>> {
    let hierarchy = selection.provider.hierarchy()?;
    let mut out = Vec::new();
    let mut seen = ConceptSet::new();
    for index in &selection.set {
        if out.len() >= wanted {
            return Some(out);
        }
        if hierarchy.any_parent_in(Concept::new(index), &selection.set) {
            continue;
        }
        subtree(hierarchy, selection, index, wanted, &mut seen, &mut out);
    }
    // A cycle or a member no root reaches keeps its place at the end, flat.
    for index in &selection.set {
        if out.len() >= wanted {
            break;
        }
        if !seen.contains(index) {
            out.push((index, 0));
        }
    }
    Some(out)
}

/// Appends the members of `selection` under `root` to `out` in pre-order with
/// their depths, stopping once `out` holds `wanted`.
pub(super) fn subtree(
    hierarchy: &dyn Hierarchy,
    selection: &Selection,
    root: u32,
    wanted: usize,
    seen: &mut ConceptSet,
    out: &mut Vec<(u32, usize)>,
) {
    let mut stack: Vec<(u32, usize)> = vec![(root, 0)];
    while let Some((index, depth)) = stack.pop() {
        if !seen.insert(index) {
            continue;
        }
        out.push((index, depth));
        if out.len() >= wanted {
            return;
        }
        let children: Vec<u32> = hierarchy
            .children(Concept::new(index))
            .iter()
            .filter(|child| selection.set.contains(*child) && !seen.contains(*child))
            .collect();
        stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
    }
}

/// The item for the concept at `index` of `selection`.
pub(super) fn materialize(
    url: &str,
    version: &str,
    selection: &Selection,
    index: u32,
    depth: usize,
    options: &Options,
) -> Result<Item, ComposeError> {
    let failed = |source: ProviderError| ComposeError::Provider {
        system: url.to_owned(),
        source,
    };
    let concept = Concept::new(index);
    let provider = &selection.provider;
    let code = match selection.spellings.get(&index) {
        Some(spelling) => spelling.clone(),
        None => provider
            .code(concept)
            .map_err(failed)?
            .unwrap_or_else(|| index.to_string()),
    };
    let requested = options.language.as_deref();
    let display = if let Some(display) = selection.overrides.get(&index) {
        Some(display.clone())
    } else {
        let asked = language::for_provider(provider.as_ref(), requested);
        let shown = provider
            .display(concept, asked.as_deref())
            .map_err(failed)?;
        // NOTE: a request refusing every language it does not name has no
        // display to answer with here, so the item carries none
        // (<https://www.rfc-editor.org/rfc/rfc9110#field.accept-language>).
        if language::refuses_unnamed(requested)
            && !acceptable_display(provider.as_ref(), concept, requested).map_err(failed)?
        {
            None
        } else {
            shown
        }
    };
    let status = provider.status(concept).map_err(failed)?;
    Ok(Item {
        system: url.to_owned(),
        version: version.to_owned(),
        code,
        display,
        inactive: !status.active,
        abstract_concept: status.abstract_concept,
        depth,
    })
}

/// Whether the concept has a display `requested` accepts: the system's own
/// language, or a designation in a language the request did not refuse.
pub(super) fn acceptable_display(
    provider: &dyn CodeSystemProvider,
    concept: Concept,
    requested: Option<&str>,
) -> Result<bool, ProviderError> {
    if provider
        .language()
        .is_some_and(|own| language::accepts(requested, own))
    {
        return Ok(true);
    }
    Ok(provider.designations(concept, None)?.iter().any(|d| {
        d.language
            .as_deref()
            .is_some_and(|tag| language::accepts(requested, tag))
    }))
}
