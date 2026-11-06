// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The implicit value sets and concept maps of the FHIR SNOMED CT page
//! (<https://hl7.org/fhir/R4B/snomedct.html>): the URI forms, the version they
//! name, and the filter each form stands for.

use super::{SYSTEM, SnomedProvider};
use crate::provider::declaration::{ImplicitArgument, ImplicitForm};
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::{
    CodeSystemProvider, ConceptSet,
    error::{InvalidValue, MalformedValueSet, ProviderError},
};
use rf2::id::ConceptId;
use sct_ecl::ast::ExpressionConstraint;
use std::sync::Arc;

impl SnomedProvider {
    /// The concept named by `text`, as the store spells it.
    pub(super) fn sctid_of(&self, url: &str, text: &str) -> Result<String, ProviderError> {
        match self.locate(text)? {
            Some(located) => Ok(located.code),
            None => Err(match ConceptId::parse(text) {
                Ok(_) => ProviderError::UnknownCode(text.to_owned()),
                Err(_) => ProviderError::malformed_implicit_value_set(
                    url,
                    format!("`{text}` is not an SCTID"),
                ),
            }),
        }
    }

    /// The preferred term of `code` in the server's default language, else
    /// `code` itself: the templates' "[sctid or preferred description]"
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
    pub(super) fn term_or_code(&self, code: &str) -> String {
        self.locate(code)
            .ok()
            .flatten()
            .and_then(|located| {
                self.choose_display(Self::ordinal(located.concept), None)
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| code.to_owned())
    }

    /// The `version` an implicit URI on `base` resolves to: `None` for the
    /// bare system URI, this edition's version URI for its own edition or
    /// version base.
    ///
    /// A base naming another edition is
    /// [`ProviderError::UnservedImplicitVersion`], never a malformed URI: the
    /// FHIR SNOMED CT page admits any edition version as the base
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets"), so
    /// the caller asks the other loaded editions before the server answers
    /// that it holds no such version.
    pub(crate) fn implicit_version(&self, base: &str) -> Result<Option<String>, ProviderError> {
        if base == SYSTEM {
            Ok(None)
        } else if base == self.system().edition || base == self.identity().version {
            Ok(Some(self.identity().version.clone()))
        } else {
            Err(ProviderError::UnservedImplicitVersion {
                url: SYSTEM.to_owned(),
                version: base.to_owned(),
            })
        }
    }

    /// The filter behind an `isa/[sctid]` or `refset/[sctid]` form of `url`.
    pub(super) fn implicit_filter(&self, url: &str, form: &str) -> Result<Filter, ProviderError> {
        let malformed = |reason: String| ProviderError::malformed_implicit_value_set(url, reason);
        let (kind, argument) = form.split_once('/').unwrap_or((form, ""));
        match kind {
            "isa" => Ok(Filter {
                property: String::from("concept"),
                op: FilterOperator::IsA,
                value: self.sctid_of(url, argument)?,
            }),
            "refset" => {
                let refset = self.sctid_of(url, argument)?;
                if ConceptId::parse(&refset)
                    .ok()
                    .and_then(|id| self.system().memberships.members(id.value()))
                    .is_none()
                {
                    // NOTE: a language reference set references descriptions, so
                    // "all concept ids in the specified reference set" selects none
                    // (<https://hl7.org/fhir/R4B/snomedct.html>).
                    if self
                        .system()
                        .keys
                        .refsets
                        .iter()
                        .any(|(_, id)| *id == refset)
                    {
                        return Err(ProviderError::invalid_filter_value(
                            "concept",
                            refset,
                            "a language reference set references descriptions, not concepts",
                        ));
                    }
                    return Err(ProviderError::UnknownCode(refset));
                }
                Ok(Filter {
                    property: String::from("concept"),
                    op: FilterOperator::In,
                    value: refset,
                })
            }
            "ecl" => {
                let text = percent_decode(argument).ok_or_else(|| {
                    malformed(String::from("the expression constraint is not URI-encoded"))
                })?;
                self.expression(&text).map_err(|error| {
                    ProviderError::MalformedImplicitValueSet(Box::new(MalformedValueSet {
                        url: url.to_owned(),
                        reason: error.to_string(),
                        position: url_offset(url, argument, error.offset()),
                        source: Some(error),
                    }))
                })?;
                Ok(Filter {
                    property: String::from("constraint"),
                    op: FilterOperator::Equal,
                    value: text,
                })
            }
            _ => Err(malformed(format!("`{form}` is not a `fhir_vs` form"))),
        }
    }

    /// The parsed form of an expression constraint, from the cache when it
    /// was parsed before (the cache is cleared when it holds 256 entries).
    pub(super) fn expression(
        &self,
        text: &str,
    ) -> Result<Arc<ExpressionConstraint>, sct_scg::front::ParseError> {
        if let Ok(cache) = self.system().expressions.lock()
            && let Some(parsed) = cache.get(text)
        {
            return Ok(Arc::clone(parsed));
        }
        let parsed = Arc::new(sct_ecl::parse(text)?);
        if let Ok(mut cache) = self.system().expressions.lock() {
            if cache.len() >= 256 {
                cache.clear();
            }
            cache.insert(text.to_owned(), Arc::clone(&parsed));
        }
        Ok(parsed)
    }

    /// The concepts an expression constraint selects; malformed text is an
    /// invalid filter value with the parser's position, an identifier the
    /// edition lacks an invalid code.
    pub(super) fn constraint(&self, text: &str) -> Result<ConceptSet, ProviderError> {
        let parsed = self.expression(text).map_err(|error| {
            ProviderError::InvalidFilterValue(Box::new(InvalidValue {
                property: String::from("constraint"),
                value: text.to_owned(),
                reason: error.to_string(),
                position: Some(error.offset()),
                source: Some(error),
            }))
        })?;
        sct_ecl::eval::evaluate(self, &parsed).map_err(|error| super::ecl::refusal(error, text))
    }
}

/// Decodes the percent-encoding of a URI component
/// (<https://www.rfc-editor.org/rfc/rfc3986#section-2.1>); `None` for a
/// stray `%` or bytes that are not UTF-8.
pub(super) fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'%' {
            let hex = bytes.get(i.checked_add(1)?..i.checked_add(3)?)?;
            let text = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(text, 16).ok()?);
            i = i.checked_add(3)?;
        } else {
            out.push(byte);
            i = i.checked_add(1)?;
        }
    }
    String::from_utf8(out).ok()
}

/// The byte offset into `url` of the byte at `offset` in the decoding of
/// `argument`, the encoded expression constraint that ends `url`.
pub(super) fn url_offset(url: &str, argument: &str, offset: usize) -> Option<usize> {
    let start = url.len().checked_sub(argument.len())?;
    start.checked_add(crate::position::encoded_offset(argument, offset)?)
}

/// The base (the system, edition, or version URI) and the form of an implicit
/// URI whose query key is `key` (`fhir_vs` or `fhir_cm`), when `url` has the
/// shape (<https://hl7.org/fhir/R4B/snomedct.html>).
pub(super) fn implicit_parts<'a>(url: &'a str, key: &str) -> Option<(&'a str, &'a str)> {
    let (base, query) = url.split_once('?')?;
    let form = match query.strip_prefix(key)? {
        "" => "",
        rest => rest.strip_prefix('=')?,
    };
    (base == SYSTEM || base.starts_with("http://snomed.info/sct/")).then_some((base, form))
}

/// The implicit value set forms the provider resolves: the five `?fhir_vs`
/// forms of <https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets".
pub(super) fn implicit_forms() -> Vec<ImplicitForm> {
    vec![
        ImplicitForm::new("http://snomed.info/sct?fhir_vs", ImplicitArgument::None),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=isa/[sctid]",
            ImplicitArgument::Code,
        ),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=refset",
            ImplicitArgument::None,
        ),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=refset/[sctid]",
            ImplicitArgument::Code,
        ),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=ecl/[ecl]",
            ImplicitArgument::Expression,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{implicit_parts, percent_decode};
    use crate::snomed::{FHIR_CM, FHIR_VS};

    #[test]
    fn percent_encoding_decodes_and_a_stray_percent_is_refused() {
        assert_eq!(
            percent_decode("%3C%3C%20404684003%20%7Cfinding%7C").as_deref(),
            Some("<< 404684003 |finding|")
        );
        assert_eq!(percent_decode("<< 1").as_deref(), Some("<< 1"));
        assert_eq!(percent_decode("%3"), None);
        assert_eq!(percent_decode("%zz"), None);
    }

    #[test]
    fn implicit_urls_split_into_base_and_form() {
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_vs", FHIR_VS),
            Some(("http://snomed.info/sct", ""))
        );
        assert_eq!(
            implicit_parts(
                "http://snomed.info/sct/11000146104/version/20260630?fhir_vs=isa/1",
                FHIR_VS
            ),
            Some((
                "http://snomed.info/sct/11000146104/version/20260630",
                "isa/1"
            ))
        );
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_cm=1", FHIR_CM),
            Some(("http://snomed.info/sct", "1"))
        );
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_cm=1", FHIR_VS),
            None,
            "one key never reads the other's URI"
        );
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_vs", FHIR_CM),
            None
        );
        assert_eq!(implicit_parts("http://loinc.org/vs", FHIR_VS), None);
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_vsx", FHIR_VS),
            None
        );
    }
}
