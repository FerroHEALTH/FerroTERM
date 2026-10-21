// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One entity as the ICD-API serves it, in as many languages as were cached.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::Linearization;

/// A text in a language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    /// The BCP 47 language.
    pub language: String,
    /// The text.
    pub value: String,
}

/// One postcoordination axis a stem accepts values on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scale {
    /// The axis name, a schema URI (`http://id.who.int/icd/schema/infectiousAgent`).
    pub axis: String,
    /// Whether WHO marks the axis as required.
    pub required: bool,
    /// `AllowAlways`, `NotAllowed`, or `AllowedExceptFromSameBlock`.
    pub multiple: String,
    /// The entity ids whose subtrees supply the values.
    pub entities: Vec<String>,
}

/// One entity of a linearization or of the Foundation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entity {
    /// The id: the number, with `/other` or `/unspecified` for a residual.
    pub id: String,
    /// The short code (`1A00`, `b198`); a block or a Foundation entity has none.
    pub code: Option<String>,
    /// `chapter`, `block`, `category`, or `window`.
    pub class_kind: Option<String>,
    /// The parents' ids; empty for a root.
    pub parents: Vec<String>,
    /// The children's ids.
    pub children: Vec<String>,
    /// The title per language.
    pub titles: Vec<Text>,
    /// The definition per language.
    pub definitions: Vec<Text>,
    /// The fully specified name per language.
    pub fully_specified: Vec<Text>,
    /// The inclusion terms.
    pub inclusions: Vec<Text>,
    /// The exclusion labels.
    pub exclusions: Vec<Text>,
    /// The index terms (the linearizations) or synonyms (the Foundation).
    pub index_terms: Vec<Text>,
    /// The postcoordination scales of a stem.
    pub scales: Vec<Scale>,
    /// The Foundation entity URI a linearization entity comes from.
    pub source: Option<String>,
    /// The browser URL.
    pub browser_url: Option<String>,
}

/// A failure to read an entity's JSON.
#[derive(Debug, thiserror::Error)]
pub enum EntityError {
    /// The JSON has no `@id` this code system names.
    #[error("the entity JSON has no `@id` in `{0}`")]
    NoId(&'static str),
}

fn text(value: Option<&Value>) -> Option<Text> {
    let object = value?.as_object()?;
    Some(Text {
        language: object
            .get("@language")
            .and_then(Value::as_str)
            .unwrap_or("en")
            .to_owned(),
        value: object.get("@value")?.as_str()?.to_owned(),
    })
}

fn labels(value: Option<&Value>) -> Vec<Text> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| text(item.get("label")))
                .collect()
        })
        .unwrap_or_default()
}

fn ids(value: Option<&Value>, linearization: Linearization) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|uri| linearization.id_of(uri))
                .collect()
        })
        .unwrap_or_default()
}

impl Entity {
    /// Reads the entity `json` the API served for `linearization`.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError::NoId`] when the JSON names no entity of the
    /// code system.
    pub fn parse(json: &Value, linearization: Linearization) -> Result<Self, EntityError> {
        let id = json
            .get("@id")
            .and_then(Value::as_str)
            .and_then(|uri| linearization.id_of(uri))
            .ok_or(EntityError::NoId(linearization.name()))?;
        let code = json
            .get("code")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        let scales = json
            .get("postcoordinationScale")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|scale| {
                        Some(Scale {
                            axis: scale.get("axisName")?.as_str()?.to_owned(),
                            required: scale
                                .get("requiredPostcoordination")
                                .and_then(Value::as_str)
                                .is_some_and(|r| r == "true"),
                            multiple: scale
                                .get("allowMultipleValues")
                                .and_then(Value::as_str)
                                .unwrap_or("AllowAlways")
                                .to_owned(),
                            entities: ids(scale.get("scaleEntity"), linearization),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut index_terms = labels(json.get("indexTerm"));
        index_terms.extend(labels(json.get("synonym")));
        Ok(Self {
            id,
            code,
            class_kind: json
                .get("classKind")
                .and_then(Value::as_str)
                .map(str::to_owned),
            parents: ids(json.get("parent"), linearization),
            children: ids(json.get("child"), linearization),
            titles: text(json.get("title")).into_iter().collect(),
            definitions: text(json.get("definition")).into_iter().collect(),
            fully_specified: text(json.get("fullySpecifiedName")).into_iter().collect(),
            inclusions: labels(json.get("inclusion")),
            exclusions: labels(json.get("exclusion")),
            index_terms,
            scales,
            source: json
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_owned),
            browser_url: json
                .get("browserUrl")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    /// Adds the texts of the same entity read in another language.
    pub fn merge_language(&mut self, other: Self) {
        for (mine, theirs) in [
            (&mut self.titles, other.titles),
            (&mut self.definitions, other.definitions),
            (&mut self.fully_specified, other.fully_specified),
            (&mut self.inclusions, other.inclusions),
            (&mut self.exclusions, other.exclusions),
            (&mut self.index_terms, other.index_terms),
        ] {
            for text in theirs {
                if !mine.contains(&text) {
                    mine.push(text);
                }
            }
        }
    }

    /// The title in `language`, else in English, else any.
    #[must_use]
    pub fn title(&self, language: &str) -> Option<&str> {
        let pick = |lang: &str| {
            self.titles
                .iter()
                .find(|t| t.language.eq_ignore_ascii_case(lang))
                .map(|t| t.value.as_str())
        };
        pick(language)
            .or_else(|| pick("en"))
            .or_else(|| self.titles.first().map(|t| t.value.as_str()))
    }

    /// The languages the entity carries texts in, sorted.
    #[must_use]
    pub fn languages(&self) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for text in self
            .titles
            .iter()
            .chain(&self.definitions)
            .chain(&self.index_terms)
        {
            out.insert(text.language.clone());
        }
        out.into_iter().collect()
    }
}
