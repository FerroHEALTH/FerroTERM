// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! URL building for every address the viewer emits.
//!
//! A code system URI, an ECL expression, a value set canonical, and a search
//! term all carry characters that are structural in a URL, so no value is ever
//! written into a path or a query by `format!`. Both encoders below follow
//! RFC 3986 (<https://www.rfc-editor.org/rfc/rfc3986>).

use percent_encoding::AsciiSet;
use percent_encoding::NON_ALPHANUMERIC;
use percent_encoding::utf8_percent_encode;

/// The characters RFC 3986 §2.3 calls unreserved, which never need encoding.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// A path segment keeps `pchar` (RFC 3986 §3.3: unreserved, sub-delims, `:`,
/// `@`), so an operation name such as `$expand` survives unencoded while `/`
/// and `?` are escaped back into the segment they belong to.
const PATH_SEGMENT: &AsciiSet = &UNRESERVED
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b':')
    .remove(b'@');

/// Percent-encodes `value` for use as one path segment.
pub(crate) fn encode_path_segment(value: &str) -> String {
    utf8_percent_encode(value, PATH_SEGMENT).collect()
}

/// Percent-encodes `value` for use as a query parameter name or value.
///
/// Everything outside the unreserved set is escaped. That is stricter than
/// RFC 3986 §3.4 allows, and deliberately so: `&`, `=`, `?`, and `+` all
/// change the meaning of the query they sit in, and `+` additionally decodes
/// as a space under the form-encoding rules browsers apply.
pub(crate) fn encode_query_component(value: &str) -> String {
    utf8_percent_encode(value, UNRESERVED).collect()
}

/// Reads back one value [`encode_query_component`] wrote.
///
/// A value the encoder wrote is valid UTF-8 once decoded; one a reader typed
/// may not be, and a byte sequence that is not UTF-8 is replaced rather than
/// refused, because the address is what the reader sees and an error page
/// would hide it.
pub(crate) fn decode_query_component(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

/// A path and query built from encoded parts, rendered relative to a root.
///
/// The same type builds a FHIR request and an in-application link, because
/// both are a path with a query and both carry values the reader supplied.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RequestUrl {
    path: String,
    query: Vec<(String, String)>,
}

impl RequestUrl {
    /// Starts an empty URL, which renders as the root itself.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Appends one percent-encoded path segment.
    #[must_use]
    pub(crate) fn segment(mut self, segment: &str) -> Self {
        self.path.push('/');
        self.path.push_str(&encode_path_segment(segment));
        self
    }

    /// Appends one query parameter, with the name and the value encoded.
    #[must_use]
    pub(crate) fn query(mut self, name: &str, value: &str) -> Self {
        self.query
            .push((encode_query_component(name), encode_query_component(value)));
        self
    }

    /// Renders the URL under `root`, which is an origin or the empty string.
    ///
    /// An empty root yields a same-origin relative URL, which is what the
    /// browser resolves against the page the bundle was served from.
    pub(crate) fn render(&self, root: &str) -> String {
        let mut rendered = String::with_capacity(root.len() + self.path.len());
        rendered.push_str(root);
        rendered.push_str(&self.path);
        for (index, (name, value)) in self.query.iter().enumerate() {
            rendered.push(if index == 0 { '?' } else { '&' });
            rendered.push_str(name);
            rendered.push('=');
            rendered.push_str(value);
        }
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_system_uri_survives_a_query_value_intact() {
        // A SNOMED CT implicit value set canonical carries `:`, `/`, `?`, and
        // `=`, every one of which would otherwise truncate the query.
        let encoded = encode_query_component("http://snomed.info/sct?fhir_vs=isa/404684003");
        assert_eq!(
            encoded, "http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs%3Disa%2F404684003",
            "every structural character in a system canonical is escaped"
        );
    }

    #[test]
    fn an_ecl_expression_survives_a_query_value_intact() {
        let encoded = encode_query_component("<< 404684003 |Clinical finding|");
        assert_eq!(
            encoded, "%3C%3C%20404684003%20%7CClinical%20finding%7C",
            "the ECL operators, pipes, and spaces are escaped"
        );
    }

    #[test]
    fn an_interior_newline_never_reaches_the_url() {
        let encoded = encode_query_component("fever\nand chills");
        assert_eq!(
            encoded, "fever%0Aand%20chills",
            "trimming strips only the ends, so the encoder carries the interior"
        );
    }

    #[test]
    fn non_ascii_search_terms_are_encoded_as_utf8() {
        let encoded = encode_query_component("koorts é");
        assert_eq!(encoded, "koorts%20%C3%A9", "the encoder emits UTF-8 bytes");
    }

    #[test]
    fn a_path_segment_keeps_an_operation_name_but_escapes_a_separator() {
        assert_eq!(
            encode_path_segment("$expand"),
            "$expand",
            "`$` is a sub-delim and legal in a segment (RFC 3986 section 3.3)"
        );
        assert_eq!(
            encode_path_segment("a/b?c"),
            "a%2Fb%3Fc",
            "a separator is escaped back into the segment it belongs to"
        );
    }

    #[test]
    fn a_query_value_survives_the_encoder_and_the_decoder() {
        for value in [
            "fever",
            "a,b",
            "koorts é",
            "<< 404684003 |Clinical finding|",
        ] {
            assert_eq!(
                decode_query_component(&encode_query_component(value)),
                value,
                "a value the viewer wrote reads back as it was written"
            );
        }
    }

    #[test]
    fn a_decoded_value_that_is_not_utf8_is_replaced_rather_than_refused() {
        assert_eq!(
            decode_query_component("%FF"),
            "\u{fffd}",
            "the address is what the reader sees, so it renders rather than failing"
        );
    }

    #[test]
    fn an_empty_url_renders_as_the_root() {
        assert_eq!(
            RequestUrl::new().render("https://tx.example.org"),
            "https://tx.example.org",
            "no segments means no trailing slash"
        );
    }

    #[test]
    fn segments_and_queries_render_in_the_order_they_were_added() {
        let url = RequestUrl::new()
            .segment("r4b")
            .segment("ValueSet")
            .segment("$expand")
            .query("url", "http://snomed.info/sct?fhir_vs")
            .query("count", "20");
        assert_eq!(
            url.render(""),
            "/r4b/ValueSet/$expand?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs&count=20",
            "the first parameter opens with `?` and the rest with `&`"
        );
    }

    #[test]
    fn an_empty_root_renders_a_same_origin_relative_url() {
        assert_eq!(
            RequestUrl::new().segment("health").render(""),
            "/health",
            "a relative URL resolves against the page the bundle came from"
        );
    }
}
