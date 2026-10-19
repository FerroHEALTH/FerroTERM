//! The taxonomy tree's model: which rows are visible.
//!
//! The model is a plain value so the tree's behaviour is pinned by unit tests
//! rather than by a browser. It carries no code system knowledge: it is given
//! a root concept, the set of open codes, and whatever children have been read
//! for each of them, and it says what to draw.
//!
//! The keyboard and the tree view pattern are FerroUI's tree widget; this
//! model says which rows the address opens.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::url::decode_query_component;
use crate::url::encode_query_component;

/// The separator between two codes in a list the address carries.
///
/// Every code is percent-encoded before it is joined, and the encoder escapes
/// this separator, so a code that contains one cannot split a list in two.
const SEPARATOR: char = ',';

/// One concept in the tree, as little of it as a row needs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct TreeConcept {
    /// The code, which is the concept's identity in its system.
    pub(crate) code: String,
    /// The display the server sent for it, absent when it sent none.
    pub(crate) display: Option<String>,
}

/// One visible row of the tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TreeRow {
    /// The concept this row draws.
    pub(crate) concept: TreeConcept,
    /// The codes from the root down to here, encoded and joined.
    ///
    /// One concept can sit under two parents, because a hierarchy may be a
    /// graph, so the path rather than the code is what identifies a row. It is
    /// derived from the data and never from a position.
    pub(crate) key: String,
    /// The key of the row this one sits under, absent at the root.
    pub(crate) parent: Option<String>,
    /// How deep the row sits, zero at the root.
    pub(crate) depth: u32,
    /// Whether this row's children are showing.
    pub(crate) open: bool,
    /// Whether this row can be opened at all.
    pub(crate) expandable: bool,
    /// Which of its siblings this row is, counting from one.
    pub(crate) position: u32,
    /// How many siblings the row sits among.
    pub(crate) siblings: u32,
}

/// The rows to draw, in the order they appear on screen.
///
/// The tree is anchored at `root` and starts at its children, so the anchor
/// itself is not a row: the concept the tree hangs from is the one the screen
/// is already showing above it.
///
/// `children` holds the concepts read for a code so far; a code it does not
/// name has not been read yet, and its row is drawn as openable so the reader
/// can ask. A code that is already on its own path is drawn as a leaf, so an
/// answer that loops cannot walk forever.
pub(crate) fn rows(
    root: &str,
    open: &BTreeSet<String>,
    children: &BTreeMap<String, Vec<TreeConcept>>,
) -> Vec<TreeRow> {
    let mut rows: Vec<TreeRow> = Vec::new();
    let top = children.get(root).map(Vec::as_slice).unwrap_or_default();
    let anchors = u32::try_from(top.len()).unwrap_or(u32::MAX);
    let mut pending: Vec<Pending> = top
        .iter()
        .enumerate()
        .rev()
        .map(|(index, concept)| Pending {
            concept: concept.clone(),
            depth: 0,
            ancestors: vec![root.to_owned()],
            parent: None,
            position: u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
            siblings: anchors,
        })
        .collect();
    while let Some(entry) = pending.pop() {
        let mut path = entry.ancestors.clone();
        path.push(entry.concept.code.clone());
        let key = key_of(&path);
        let read = children.get(&entry.concept.code);
        let expandable = !entry.ancestors.contains(&entry.concept.code)
            && read.is_none_or(|read| !read.is_empty());
        let showing = expandable && open.contains(&entry.concept.code);
        if let Some(read) = read.filter(|_| showing) {
            let siblings = u32::try_from(read.len()).unwrap_or(u32::MAX);
            pending.extend(read.iter().enumerate().rev().map(|(index, child)| Pending {
                concept: child.clone(),
                depth: entry.depth.saturating_add(1),
                ancestors: path.clone(),
                parent: Some(key.clone()),
                position: u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
                siblings,
            }));
        }
        rows.push(TreeRow {
            concept: entry.concept,
            key,
            parent: entry.parent,
            depth: entry.depth,
            open: showing,
            expandable,
            position: entry.position,
            siblings: entry.siblings,
        });
    }
    rows
}

/// One node the walk has reached but not yet drawn.
struct Pending {
    /// The concept the row will draw.
    concept: TreeConcept,
    /// How deep it sits.
    depth: u32,
    /// The codes above it, which is what stops a walk that loops.
    ancestors: Vec<String>,
    /// The key of the row it sits under.
    parent: Option<String>,
    /// Which of its siblings it is, counting from one.
    position: u32,
    /// How many siblings it sits among.
    siblings: u32,
}

/// The path of codes as one value an address can carry.
fn key_of(path: &[String]) -> String {
    path.iter()
        .map(|code| encode_query_component(code))
        .collect::<Vec<String>>()
        .join(&SEPARATOR.to_string())
}

/// The open codes as one value an address can carry.
///
/// The set is ordered, so the same open nodes always write the same address
/// and a link a reader shares is the one they were reading.
pub(crate) fn encode_open(open: &BTreeSet<String>) -> String {
    open.iter()
        .map(|code| encode_query_component(code))
        .collect::<Vec<String>>()
        .join(&SEPARATOR.to_string())
}

/// The open codes an address carries.
pub(crate) fn decode_open(value: &str) -> BTreeSet<String> {
    value
        .split(SEPARATOR)
        .filter(|part| !part.is_empty())
        .map(decode_query_component)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concept(code: &str) -> TreeConcept {
        TreeConcept {
            code: code.to_owned(),
            display: Some(format!("{code} display")),
        }
    }

    fn read(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<TreeConcept>> {
        entries
            .iter()
            .map(|(parent, children)| {
                (
                    (*parent).to_owned(),
                    children.iter().map(|code| concept(code)).collect(),
                )
            })
            .collect()
    }

    fn open(codes: &[&str]) -> BTreeSet<String> {
        codes.iter().map(|code| (*code).to_owned()).collect()
    }

    fn codes(rows: &[TreeRow]) -> Vec<String> {
        rows.iter().map(|row| row.concept.code.clone()).collect()
    }

    #[test]
    fn an_anchor_whose_children_are_unread_draws_no_row() {
        assert!(
            rows("root", &BTreeSet::new(), &BTreeMap::new()).is_empty(),
            "the tree waits for the read rather than inventing children"
        );
    }

    #[test]
    fn the_children_of_the_anchor_are_the_top_of_the_tree() {
        let drawn = rows("root", &BTreeSet::new(), &read(&[("root", &["a", "b"])]));
        assert_eq!(codes(&drawn), ["a", "b"]);
        assert_eq!(
            drawn.iter().map(|row| row.depth).collect::<Vec<u32>>(),
            [0, 0],
            "the anchor is the concept the screen already shows, so it is not a row"
        );
        assert!(
            drawn.iter().all(|row| row.expandable && !row.open),
            "children that have not been read yet are still worth asking for"
        );
    }

    #[test]
    fn every_row_states_where_it_sits_among_its_siblings() {
        let drawn = rows(
            "root",
            &BTreeSet::new(),
            &read(&[("root", &["a", "b", "c"])]),
        );
        assert_eq!(
            drawn
                .iter()
                .map(|row| (row.position, row.siblings))
                .collect::<Vec<(u32, u32)>>(),
            [(1, 3), (2, 3), (3, 3)],
            "a flat tree carries the set each row belongs to, because the DOM does not"
        );
    }

    #[test]
    fn a_node_read_as_childless_is_a_leaf() {
        let drawn = rows(
            "root",
            &open(&["a"]),
            &read(&[("root", &["a"]), ("a", &[])]),
        );
        assert!(
            drawn
                .first()
                .is_some_and(|row| !row.expandable && !row.open),
            "a node the server answered no children for cannot be opened"
        );
    }

    #[test]
    fn every_level_of_an_open_path_is_drawn_in_order() {
        let drawn = rows(
            "root",
            &open(&["a"]),
            &read(&[("root", &["a", "b"]), ("a", &["a1", "a2"])]),
        );
        assert_eq!(codes(&drawn), ["a", "a1", "a2", "b"]);
        assert_eq!(
            drawn.iter().map(|row| row.depth).collect::<Vec<u32>>(),
            [0, 1, 1, 0]
        );
        assert_eq!(
            drawn.get(1).and_then(|row| row.parent.clone()),
            Some(String::from("root,a")),
            "a row knows the row it sits under, which is what the left arrow walks to"
        );
    }

    #[test]
    fn one_concept_under_two_parents_gets_two_distinct_keys() {
        let drawn = rows(
            "root",
            &open(&["a", "b"]),
            &read(&[
                ("root", &["a", "b"]),
                ("a", &["shared"]),
                ("b", &["shared"]),
            ]),
        );
        let keys: Vec<String> = drawn.iter().map(|row| row.key.clone()).collect();
        let unique: BTreeSet<String> = keys.iter().cloned().collect();
        assert_eq!(
            keys.len(),
            unique.len(),
            "a hierarchy may be a graph, so the path identifies a row: {keys:?}"
        );
    }

    #[test]
    fn a_key_survives_a_code_that_carries_the_separator() {
        let drawn = rows("a,b", &BTreeSet::new(), &read(&[("a,b", &["c,d"])]));
        assert_eq!(
            drawn
                .iter()
                .map(|row| row.key.clone())
                .collect::<Vec<String>>(),
            ["a%2Cb,c%2Cd"],
            "the code is encoded before it is joined, so it cannot split the path"
        );
    }

    #[test]
    fn an_answer_that_loops_back_stops_at_the_repeat() {
        let drawn = rows(
            "root",
            &open(&["a"]),
            &read(&[("root", &["a"]), ("a", &["root"])]),
        );
        assert_eq!(codes(&drawn), ["a", "root"]);
        assert!(
            drawn.get(1).is_some_and(|row| !row.expandable),
            "a code already on its own path is drawn as a leaf, so the walk ends"
        );
    }

    #[test]
    fn the_open_nodes_round_trip_through_the_address() {
        let opened = open(&["a,b", "c d", "e"]);
        let written = encode_open(&opened);
        assert_eq!(written, "a%2Cb,c%20d,e");
        assert_eq!(
            decode_open(&written),
            opened,
            "a code carrying the separator or a space survives the address"
        );
    }

    #[test]
    fn an_address_that_opens_nothing_reads_as_no_open_nodes() {
        assert!(decode_open("").is_empty());
        assert!(
            decode_open(",,").is_empty(),
            "an empty part names no code, so it is left out"
        );
        assert_eq!(encode_open(&BTreeSet::new()), "");
    }
}
