// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ValueSet` a save of the composer writes and the `Parameters` a
//! preview posts, through the `fhir-types` model of the release that serves
//! it.

use super::{Clause, Draft, Preview};
use crate::fhir::release::{Previews, Rewrites};

/// The `$expand` input carrying the value set to expand
/// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
const VALUE_SET_PARAMETER: &str = "valueSet";

/// The `$expand` input carrying the text filter.
const FILTER_PARAMETER: &str = "filter";

/// The `$expand` input carrying the page size.
const COUNT_PARAMETER: &str = "count";

/// The `$expand` input carrying the offset of the page.
const OFFSET_PARAMETER: &str = "offset";

impl Draft {
    /// The clauses of one side, included or excluded, in the order the
    /// composer holds them.
    fn side(&self, included: bool) -> impl Iterator<Item = &Clause> {
        self.clauses
            .iter()
            .filter(move |clause| clause.included == included)
    }
}

/// Writes the composer's drafts over the `ValueSet`, and its previews as the
/// `Parameters`, of the `fhir-types` module `$module`.
macro_rules! writes_value_set {
    ($module:ident) => {
        mod $module {
            use fhir_types::$module::parameters::{
                Parameters, ParametersParameter, ParametersParameterValue,
            };
            use fhir_types::$module::primitives;
            use fhir_types::$module::resource::Resource;
            use fhir_types::$module::value_set::{
                ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
                ValueSetComposeIncludeFilter,
            };

            use super::{
                COUNT_PARAMETER, Clause, Draft, FILTER_PARAMETER, OFFSET_PARAMETER, Preview,
                Previews, Rewrites, VALUE_SET_PARAMETER,
            };

            /// `text`, or nothing when the reader left it empty.
            fn stated<T: From<String>>(text: &str) -> Option<T> {
                (!text.is_empty()).then(|| T::from(text.to_owned()))
            }

            /// `clause`, written over `held`, the entry it was read from, so an
            /// element the form does not draw travels back rather than being
            /// deleted by the update.
            fn written(
                mut held: ValueSetComposeInclude,
                clause: &Clause,
            ) -> ValueSetComposeInclude {
                let system = clause.system.trim();
                held.system = stated(system);
                held.version = if system.is_empty() {
                    None
                } else {
                    stated(clause.system_version.trim())
                };
                held.value_set = clause
                    .value_sets
                    .iter()
                    .filter(|row| !row.canonical.trim().is_empty())
                    .map(|row| row.canonical.trim().into())
                    .collect();
                // A concept is identified by its code and a filter by the property
                // it is over (<https://hl7.org/fhir/R4B/valueset.html>), so that is
                // what an authored row is matched to the stored one by.
                let concepts = std::mem::take(&mut held.concept);
                held.concept = clause
                    .concepts
                    .iter()
                    .filter(|picked| !picked.code.trim().is_empty())
                    .map(|picked| {
                        let code = picked.code.trim();
                        let mut concept = concepts
                            .iter()
                            .find(|stored| stored.code.value.as_deref() == Some(code))
                            .cloned()
                            .unwrap_or_default();
                        concept.code = code.into();
                        // NOTE: "Any display names specified for the codes are ignored" in an
                        // exclude (<https://hl7.org/fhir/R4B/valueset.html>,
                        // `ValueSet.compose.exclude`), so none is written into one.
                        concept.display = if clause.included {
                            stated(picked.display.trim())
                        } else {
                            None
                        };
                        concept
                    })
                    .collect::<Vec<ValueSetComposeIncludeConcept>>();
                let filters = std::mem::take(&mut held.filter);
                held.filter = clause
                    .filters
                    .iter()
                    .filter(|filter| !filter.property.trim().is_empty())
                    .map(|filter| {
                        let property = filter.property.trim();
                        let mut written = filters
                            .iter()
                            .find(|stored| stored.property.value.as_deref() == Some(property))
                            .cloned()
                            .unwrap_or_default();
                        written.property = property.into();
                        written.op = filter.op.trim().into();
                        written.value = From::from(primitives::String::from(filter.value.trim()));
                        written
                    })
                    .collect::<Vec<ValueSetComposeIncludeFilter>>();
                held
            }

            /// The clauses of one side, each written over the stored entry at
            /// its position on that side.
            ///
            /// A clause is matched to the stored one by its position on its own
            /// side, which is the order the draft read them in and the order this
            /// writes them back, so a clause keeps the elements the form does not
            /// draw as long as the reader has not reordered that side.
            fn side(
                draft: &Draft,
                included: bool,
                stored: Vec<ValueSetComposeInclude>,
            ) -> Vec<ValueSetComposeInclude> {
                draft
                    .side(included)
                    .enumerate()
                    .map(|(at, clause)| {
                        written(stored.get(at).cloned().unwrap_or_default(), clause)
                    })
                    .collect()
            }

            impl Rewrites<Draft> for ValueSet {
                fn rewritten(mut self, draft: &Draft) -> Self {
                    self.id = (!draft.id.is_empty()).then(|| draft.id.clone());
                    self.url = stated(draft.url.trim());
                    self.version = stated(draft.version.trim());
                    self.name = stated(draft.name.trim());
                    self.title = stated(draft.title.trim());
                    self.status = draft.status.as_str().into();
                    // `ValueSet.compose` is 0..1, so a draft composing nothing
                    // sends no element rather than an empty one; the elements the
                    // composer does not draw, `lockedDate` among them, travel back.
                    self.compose = if draft.clauses.is_empty() {
                        None
                    } else {
                        let mut compose: ValueSetCompose = self.compose.unwrap_or_default();
                        compose.inactive = draft.inactive.map(|inactive| primitives::Boolean {
                            value: Some(inactive),
                            ..primitives::Boolean::default()
                        });
                        let include = std::mem::take(&mut compose.include);
                        let exclude = std::mem::take(&mut compose.exclude);
                        compose.include = side(draft, true, include);
                        compose.exclude = side(draft, false, exclude);
                        Some(compose)
                    };
                    self
                }
            }

            /// One parameter named `name` carrying the integer `number`.
            fn counted(name: &str, number: i32) -> ParametersParameter {
                ParametersParameter {
                    name: name.into(),
                    value: Some(ParametersParameterValue::Integer(primitives::Integer {
                        value: Some(number),
                        ..primitives::Integer::default()
                    })),
                    ..ParametersParameter::default()
                }
            }

            impl Previews for Parameters {
                type ValueSet = ValueSet;

                fn previewed(value_set: ValueSet, page: &Preview) -> Self {
                    let mut parameter = vec![ParametersParameter {
                        name: VALUE_SET_PARAMETER.into(),
                        resource: Some(Resource::ValueSet(Box::new(value_set))),
                        ..ParametersParameter::default()
                    }];
                    if let Some(filter) = page.filter.as_ref().filter(|text| !text.is_empty()) {
                        parameter.push(ParametersParameter {
                            name: FILTER_PARAMETER.into(),
                            value: Some(ParametersParameterValue::String(filter.as_str().into())),
                            ..ParametersParameter::default()
                        });
                    }
                    parameter.push(counted(COUNT_PARAMETER, page.count));
                    if page.offset > 0 {
                        parameter.push(counted(OFFSET_PARAMETER, page.offset));
                    }
                    Self {
                        parameter,
                        ..Self::default()
                    }
                }
            }
        }
    };
}

writes_value_set!(r4);
writes_value_set!(r4b);
writes_value_set!(r5);
writes_value_set!(r6);
