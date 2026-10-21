// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The wire format of a request and a response: FHIR JSON or FHIR XML.
//!
//! A client names the response format with `_format` in the query, else with
//! `Accept`; a request body's format is its `Content-Type`
//! (<https://hl7.org/fhir/R4B/http.html#mime-type>). JSON is the default. XML
//! goes through the generated codec's schema of the served version
//! (<https://hl7.org/fhir/R4B/xml.html>).

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use fhir_types::codec::Object;
use fhir_types::schema::Schemas;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, StatusCode};

use crate::outcome::Failure;

/// The FHIR JSON media type of a response.
pub const FHIR_JSON: &str = "application/fhir+json; charset=utf-8";
/// The FHIR XML media type of a response.
pub const FHIR_XML: &str = "application/fhir+xml; charset=utf-8";
/// The query parameter naming the format.
pub const FORMAT: &str = "_format";

/// A wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wire {
    /// FHIR JSON.
    #[default]
    Json,
    /// FHIR XML.
    Xml,
}

impl Wire {
    /// The format a media type or `_format` value names; `None` for one the
    /// server does not speak. The wildcards and the plain JSON types count as
    /// JSON (<https://hl7.org/fhir/R4B/http.html#mime-type>), and so do the
    /// DSTU2 types the same section lets a server keep accepting.
    #[must_use]
    pub fn of_media(media: &str) -> Option<Self> {
        let media = media.split(';').next().unwrap_or("").trim();
        match media.to_ascii_lowercase().as_str() {
            "json"
            | "application/json"
            | "application/fhir+json"
            | "application/json+fhir"
            | "*/*"
            | "application/*" => Some(Self::Json),
            "xml"
            | "text/xml"
            | "application/xml"
            | "application/fhir+xml"
            | "application/xml+fhir" => Some(Self::Xml),
            _ => None,
        }
    }

    /// The format an `Accept` header value selects, `None` when it accepts
    /// neither.
    ///
    /// Each format takes the weight of the most specific media range that
    /// matches it, and the heavier format wins, JSON on a tie; a weight of `0`
    /// excludes (<https://www.rfc-editor.org/rfc/rfc9110#section-12.5.1>). A
    /// value with no element accepts anything, like an absent header.
    fn of_accept(accept: &str) -> Option<Self> {
        let elements: Vec<&str> = split_unquoted(accept, ',')
            .filter(|element| !element.trim().is_empty())
            .collect();
        if elements.is_empty() {
            return Some(Self::Json);
        }
        let ranges: Vec<Range<'_>> = elements.into_iter().filter_map(Range::parse).collect();
        let json = Self::Json.weight(&ranges);
        let xml = Self::Xml.weight(&ranges);
        match (json, xml) {
            (0, 0) => None,
            (json, xml) if xml > json => Some(Self::Xml),
            _ => Some(Self::Json),
        }
    }

    /// The weight `ranges` give this format, in thousandths: that of the most
    /// specific matching range, the heaviest among equally specific ones, and
    /// `0` when none matches.
    fn weight(self, ranges: &[Range<'_>]) -> u16 {
        ranges
            .iter()
            .filter_map(|range| range.specificity(self).map(|s| (s, range.weight)))
            .max()
            .map_or(0, |(_, weight)| weight)
    }

    /// The response format a request asks for: `_format` in the query first,
    /// then the most preferred acceptable media range of `Accept`; JSON when
    /// neither says.
    ///
    /// # Errors
    ///
    /// Returns a `406` failure for a `_format` the server does not speak, and
    /// for an `Accept` that accepts neither FHIR JSON nor FHIR XML
    /// (<https://hl7.org/fhir/R4B/http.html#mime-type>).
    pub fn negotiate(query: &[(String, String)], headers: &HeaderMap) -> Result<Self, Failure> {
        if let Some((_, value)) = query.iter().find(|(name, _)| name == FORMAT) {
            return Self::of_media(value).ok_or_else(|| {
                Failure::new(
                    StatusCode::NOT_ACCEPTABLE,
                    "not-supported",
                    format!("`_format={value}` is not FHIR JSON or FHIR XML"),
                )
            });
        }
        // NOTE: RFC 9110 section 5.3 joins repeated field lines with commas; a
        // line that is not visible ASCII names no media range and is skipped.
        let accept = headers
            .get_all(http::header::ACCEPT)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join(",");
        Self::of_accept(&accept).ok_or_else(|| {
            Failure::new(
                StatusCode::NOT_ACCEPTABLE,
                "not-supported",
                format!("`Accept: {accept}` accepts neither FHIR JSON nor FHIR XML"),
            )
        })
    }

    /// The format of a request body, by `Content-Type`.
    ///
    /// # Errors
    ///
    /// Returns a `415` failure for a media type other than FHIR JSON or XML.
    pub fn of_body(headers: &HeaderMap) -> Result<Self, Failure> {
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let media = content_type.split(';').next().unwrap_or("").trim();
        match Self::of_media(media) {
            Some(wire) if media != "*/*" && media != "application/*" => Ok(wire),
            _ => Err(Failure::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "not-supported",
                format!("content type `{media}` is not FHIR JSON or FHIR XML"),
            )),
        }
    }

    /// The `Content-Type` of a response in this format.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Json => FHIR_JSON,
            Self::Xml => FHIR_XML,
        }
    }

    /// A response carrying `object`, a resource in the JSON object model, in
    /// this format; `schemas` is the served version's XML schema.
    #[must_use]
    pub fn response(self, status: StatusCode, object: &Object, schemas: &Schemas) -> Response {
        let body = match self {
            // NOTE: the map serializes by reference
            // (<https://docs.rs/serde_json/1/serde_json/fn.to_vec.html>); wrapping it
            // in a `Value` would deep-copy the whole resource to serialize it.
            Self::Json => match serde_json::to_vec(object) {
                Ok(json) => json,
                Err(_) => return status.into_response(),
            },
            Self::Xml => match fhir_types::xml::to_xml(schemas, object) {
                Ok(xml) => xml.into_bytes(),
                // NOTE: a resource the server built always has an XML form; if the
                // codec ever refuses, the status alone still tells the client.
                Err(_) => return status.into_response(),
            },
        };
        self.body(status, body)
    }

    /// A response carrying `resource`, a typed FHIR resource, in this format;
    /// `schemas` is the served version's XML schema.
    ///
    /// JSON writes the resource straight to bytes through the generated
    /// `Serialize`, with no intermediate document. XML converts through the
    /// object model its schema reads (<https://hl7.org/fhir/R4B/xml.html>).
    ///
    /// # Errors
    ///
    /// Returns a `500` failure when the resource has no form in this wire.
    pub fn resource<R: fhir_types::codec::Json + serde::Serialize>(
        self,
        status: StatusCode,
        resource: &R,
        schemas: &Schemas,
    ) -> Result<Response, Failure> {
        let body = match self {
            Self::Json => serde_json::to_vec(resource).map_err(|e| encoding(&e))?,
            Self::Xml => {
                let object = resource.to_json().map_err(|e| encoding(&e))?;
                fhir_types::xml::to_xml(schemas, &object)
                    .map_err(|e| encoding(&e))?
                    .into_bytes()
            }
        };
        Ok(self.body(status, body))
    }

    /// A resource already in its wire form, for an answer no typed value
    /// can hold.
    ///
    /// A `_elements` projection is the case: a subset of a resource is not a
    /// resource, so it has no typed form to serialize from
    /// (<https://hl7.org/fhir/R5/search.html#elements>).
    ///
    /// # Errors
    ///
    /// Returns the `500` of an object with no wire form.
    pub fn object(
        self,
        status: StatusCode,
        object: &Object,
        schemas: &Schemas,
    ) -> Result<Response, Failure> {
        let body = match self {
            Self::Json => serde_json::to_vec(object).map_err(|e| encoding(&e))?,
            Self::Xml => fhir_types::xml::to_xml(schemas, object)
                .map_err(|e| encoding(&e))?
                .into_bytes(),
        };
        Ok(self.body(status, body))
    }

    /// The response of `body` with this format's `Content-Type`.
    fn body(self, status: StatusCode, body: Vec<u8>) -> Response {
        Response::builder()
            .status(status)
            .header(CONTENT_TYPE, self.content_type())
            .body(Body::from(body))
            .unwrap_or_else(|_| status.into_response())
    }
}

/// One media range of an `Accept` header and its weight.
#[derive(Debug)]
struct Range<'a> {
    /// The `type/subtype`, possibly a wildcard, without its parameters.
    media: &'a str,
    /// The weight, in thousandths.
    weight: u16,
}

impl<'a> Range<'a> {
    /// The range of one `Accept` element; `None` for an element whose weight
    /// is not a `qvalue` (<https://www.rfc-editor.org/rfc/rfc9110#section-12.4.2>).
    fn parse(element: &'a str) -> Option<Self> {
        let mut parts = split_unquoted(element, ';');
        let media = parts.next().unwrap_or("").trim();
        let mut weight = 1000;
        for parameter in parts {
            let Some((name, value)) = parameter.split_once('=') else {
                continue;
            };
            if name.trim().eq_ignore_ascii_case("q") {
                // NOTE: no FHIR/SNOMED spec governs this: our own design; an element
                // with a malformed weight is dropped, so it accepts nothing.
                weight = qvalue(value.trim())?;
            }
        }
        Some(Self { media, weight })
    }

    /// How specifically this range names `wire`: `3` for one of its own media
    /// types, `2` for `application/*`, `1` for `*/*`, `None` for no match.
    fn specificity(&self, wire: Wire) -> Option<u8> {
        if self.media == "*/*" {
            Some(1)
        } else if self.media.eq_ignore_ascii_case("application/*") {
            Some(2)
        } else if Wire::of_media(self.media) == Some(wire) {
            Some(3)
        } else {
            None
        }
    }
}

/// A `qvalue` in thousandths: `0` to `1` with at most three decimals
/// (<https://www.rfc-editor.org/rfc/rfc9110#section-12.4.2>).
fn qvalue(text: &str) -> Option<u16> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let thousandths = format!("{fraction:0<3}").parse::<u16>().ok()?;
    match whole {
        "0" => Some(thousandths),
        "1" if thousandths == 0 => Some(1000),
        _ => None,
    }
}

/// The pieces of `text` between each `separator` outside a quoted string
/// (<https://www.rfc-editor.org/rfc/rfc9110#section-5.6.4>).
fn split_unquoted(text: &str, separator: char) -> impl Iterator<Item = &str> {
    let mut quoted = false;
    let mut escaped = false;
    text.split(move |c: char| {
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == separator && !quoted {
            return true;
        }
        false
    })
}

/// The `500` a resource with no wire form answers with.
fn encoding(error: &dyn std::fmt::Display) -> Failure {
    Failure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "exception",
        format!("cannot encode the response: {error}"),
    )
}

/// The query without its `_format`, for the operation's own parameters.
#[must_use]
pub fn without_format(query: &[(String, String)]) -> Vec<(String, String)> {
    query
        .iter()
        .filter(|(name, _)| name != FORMAT)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qvalue_is_zero_to_one_with_at_most_three_decimals() {
        assert_eq!(qvalue("1"), Some(1000));
        assert_eq!(qvalue("1.000"), Some(1000));
        assert_eq!(qvalue("0"), Some(0));
        assert_eq!(qvalue("0.5"), Some(500));
        assert_eq!(qvalue("0.001"), Some(1));
        for malformed in ["1.001", "0.0001", "2", "-0", ".5", "0.a", "", "1e0"] {
            assert_eq!(qvalue(malformed), None, "`{malformed}`");
        }
    }

    #[test]
    fn a_comma_inside_a_quoted_parameter_does_not_split_the_list() {
        let elements: Vec<&str> =
            split_unquoted(r#"text/x;a="1,\"2", application/fhir+xml"#, ',').collect();
        assert_eq!(elements.len(), 2, "{elements:?}");
        assert_eq!(
            Wire::of_accept(r#"text/x;a="b,c", application/fhir+xml"#),
            Some(Wire::Xml)
        );
    }

    #[test]
    fn an_empty_accept_accepts_anything() {
        assert_eq!(Wire::of_accept(""), Some(Wire::Json));
        assert_eq!(Wire::of_accept(" , "), Some(Wire::Json));
    }

    #[test]
    fn the_more_specific_range_sets_the_weight() {
        assert_eq!(
            Wire::of_accept("application/*;q=0.1, application/fhir+xml;q=0.2"),
            Some(Wire::Xml)
        );
        assert_eq!(
            Wire::of_accept("application/fhir+json;q=0, application/*"),
            Some(Wire::Xml)
        );
        assert_eq!(Wire::of_accept("application/json+fhir"), Some(Wire::Json));
    }
}
