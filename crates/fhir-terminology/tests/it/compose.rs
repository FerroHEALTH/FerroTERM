// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The compose layer: union, intersection, exclude, dedup, order, paging
//! (<https://hl7.org/fhir/R5/valueset.html#compositions>).

use fhir_terminology::compose::{
    Compose, ComposeError, ConceptRef, Expander, Expansion, Include, Options, SystemRef,
    ValueSetResolver,
};
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::provider::ProviderError;
use fhir_terminology::registry::ResolveError;

use crate::fixture::{FLAT_URL, URL, registry};

fn system(version: Option<&str>) -> SystemRef {
    SystemRef {
        url: String::from(URL),
        version: version.map(str::to_owned),
    }
}

fn filter(property: &str, op: FilterOperator, value: &str) -> Filter {
    Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    }
}

fn concepts(codes: &[&str]) -> Vec<ConceptRef> {
    codes
        .iter()
        .map(|code| ConceptRef {
            deprecated: false,
            code: (*code).to_owned(),
            display: None,
        })
        .collect()
}

fn codes(expansion: &Expansion) -> Vec<String> {
    expansion
        .items
        .iter()
        .map(|item| item.code.clone())
        .collect()
}

#[test]
fn includes_union_criteria_intersect_and_excludes_subtract() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = Compose {
        include: vec![
            Include {
                system: Some(system(None)),
                filters: vec![
                    filter("concept", FilterOperator::IsA, "animal"),
                    filter("legs", FilterOperator::Equal, "4"),
                ],
                ..Include::default()
            },
            Include {
                system: Some(system(None)),
                concepts: concepts(&["plant", "cat"]),
                ..Include::default()
            },
        ],
        exclude: vec![Include {
            system: Some(system(None)),
            concepts: concepts(&["dog"]),
            ..Include::default()
        }],
        inactive: None,
    };
    let expansion = expander
        .expand(&compose, &Options::default())
        .expect("expands");
    // cat appears once although two includes select it, and the order is the
    // compose's: the filter include first, then the codes the second include
    // named, first occurrence winning.
    assert_eq!(codes(&expansion), ["cat", "kitten", "plant"]);
    assert_eq!(expansion.total, 3);
    assert_eq!(expansion.versions.len(), 1);
    assert_eq!(expansion.versions[0].version, "2025");
    assert!(expansion.versions[0].defaulted);
}

#[test]
fn a_system_without_criteria_is_every_code_regardless_of_status() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = Compose {
        include: vec![Include {
            system: Some(system(Some("2024"))),
            ..Include::default()
        }],
        ..Compose::default()
    };
    let all = expander
        .expand(&compose, &Options::default())
        .expect("expands");
    assert_eq!(all.total, 7);
    assert!(
        all.items
            .iter()
            .any(|item| item.code == "fish" && item.inactive)
    );
    assert_eq!(all.versions[0].version, "2024");
    assert!(!all.versions[0].defaulted);
    let active = expander
        .expand(
            &compose,
            &Options {
                active_only: true,
                ..Options::default()
            },
        )
        .expect("expands");
    assert_eq!(active.total, 6);
    assert!(active.items.iter().all(|item| !item.inactive));
    // compose.inactive = false is a floor the request cannot lift.
    let floor = Compose {
        inactive: Some(false),
        ..compose.clone()
    };
    assert_eq!(
        expander
            .expand(&floor, &Options::default())
            .expect("expands")
            .total,
        6
    );
}

#[test]
fn paging_partitions_the_ordered_result() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = Compose {
        include: vec![Include {
            system: Some(system(None)),
            ..Include::default()
        }],
        ..Compose::default()
    };
    let all = expander
        .expand(&compose, &Options::default())
        .expect("expands");
    let mut paged = Vec::new();
    let mut offset = 0;
    loop {
        let page = expander
            .expand(
                &compose,
                &Options {
                    offset,
                    count: Some(3),
                    ..Options::default()
                },
            )
            .expect("expands");
        assert_eq!(page.total, all.total);
        assert_eq!(page.offset, offset);
        if page.items.is_empty() {
            break;
        }
        paged.extend(page.items);
        offset += 3;
    }
    assert_eq!(paged, all.items);
    let zero = expander
        .expand(
            &compose,
            &Options {
                count: Some(0),
                ..Options::default()
            },
        )
        .expect("expands");
    assert_eq!((zero.total, zero.items.len()), (7, 0));
}

#[test]
fn text_and_language_apply_per_system() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = Compose {
        include: vec![Include {
            system: Some(system(None)),
            ..Include::default()
        }],
        ..Compose::default()
    };
    let options = Options {
        text: Some(String::from("ki")),
        language: Some(String::from("nl")),
        ..Options::default()
    };
    let hits = expander.expand(&compose, &options).expect("expands");
    assert_eq!(codes(&hits), ["kitten"]);
    assert_eq!(hits.items[0].display.as_deref(), Some("Kitten"));
    let dutch = expander
        .expand(
            &compose,
            &Options {
                text: Some(String::from("hon")),
                language: Some(String::from("nl")),
                ..Options::default()
            },
        )
        .expect("expands");
    assert_eq!(codes(&dutch), ["dog"]);
    assert_eq!(dutch.items[0].display.as_deref(), Some("Hond"));
}

#[test]
fn order_follows_the_compose_and_displays_can_be_overridden() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = Compose {
        include: vec![
            Include {
                system: Some(system(Some("2025"))),
                concepts: concepts(&["dog"]),
                ..Include::default()
            },
            Include {
                system: Some(system(Some("2024"))),
                concepts: concepts(&["cat"]),
                ..Include::default()
            },
            Include {
                system: Some(SystemRef {
                    url: String::from(FLAT_URL),
                    version: None,
                }),
                concepts: vec![ConceptRef {
                    deprecated: false,
                    code: String::from("cat"),
                    display: Some(String::from("Tabby")),
                }],
                ..Include::default()
            },
        ],
        ..Compose::default()
    };
    let expansion = expander
        .expand(&compose, &Options::default())
        .expect("expands");
    let keys: Vec<(&str, &str, &str)> = expansion
        .items
        .iter()
        .map(|item| {
            (
                item.system.as_str(),
                item.version.as_str(),
                item.code.as_str(),
            )
        })
        .collect();
    // The compose named the 2025 system first, so it answers first: no FHIR
    // version fixes the order of an expansion
    // (<https://hl7.org/fhir/R4B/valueset.html>), and the compose order is the
    // one `ValueSet.compose.include.concept` says an expansion typically follows.
    assert_eq!(
        keys,
        [
            (URL, "2025", "dog"),
            (URL, "2024", "cat"),
            (FLAT_URL, "1", "cat")
        ]
    );
    assert_eq!(expansion.items[2].display.as_deref(), Some("Tabby"));
    assert_eq!(expansion.versions.len(), 3);
}

#[derive(Debug)]
struct Fixed(Vec<(&'static str, Vec<&'static str>)>);

impl ValueSetResolver for Fixed {
    fn expand(&self, url: &str) -> Result<Expansion, ComposeError> {
        let registry = registry();
        let (_, codes) = self
            .0
            .iter()
            .find(|(u, _)| *u == url)
            .ok_or_else(|| ComposeError::NoResolver(url.to_owned()))?;
        Expander::new(&registry).expand(
            &Compose {
                include: vec![Include {
                    system: Some(system(None)),
                    concepts: concepts(codes),
                    ..Include::default()
                }],
                ..Compose::default()
            },
            &Options::default(),
        )
    }
}

#[test]
fn value_sets_in_one_include_intersect_with_each_other_and_with_the_system() {
    let registry = registry();
    let resolver = Fixed(vec![
        ("http://example.org/vs/pets", vec!["cat", "dog", "fish"]),
        ("http://example.org/vs/furry", vec!["cat", "dog", "kitten"]),
    ]);
    let expander = Expander::with_resolver(&registry, &resolver);
    let two = Compose {
        include: vec![Include {
            value_sets: vec![
                String::from("http://example.org/vs/pets"),
                String::from("http://example.org/vs/furry"),
            ],
            ..Include::default()
        }],
        ..Compose::default()
    };
    assert_eq!(
        codes(&expander.expand(&two, &Options::default()).expect("expands")),
        ["cat", "dog"]
    );
    let with_system = Compose {
        include: vec![Include {
            system: Some(system(None)),
            filters: vec![filter("legs", FilterOperator::Equal, "0")],
            value_sets: vec![String::from("http://example.org/vs/pets")],
            ..Include::default()
        }],
        ..Compose::default()
    };
    assert_eq!(
        codes(
            &expander
                .expand(&with_system, &Options::default())
                .expect("expands")
        ),
        ["fish"]
    );
    let without_resolver = Expander::new(&registry);
    assert!(matches!(
        without_resolver.expand(&two, &Options::default()),
        Err(ComposeError::NoResolver(_))
    ));
}

#[test]
fn invalid_composes_and_unknown_codes_are_typed_errors() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let run = |include: Include| {
        expander.expand(
            &Compose {
                include: vec![include],
                ..Compose::default()
            },
            &Options::default(),
        )
    };
    assert!(matches!(
        run(Include::default()),
        Err(ComposeError::NoSystemOrValueSet)
    ));
    assert!(matches!(
        run(Include {
            concepts: concepts(&["cat"]),
            value_sets: vec![String::from("http://example.org/vs/pets")],
            ..Include::default()
        }),
        Err(ComposeError::CriteriaWithoutSystem)
    ));
    assert!(matches!(
        run(Include {
            system: Some(system(None)),
            concepts: concepts(&["cat"]),
            filters: vec![filter("legs", FilterOperator::Equal, "4")],
            ..Include::default()
        }),
        Err(ComposeError::ConceptsAndFilters)
    ));
    // An enumerated code the system does not define leaves the expansion.
    let dropped = run(Include {
        system: Some(system(None)),
        concepts: concepts(&["unicorn", "cat"]),
        ..Include::default()
    })
    .expect("expands");
    assert_eq!(dropped.total, 1);
    assert!(matches!(
        run(Include {
            system: Some(system(Some("1999"))),
            ..Include::default()
        }),
        Err(ComposeError::Resolve(ResolveError::UnknownVersion { .. }))
    ));
    assert!(matches!(
        run(Include {
            system: Some(system(None)),
            filters: vec![filter("colour", FilterOperator::Equal, "red")],
            ..Include::default()
        }),
        Err(ComposeError::Provider { .. })
    ));
}

#[test]
fn implicit_value_sets_are_parsed_by_the_system_that_owns_the_uri() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = registry
        .implicit_value_set("http://example.org/fixture?vs=isa/cat")
        .expect("the fixture claims the URI")
        .expect("well formed");
    let expansion = expander
        .expand(&compose, &Options::default())
        .expect("expands");
    assert_eq!(codes(&expansion), ["cat", "kitten"]);
    assert_eq!(expansion.versions[0].version, "2025");
    assert!(
        registry
            .implicit_value_set("http://example.org/elsewhere?vs=isa/cat")
            .is_none()
    );
    assert!(matches!(
        registry.implicit_value_set("http://example.org/fixture?vs=refset/1"),
        Some(Err(ProviderError::MalformedImplicitValueSet(_)))
    ));
}

/// Paging cuts the compose order, not the provider's.
///
/// No FHIR version fixes the order of an expansion
/// (<https://hl7.org/fhir/R4B/valueset.html>), and
/// `ValueSet.compose.include.concept` says an expansion typically follows the
/// compose, so a page of the first two is the first two the compose named
/// (#276).
#[test]
fn count_and_offset_page_over_the_compose_order() {
    let registry = registry();
    let expander = Expander::new(&registry);
    let compose = Compose {
        include: vec![Include {
            system: Some(system(None)),
            concepts: concepts(&["dog", "plant", "cat"]),
            ..Include::default()
        }],
        ..Compose::default()
    };
    let page = |offset: usize, count: usize| {
        let options = Options {
            offset,
            count: Some(count),
            ..Options::default()
        };
        let expansion = expander.expand(&compose, &options).expect("expands");
        codes(&expansion)
    };
    assert_eq!(page(0, 3), ["dog", "plant", "cat"], "the order it named");
    assert_eq!(page(0, 2), ["dog", "plant"], "the first page");
    assert_eq!(page(2, 2), ["cat"], "the page after it");
    assert_eq!(page(1, 1), ["plant"], "one from the middle");
}
