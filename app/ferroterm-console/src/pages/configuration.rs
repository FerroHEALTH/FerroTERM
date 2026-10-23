// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: the console's own settings as its file and environment set
//! them, read-only (FerroUI's family rule 13: a product with no
//! configuration store runs in file mode), one tab per section.
//!
//! The server keeps its own settings; the Server screen shows what it
//! declares. A secret is shown by reference only, never its value.

use std::time::Duration;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::dialog::{Explanation, info_dialog};
use ferrohealth_ui::view::secret::{SecretState, secret_state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::Header;
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::settings::{How, Setting, SettingsPage, Source, settings_page};
use ferrohealth_ui::widget::action_menu::{Act, Action, Item, Menu};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

use crate::config::settings::Settings;
use crate::locale::{Msg, tr, tr_with};
use crate::routes::{CONFIGURATION_PATH, ui_path};
use crate::server::ConsoleState;

/// The id of the dialog that says how configuration works.
const HOW_DIALOG: &str = "config-how";

/// The id of the title row's More menu.
const MORE_MENU: &str = "config-more";

/// The file the console reads when it was started with none named.
const DEFAULT_FILE: &str = "console.toml";

/// The four tabs, each a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigTab {
    /// Where the console reaches the server.
    Connection,
    /// The OpenID Provider readers sign in at.
    SignIn,
    /// The sign-in sessions.
    Sessions,
    /// The listener and the site root.
    Listener,
}

impl ConfigTab {
    /// Every tab, in order.
    const ALL: [Self; 4] = [
        Self::Connection,
        Self::SignIn,
        Self::Sessions,
        Self::Listener,
    ];

    /// The tab the route's segment names: none for Connection.
    #[must_use]
    pub(crate) fn of(segment: Option<&str>) -> Option<Self> {
        match segment {
            None => Some(Self::Connection),
            Some(key) => Self::ALL
                .into_iter()
                .find(|tab| *tab != Self::Connection && tab.key() == key),
        }
    }

    /// The tab's key, its route's segment.
    const fn key(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::SignIn => "sign-in",
            Self::Sessions => "sessions",
            Self::Listener => "listener",
        }
    }

    /// The tab's words.
    fn words(self) -> String {
        tr(match self {
            Self::Connection => Msg::ConfigConnection,
            Self::SignIn => Msg::ConfigSignIn,
            Self::Sessions => Msg::ConfigSessions,
            Self::Listener => Msg::ConfigListener,
        })
    }

    /// The tab's route.
    fn href(self) -> String {
        match self {
            Self::Connection => ui_path(CONFIGURATION_PATH),
            Self::SignIn | Self::Sessions | Self::Listener => {
                format!("{}/{}", ui_path(CONFIGURATION_PATH), self.key())
            }
        }
    }
}

/// A setting row of `label` and `description`, the value `value`, set by
/// the key `key`.
fn row(label: Msg, description: Msg, value: AnyView, key: &str) -> Setting {
    Setting {
        label: tr(label),
        description: tr(description),
        value,
        keys: vec![key.to_owned()],
        next_start: false,
    }
}

/// A value in the code face.
fn code(text: impl Into<String>) -> AnyView {
    let text = text.into();
    view! { <code>{text}</code> }.into_any()
}

/// A duration in whole seconds, minutes or hours, the largest that is whole.
fn duration(value: Duration) -> String {
    let seconds = value.as_secs();
    if seconds != 0 && seconds.is_multiple_of(3_600) {
        tr_with(
            Msg::ConfigHours,
            &[("count", seconds.div_euclid(3_600).into())],
        )
    } else if seconds != 0 && seconds.is_multiple_of(60) {
        tr_with(
            Msg::ConfigMinutes,
            &[("count", seconds.div_euclid(60).into())],
        )
    } else if value.subsec_millis() == 0 {
        tr_with(Msg::ConfigSeconds, &[("count", seconds.into())])
    } else {
        tr_with(
            Msg::ConfigMilliseconds,
            &[(
                "count",
                u64::try_from(value.as_millis()).unwrap_or(u64::MAX).into(),
            )],
        )
    }
}

/// On or Off, as a badge with its word.
fn on(value: bool) -> AnyView {
    if value {
        badge(Tone::Ok, tr(Msg::ConfigOn)).into_any()
    } else {
        badge(Tone::Neutral, tr(Msg::ConfigOff)).into_any()
    }
}

/// The settings of `tab` from `settings`.
fn settings_of(tab: ConfigTab, settings: &Settings) -> Vec<Setting> {
    match tab {
        ConfigTab::Connection => connection(settings),
        ConfigTab::SignIn => sign_in(settings),
        ConfigTab::Sessions => sessions(settings),
        ConfigTab::Listener => listener(settings),
    }
}

/// The settings of the terminology server and the sync service.
fn connection(settings: &Settings) -> Vec<Setting> {
    let sync = settings.ferroterm.sync.as_ref().map_or_else(
        || view! { <span class="fui-muted">{tr(Msg::ConfigNotSet)}</span> }.into_any(),
        |url| code(url.as_str()),
    );
    vec![
        row(
            Msg::ConfigServer,
            Msg::ConfigServerLine,
            code(settings.ferroterm.base.as_str()),
            "ferroterm.base_url",
        ),
        row(
            Msg::ConfigPublic,
            Msg::ConfigPublicLine,
            code(settings.ferroterm.public.as_str()),
            "ferroterm.public_url",
        ),
        row(
            Msg::ConfigSync,
            Msg::ConfigSyncLine,
            sync,
            "ferroterm.sync_url",
        ),
        row(
            Msg::ConfigTimeout,
            Msg::ConfigTimeoutLine,
            duration(settings.ferroterm.timeout).into_any(),
            "ferroterm.timeout_ms",
        ),
    ]
}

/// The settings of the OpenID Provider.
fn sign_in(settings: &Settings) -> Vec<Setting> {
    settings.oidc.as_ref().map_or_else(Vec::new, |oidc| {
        let secret = match (&oidc.client_secret_file, &oidc.client_secret) {
            (Some(file), _) => SecretState::Set(file.display().to_string()),
            (None, Some(_)) => SecretState::Redacted,
            (None, None) => SecretState::Unset,
        };
        let sign_out = if oidc.end_session_endpoint.is_some() {
            tr(Msg::ConfigSetUp)
        } else {
            tr(Msg::ConfigNotSet)
        };
        vec![
            row(
                Msg::ConfigIssuer,
                Msg::ConfigIssuerLine,
                code(oidc.issuer.clone()),
                "oidc.issuer",
            ),
            row(
                Msg::ConfigClient,
                Msg::ConfigClientLine,
                code(oidc.client_id.clone()),
                "oidc.client_id",
            ),
            row(
                Msg::ConfigSecret,
                Msg::ConfigSecretLine,
                secret_state(&secret),
                "oidc.client_secret_file",
            ),
            row(
                Msg::ConfigRedirect,
                Msg::ConfigRedirectLine,
                code(oidc.redirect_uri.as_str()),
                "oidc.redirect_uri",
            ),
            row(
                Msg::ConfigScopes,
                Msg::ConfigScopesLine,
                code(oidc.scopes.join(" ")),
                "oidc.scopes",
            ),
            row(
                Msg::ConfigSignOut,
                Msg::ConfigSignOutLine,
                sign_out.into_any(),
                "oidc.end_session_endpoint",
            ),
        ]
    })
}

/// The settings of the sessions.
fn sessions(settings: &Settings) -> Vec<Setting> {
    vec![
        row(
            Msg::ConfigSecureCookie,
            Msg::ConfigSecureCookieLine,
            on(settings.session.secure_cookie),
            "session.secure_cookie",
        ),
        row(
            Msg::ConfigIdle,
            Msg::ConfigIdleLine,
            duration(settings.session.idle_timeout).into_any(),
            "session.idle_timeout_s",
        ),
        row(
            Msg::ConfigAbsolute,
            Msg::ConfigAbsoluteLine,
            duration(settings.session.absolute_timeout).into_any(),
            "session.absolute_timeout_s",
        ),
        row(
            Msg::ConfigLimit,
            Msg::ConfigLimitLine,
            ferrohealth_ui::i18n::number::grouped(
                u64::try_from(settings.session.max_sessions).unwrap_or(u64::MAX),
            )
            .into_any(),
            "session.max_sessions",
        ),
    ]
}

/// The settings of the listener.
fn listener(settings: &Settings) -> Vec<Setting> {
    vec![
        row(
            Msg::ConfigListen,
            Msg::ConfigListenLine,
            code(settings.listen.to_string()),
            "server.listen",
        ),
        row(
            Msg::ConfigSiteRoot,
            Msg::ConfigSiteRootLine,
            code(settings.site_root.display().to_string()),
            "server.site_root",
        ),
    ]
}

/// The title row's actions: a More menu that copies the path of the
/// configuration file `file`, and none when the console was started with no
/// file named.
fn more(file: Option<&std::path::Path>) -> ActionGroup {
    let Some(file) = file else {
        return ActionGroup::none();
    };
    let path = file.display().to_string();
    ActionGroup::more_only(Menu::new(
        MORE_MENU,
        tr(Msg::ConfigMore),
        vec![
            Item::new(Action::new(
                Some(Glyph::COPY),
                tr(Msg::ConfigCopyPath),
                Act::Copy(path.clone()),
            ))
            .note(path),
        ],
    ))
}

/// Configuration, on the tab the route names.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn ConfigurationPage() -> impl IntoView {
    let params = use_params_map();
    let segment = params.with_untracked(|params| params.get("tab"));
    let Some(tab) = ConfigTab::of(segment.as_deref()) else {
        return crate::pages::errors::not_found_page();
    };
    let Some(state) = use_context::<ConsoleState>() else {
        return crate::pages::errors::not_found_page();
    };
    let settings = state.settings();
    let shown_file = settings
        .file
        .as_ref()
        .and_then(|path| path.file_name())
        .map_or_else(
            || String::from(DEFAULT_FILE),
            |name| name.to_string_lossy().into_owned(),
        );
    let mut rows = settings_of(tab, settings);
    let empty = tab == ConfigTab::SignIn && rows.is_empty();
    if empty {
        rows.push(Setting {
            label: tr(Msg::ConfigNoSignIn),
            description: tr(Msg::ConfigNoSignInLine),
            value: ().into_any(),
            keys: vec![String::from("oidc")],
            next_start: false,
        });
    }
    let page = settings_page(SettingsPage {
        header: Header {
            title: tr(Msg::ShellConfiguration).into(),
            subtitle: Some(
                tr_with(Msg::ConfigSubtitle, &[("file", shown_file.clone().into())]).into(),
            ),
            subtitle_view: None,
            crumbs: Vec::new(),
            mono: false,
            badge: None,
        },
        actions: more(settings.file.as_deref()),
        source: Source::File {
            file: shown_file.clone(),
            how: How::Dialog(HOW_DIALOG.into()),
        },
        tabs: Some(TabRow {
            label: tr(Msg::ConfigSections),
            tabs: ConfigTab::ALL
                .into_iter()
                .map(|each| Tab::new(each.key(), each.href(), each.words()))
                .collect(),
            current: tab.key().to_owned(),
        }),
        settings: rows,
        replica: None,
    });
    let how = info_dialog(
        Explanation {
            id: HOW_DIALOG.into(),
            title: tr(Msg::ConfigHow).into(),
        },
        view! {
            <p>{tr_with(Msg::ConfigHowRead, &[("file", shown_file.into())])}</p>
            <p>{tr(Msg::ConfigHowChange)}</p>
            <p>{tr(Msg::ConfigHowShown)}</p>
        },
    );
    view! {
        <Title text=tr(Msg::ShellConfiguration) />
        {page}
        {how}
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use std::path::Path;

    use ferrohealth_ui::widget::action_menu::Act;

    use super::{ConfigTab, duration, more};

    #[test]
    fn more_copies_the_path_of_the_file_named() {
        assert!(
            more(None).is_empty(),
            "with no file named there is no path to copy"
        );
        let group = more(Some(Path::new("/etc/ferroterm/console.toml")));
        let menu = group
            .more
            .expect("a named file gives the title row its More menu");
        let [item] = menu.items.as_slice() else {
            panic!("one item, not {}", menu.items.len());
        };
        assert_eq!(
            Act::Copy(String::from("/etc/ferroterm/console.toml")),
            item.action.act
        );
        assert_eq!(Some("/etc/ferroterm/console.toml"), item.note.as_deref());
        assert_eq!("Copy the file's path", item.action.label);
        assert_eq!("More configuration actions", menu.label);
    }

    #[test]
    fn each_tab_is_a_route() {
        assert_eq!(Some(ConfigTab::Connection), ConfigTab::of(None));
        assert_eq!(Some(ConfigTab::SignIn), ConfigTab::of(Some("sign-in")));
        assert_eq!(Some(ConfigTab::Listener), ConfigTab::of(Some("listener")));
        assert_eq!(None, ConfigTab::of(Some("connection")));
    }

    #[test]
    fn a_duration_reads_in_its_largest_whole_unit() {
        assert_eq!("30 s", duration(Duration::from_secs(30)));
        assert_eq!("30 min", duration(Duration::from_mins(30)));
        assert_eq!("8 h", duration(Duration::from_hours(8)));
        assert_eq!("1,500 ms", duration(Duration::from_millis(1_500)));
    }
}
