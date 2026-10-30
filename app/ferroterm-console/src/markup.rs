// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shorthands the screens write their own markup with, beside the
//! library's parts: markup made here from escaped parts, and text escaped
//! for it.
//!
//! Every value a screen writes into its own markup goes through [`t`] (or a
//! library part, which escapes as it draws), so a code, a display or an
//! address the server answered never reaches the page as markup. No
//! specification governs this: our own design.

use ferrohealth_ui::html::Html;

/// The markup `markup`, made by the screen from escaped parts.
#[must_use]
pub(crate) const fn h(markup: String) -> Html {
    Html::raw(markup)
}

/// `text` escaped as a text node or an attribute value.
#[must_use]
pub(crate) fn t(text: impl AsRef<str>) -> Html {
    Html::text(text)
}

/// `text` escaped inside a `<code>` element.
#[must_use]
pub(crate) fn code(text: impl AsRef<str>) -> Html {
    h(format!("<code>{}</code>", t(text)))
}

/// `text` escaped inside a `<strong>` element.
#[must_use]
pub(crate) fn strong(text: impl AsRef<str>) -> Html {
    h(format!("<strong>{}</strong>", t(text)))
}

/// `body` inside a `<p>` element.
#[must_use]
pub(crate) fn p(body: impl Into<Html>) -> Html {
    h(format!("<p>{}</p>", body.into()))
}

/// A link to `href` reading `body`, the address escaped.
#[must_use]
pub(crate) fn link(href: impl AsRef<str>, body: impl Into<Html>) -> Html {
    h(format!(r#"<a href="{}">{}</a>"#, t(href), body.into()))
}

/// The parts `parts`, one after another.
#[must_use]
pub(crate) fn join(parts: impl IntoIterator<Item = Html>) -> Html {
    Html::join(parts)
}

#[cfg(test)]
mod tests {
    use super::{code, link, p, t};

    #[test]
    fn every_value_a_screen_writes_is_escaped() {
        assert_eq!("&#60;b&#62;", t("<b>").as_str());
        assert_eq!("<code>a&#38;b</code>", code("a&b").as_str());
        assert_eq!(
            r#"<a href="/x?a=1&#38;b=&#34;2&#34;">go</a>"#,
            link("/x?a=1&b=\"2\"", t("go")).as_str()
        );
        assert_eq!("<p>&#60;i&#62;</p>", p(t("<i>")).as_str());
    }
}
