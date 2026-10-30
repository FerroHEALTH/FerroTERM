// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The locale the console renders in, and the catalog of its own words.
//!
//! The console's words are messages of the Fluent catalogs under the crate's
//! `locales/` directory, one directory per language
//! (`locales/en/ferroterm-console.ftl`), embedded at build time by
//! `fluent_templates::static_loader!`, the mechanism `ferrohealth_ui`
//! catalogs its own words with. English is the one catalog that ships and the
//! one every lookup falls back to; a language joins as one catalog file and
//! no code change. The build writes [`Msg`] from the English catalog, so a
//! lookup of a message the catalog lacks fails to compile.
//!
//! The server chooses the locale once per request: the best language of the
//! request's `Accept-Language` the catalog holds, else the configured
//! `[server] locale`, and a language the reader chose in Your preferences
//! wins over both. The request carries it (`crate::request::Request`), and
//! every word a screen draws, its own and the components', is looked up in
//! it, so the document writes it as `<html lang>` (WCAG 2.2 SC 3.1.1). A
//! number a message takes is written in the marks of the page's language by
//! the library's formatter, which reads the catalog's `number-group` and
//! `number-decimal`. No specification governs the catalog layout or the
//! choice: our own design.

use ferrohealth_ui::i18n::{Arg, Locale, Message};
use fluent_templates::Loader;

fluent_templates::static_loader! {
    static CATALOG = {
        locales: "./locales",
        fallback_language: "en",
        // NOTE: Fluent's guide, "Bidirectionality": the isolation marks around a
        // placeable are for mixed-direction text; the English catalog draws none.
        customise: |bundle| {
            bundle.set_use_isolating(false);
            bundle.set_formatter(Some(ferrohealth_ui::i18n::number::format_number));
        },
    };
}

include!(concat!(env!("OUT_DIR"), "/messages.rs"));

/// The console's catalogs, every language it ships.
#[must_use]
pub(crate) fn catalog() -> &'static (dyn Loader + Send + Sync) {
    &*CATALOG
}

/// The console's message `msg` in `locale`.
#[must_use]
pub(crate) fn tr(locale: &Locale, msg: Msg) -> String {
    tr_in(catalog(), locale, msg)
}

/// The console's message `msg` in `locale`, with the placeables `args`
/// filled in.
#[must_use]
pub(crate) fn tr_with(locale: &Locale, msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale.lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` as a [`Message`] a component looks up as it
/// draws, such as a listing's noun or an editor's title.
#[must_use]
pub(crate) fn message(msg: Msg) -> Message {
    Message::new(catalog(), msg.id())
}

/// The message `msg` of `catalog` in `locale`.
#[must_use]
pub(crate) fn tr_in(catalog: &dyn Loader, locale: &Locale, msg: Msg) -> String {
    locale.lookup(catalog, msg.id())
}

/// The locale a request whose `Accept-Language` is `header` asks for: the
/// best language of it the catalog holds, else `configured`.
#[must_use]
pub(crate) fn requested(header: &str, configured: Locale) -> Locale {
    Locale::negotiate_with(header, catalog(), configured)
}

/// The locale the page is drawn in: the language the reader chose in Your
/// preferences when the console ships a catalog of it, else `requested`,
/// the language the request negotiates.
#[must_use]
pub(crate) fn chosen(prefs: &crate::prefs::Prefs, requested: Locale) -> Locale {
    prefs
        .family
        .language
        .tag()
        .filter(|tag| offered(tag))
        .and_then(|tag| Locale::parse(tag).ok())
        .unwrap_or(requested)
}

/// Whether the console ships a catalog of the language `tag`.
#[must_use]
pub(crate) fn offered(tag: &str) -> bool {
    catalog()
        .locales()
        .any(|shipped| shipped.to_string().as_str() == tag)
}

/// The languages the console ships a catalog of, each as a locale.
#[must_use]
pub(crate) fn languages() -> Vec<Locale> {
    let mut tags: Vec<String> = catalog().locales().map(ToString::to_string).collect();
    tags.sort();
    tags.iter()
        .filter_map(|tag| Locale::parse(tag).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;
    use fluent_templates::fluent_bundle::FluentValue;

    use super::{Msg, catalog, languages, offered, tr_in};

    fluent_templates::static_loader! {
        static THROWAWAY = {
            locales: "./tests/locales",
            fallback_language: "qaa",
            customise: |bundle| bundle.set_use_isolating(false),
        };
    }

    /// Every placeable the catalog at `path` names, each set to a number, so
    /// a message that selects a form by one formats whole.
    fn placeables(path: &Path) -> HashMap<Cow<'static, str>, FluentValue<'static>> {
        let source = std::fs::read_to_string(path).expect("the catalog");
        source
            .split("{ $")
            .skip(1)
            .filter_map(|rest| rest.split([' ', '}']).next())
            .map(|name| (Cow::Owned(name.to_owned()), FluentValue::from(3)))
            .collect()
    }

    /// The directories under `locales/`, each a language.
    fn catalog_directories(root: &Path) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(root)
            .expect("the locales directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn every_catalog_directory_is_a_language_holding_every_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let found = catalog_directories(&root);
        let mut shipped: Vec<String> = catalog().locales().map(ToString::to_string).collect();
        shipped.sort();
        assert_eq!(found, shipped, "a catalog directory is a language");
        assert!(
            found.contains(&String::from("en")),
            "English is the reference"
        );
        for tag in found {
            let language = tag.parse().expect("a language");
            let args = placeables(&root.join(&tag).join("ferroterm-console.ftl"));
            for msg in Msg::ALL {
                let words = catalog().try_lookup_with_args(&language, msg.id(), &args);
                assert!(
                    words.is_some_and(|words| !words.trim().is_empty()),
                    "{tag} holds {}",
                    msg.id()
                );
            }
        }
    }

    #[test]
    fn a_throwaway_catalog_changes_the_words_with_no_code_change() {
        let english = Locale::english();
        assert_eq!(
            "No such screen",
            tr_in(catalog(), &english, Msg::ErrorsNotFoundTitle)
        );
        let throwaway = Locale::parse("qaa").expect("a private-use language");
        assert_eq!(
            "Qo quch qcreen",
            tr_in(&*THROWAWAY, &throwaway, Msg::ErrorsNotFoundTitle)
        );
        assert_eq!(
            "Dashboard",
            tr_in(catalog(), &throwaway, Msg::ShellDashboard),
            "the shipped catalogs fall back to English"
        );
    }

    /// Every source file of the console, its path and its text.
    fn sources(directory: &Path, found: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(directory)
            .expect("the source directory")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file");
                found.push((path.display().to_string(), text));
            }
        }
    }

    /// The variables the catalog message `id` names, from the English catalog.
    fn variables(catalog_text: &str, id: &str) -> Vec<String> {
        let mut lines = catalog_text
            .lines()
            .skip_while(|line| !line.starts_with(&format!("{id} =")));
        let mut text = lines.next().map(str::to_owned).unwrap_or_default();
        for line in lines {
            if line.starts_with(' ') || line.starts_with('\t') {
                text.push_str(line);
            } else {
                break;
            }
        }
        let mut names: Vec<String> = text
            .split("{ $")
            .skip(1)
            .filter_map(|rest| {
                rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .map(str::to_owned)
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    #[test]
    fn every_tr_with_call_supplies_each_variable_its_message_names() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let catalog_text = std::fs::read_to_string(root.join("locales/en/ferroterm-console.ftl"))
            .expect("the catalog");
        let ids: HashMap<String, &str> = Msg::ALL
            .iter()
            .map(|msg| (format!("{msg:?}"), msg.id()))
            .collect();
        let mut files = Vec::new();
        sources(&root.join("src"), &mut files);
        let mut missing = Vec::new();
        for (path, text) in &files {
            for (at, _) in text.match_indices("tr_with(") {
                let rest = text.get(at + "tr_with(".len()..).unwrap_or_default();
                let mut depth = 1_u32;
                let mut end = rest.len();
                for (index, c) in rest.char_indices() {
                    match c {
                        '(' | '[' => depth += 1,
                        ')' | ']' => {
                            depth -= 1;
                            if depth == 0 {
                                end = index;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let call = rest.get(..end).unwrap_or_default();
                // NOTE: no specification governs this: our own design; the call
                // names its locale first, then the message.
                let Some((_locale, variant)) = call.split_once("Msg::") else {
                    continue;
                };
                let variant: String = variant
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect();
                let Some(id) = ids.get(&variant) else {
                    continue;
                };
                let packed: String = call.split_whitespace().collect();
                for name in variables(&catalog_text, id) {
                    if !packed.contains(&format!("(\"{name}\"")) {
                        missing.push(format!(
                            "{path}: {id} names ${name} and the call leaves it out"
                        ));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "a call that leaves out a variable falls back to the message key: {missing:#?}"
        );
    }

    #[test]
    fn every_catalog_carries_the_marks_numbers_are_written_with() {
        for language in catalog().locales() {
            for id in ["number-group", "number-decimal"] {
                assert!(
                    catalog().try_lookup(language, id).is_some(),
                    "{language} holds {id}"
                );
            }
        }
        let words = Locale::english().lookup_with(
            catalog(),
            Msg::ValueSetShownOf.id(),
            &[("shown", 10_u32.into()), ("total", 12_345_u32.into())],
        );
        assert_eq!(
            "Concepts 1 to 10 of 12,345.", words,
            "a number is grouped by the catalog's mark"
        );
    }

    #[test]
    fn the_languages_offered_are_the_catalog_directories_and_no_others() {
        let shipped = catalog_directories(&Path::new(env!("CARGO_MANIFEST_DIR")).join("locales"));
        assert_eq!(
            shipped,
            languages().iter().map(Locale::tag).collect::<Vec<_>>(),
            "a language joins as one catalog directory"
        );
        for tag in &shipped {
            assert!(offered(tag), "{tag}");
        }
        for tag in ["", "qaa", "xx", "EN-zz"] {
            assert!(!offered(tag), "{tag} has no catalog");
        }
    }
}
