//! The display language of a request: a language range list resolved
//! against the languages a code system carries.
//!
//! `displayLanguage` and `Accept-Language` both carry a list of language
//! ranges with optional quality values (`en, en-AU; q=0.4`, `de,*`), the
//! syntax of RFC 9110 §12.5.4 (<https://www.rfc-editor.org/rfc/rfc9110#field.accept-language>);
//! the HL7 terminology ecosystem IG reads the parameter the same way
//! (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/languages.html>).
//! The first range, by quality then by position, whose primary subtag a code
//! system carries names the display language; `*` means the system's own.

use crate::operations::OperationError;
use crate::provider::CodeSystemProvider;
use crate::registries::bcp47::Analysis;

/// One language range of a list, with its quality.
#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    /// The range as written (`en-AU`, `*`).
    pub tag: String,
    /// The quality value, `1.0` when absent.
    pub quality: f32,
}

/// Every range of `text` as written, zero-quality ones included, highest
/// quality first with ties in list order.
fn all_ranges(text: &str) -> Vec<Range> {
    let mut ranges: Vec<Range> = text
        .split(',')
        .filter_map(|part| {
            let mut pieces = part.split(';');
            let tag = pieces.next()?.trim();
            if tag.is_empty() {
                return None;
            }
            let quality = pieces
                .filter_map(|p| p.trim().strip_prefix("q="))
                .find_map(|q| q.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            Some(Range {
                tag: tag.to_owned(),
                quality,
            })
        })
        .collect();
    ranges.sort_by(|a, b| {
        b.quality
            .partial_cmp(&a.quality)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranges
}

/// The ranges of `text`, highest quality first, ties in list order; ranges
/// with quality `0` are dropped (RFC 9110: "not acceptable").
#[must_use]
pub fn ranges(text: &str) -> Vec<Range> {
    all_ranges(text)
        .into_iter()
        .filter(|range| range.quality > 0.0)
        .collect()
}

/// Whether `requested` refuses every language it does not name.
///
/// A `*` at quality zero is the one way to say it: RFC 9110 gives `q=0` the
/// meaning "not acceptable", and `*` stands for every language not listed
/// (<https://www.rfc-editor.org/rfc/rfc9110#field.accept-language>). Without
/// it a request states a preference, and a system with nothing in the asked
/// language still answers its own display.
#[must_use]
pub fn refuses_unnamed(requested: Option<&str>) -> bool {
    requested.is_some_and(|text| {
        all_ranges(text)
            .iter()
            .any(|range| range.tag == "*" && range.quality == 0.0)
    })
}

/// Whether `requested` accepts a display written in `tag`.
///
/// A language named at quality zero is refused however the rest of the list
/// reads, and a `*` above zero accepts whatever the list does not name.
#[must_use]
pub fn accepts(requested: Option<&str>, tag: &str) -> bool {
    let Some(text) = requested else {
        return true;
    };
    let ranges = all_ranges(text);
    let named = ranges
        .iter()
        .find(|range| primary(&range.tag) == primary(tag));
    if let Some(range) = named {
        return range.quality > 0.0;
    }
    ranges
        .iter()
        .find(|range| range.tag == "*")
        .is_none_or(|range| range.quality > 0.0)
}

/// Checks a `displayLanguage` (or `Accept-Language`) value: every range is
/// `*` or a valid BCP 47 tag with registered subtags.
///
/// # Errors
///
/// Returns [`OperationError::InvalidLanguage`] naming the first range that is
/// malformed or carries an unregistered subtag; a display language the
/// server cannot interpret is refused, never guessed (the ecosystem's
/// `lookup-bad-language` and `validation-wrong-de-en-bad` cases).
pub fn check(requested: Option<&str>) -> Result<(), OperationError> {
    let Some(text) = requested else {
        return Ok(());
    };
    let ranges = ranges(text);
    if ranges.is_empty() {
        return Err(OperationError::InvalidLanguage(text.to_owned()));
    }
    for range in ranges {
        if range.tag == "*" {
            continue;
        }
        match crate::registries::bcp47::analyze_tag(&range.tag) {
            Analysis::Valid(_) => {}
            Analysis::Malformed(_) | Analysis::WellFormed { .. } => {
                return Err(OperationError::InvalidLanguage(range.tag));
            }
        }
    }
    Ok(())
}

fn primary(tag: &str) -> String {
    tag.split(['-', '_'])
        .next()
        .unwrap_or(tag)
        .to_ascii_lowercase()
}

/// The language to display in, from the ranges of `requested` and the
/// languages `available`.
///
/// The first range a language matches by primary subtag wins; `*` and no
/// request mean the system's own language (`None`); when nothing matches,
/// the first range as written, so the provider's fallback applies and the
/// response states the language it used.
#[must_use]
pub fn choose(requested: Option<&str>, available: &[String]) -> Option<String> {
    let text = requested?;
    let ranges = ranges(text);
    if ranges.is_empty() {
        return None;
    }
    for range in &ranges {
        if range.tag == "*" {
            return None;
        }
        if available.iter().any(|a| primary(a) == primary(&range.tag)) {
            return Some(range.tag.clone());
        }
    }
    ranges.first().map(|r| r.tag.clone())
}

/// [`choose`] against the languages `provider` declares.
#[must_use]
pub fn for_provider(provider: &dyn CodeSystemProvider, requested: Option<&str>) -> Option<String> {
    choose(requested, &provider.declaration().languages)
}

/// The language `provider` answers a request for `requested` in: the chosen
/// range when the system carries designations in it, else the system's own
/// language.
///
/// [`choose`] answers an unmatched request with the range as written so the
/// provider can try it; the answer is then in the system's own language, and
/// saying otherwise would label a display with a language it is not in.
#[must_use]
pub fn answered(provider: &dyn CodeSystemProvider, requested: Option<&str>) -> Option<String> {
    let available = &provider.declaration().languages;
    match for_provider(provider, requested) {
        Some(tag) if available.iter().any(|a| primary(a) == primary(&tag)) => Some(tag),
        _ => provider.language().map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::{choose, ranges};

    fn langs(list: &[&str]) -> Vec<String> {
        list.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn ranges_sort_by_quality_then_position_and_drop_q0() {
        let r = ranges("en, en-AU; q=0.4, de;q=0.8, fr;q=0");
        let tags: Vec<&str> = r.iter().map(|x| x.tag.as_str()).collect();
        assert_eq!(tags, ["en", "de", "en-AU"]);
        assert_eq!(ranges(""), Vec::<crate::language::Range>::new());
    }

    #[test]
    fn the_first_carried_range_wins_and_star_means_the_default() {
        let en_de = langs(&["en", "de"]);
        assert_eq!(choose(Some("de"), &en_de).as_deref(), Some("de"));
        assert_eq!(
            choose(Some("en, en-AU; q=0.4"), &en_de).as_deref(),
            Some("en")
        );
        assert_eq!(choose(Some("fr, de;q=0.5"), &en_de).as_deref(), Some("de"));
        assert_eq!(choose(Some("de,*"), &en_de).as_deref(), Some("de"));
        assert_eq!(choose(Some("*"), &en_de), None);
        assert_eq!(
            choose(Some("fr,*"), &en_de),
            None,
            "any language is acceptable"
        );
        assert_eq!(
            choose(Some("en-GB"), &langs(&["en-US"])).as_deref(),
            Some("en-GB")
        );
        assert_eq!(
            choose(Some("zz"), &en_de).as_deref(),
            Some("zz"),
            "nothing matches: the first range, so the fallback states itself"
        );
        assert_eq!(choose(None, &en_de), None);
        assert_eq!(choose(Some("nl"), &[]).as_deref(), Some("nl"));
    }
}
