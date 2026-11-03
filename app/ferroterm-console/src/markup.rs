// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shorthands the screens write their own markup with, beside the
//! library's parts: a code, a paragraph, strong text and a link, each made
//! from escaped parts.
//!
//! Every value a screen writes into its own markup goes through
//! [`Html::text`] (or a
//! library part, which escapes as it draws), so a code, a display or an
//! address the server answered never reaches the page as markup. No
//! specification governs this: our own design.

use ferrohealth_ui::html::Html;

/// `text` escaped inside a `<code>` element.
#[must_use]
pub(crate) fn code(text: impl AsRef<str>) -> Html {
    Html::raw(format!("<code>{}</code>", Html::text(text)))
}

/// `text` escaped inside a `<strong>` element.
#[must_use]
pub(crate) fn strong(text: impl AsRef<str>) -> Html {
    Html::raw(format!("<strong>{}</strong>", Html::text(text)))
}

/// `body` inside a `<p>` element.
#[must_use]
pub(crate) fn p(body: impl Into<Html>) -> Html {
    Html::raw(format!("<p>{}</p>", body.into()))
}

/// A link to `href` reading `body`, the address escaped.
#[must_use]
pub(crate) fn link(href: impl AsRef<str>, body: impl Into<Html>) -> Html {
    Html::raw(format!(
        r#"<a href="{}">{}</a>"#,
        Html::text(href),
        body.into()
    ))
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::html::Html;

    use super::{code, link, p};

    #[test]
    fn every_value_a_screen_writes_is_escaped() {
        assert_eq!("&#60;b&#62;", Html::text("<b>").as_str());
        assert_eq!("<code>a&#38;b</code>", code("a&b").as_str());
        assert_eq!(
            r#"<a href="/x?a=1&#38;b=&#34;2&#34;">go</a>"#,
            link("/x?a=1&b=\"2\"", Html::text("go")).as_str()
        );
        assert_eq!("<p>&#60;i&#62;</p>", p(Html::text("<i>")).as_str());
    }
}
