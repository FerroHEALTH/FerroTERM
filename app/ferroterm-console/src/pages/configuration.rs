// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: the console's own settings as its file and environment set
//! them, read-only (FerroUI's family rule 13: a product with no
//! configuration store runs in file mode), one tab per section.
//!
//! The server keeps its own settings; the Server screen shows what it
//! declares. A secret is shown by reference only, never its value.

use std::time::Duration;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu};
use ferrohealth_ui::view::dialog::{Explanation, info_dialog};
use ferrohealth_ui::view::secret::{SecretState, secret_state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::Header;
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::settings::{
    How, Setting, SettingsPage, Source, ValueSource, settings_page,
};

use crate::locale::{Msg, tr, tr_with};
use crate::markup::{h, join, p, t};
use crate::request::Request;
use crate::routes::{CONFIGURATION_PATH, ui_path};

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
    /// The listener.
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
    fn words(self, locale: &Locale) -> String {
        tr(
            locale,
            match self {
                Self::Connection => Msg::ConfigConnection,
                Self::SignIn => Msg::ConfigSignIn,
                Self::Sessions => Msg::ConfigSessions,
                Self::Listener => Msg::ConfigListener,
            },
        )
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
/// Where a key took its value from, as the configuration loader reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeySource {
    /// The configuration file.
    File,
    /// An environment variable.
    Environment,
    /// A `--set key=value` argument.
    Set,
    /// Nothing sets it: the console's default.
    Default,
}

/// How the client secret of the sign-in is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SecretShown {
    /// Read from the file at this path.
    File(String),
    /// Set inline, never shown.
    Inline,
    /// Not set.
    Unset,
}

/// The `[oidc]` settings the page shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OidcShown {
    /// The issuer.
    issuer: String,
    /// The client id.
    client_id: String,
    /// How the client secret is set.
    secret: SecretShown,
    /// The redirect URI.
    redirect_uri: String,
    /// The scopes asked for.
    scopes: Vec<String>,
    /// Whether an end-session endpoint is set.
    end_session: bool,
}

/// What the Configuration page shows of the console's running settings,
/// with no secret in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The configuration file the settings were read from, if one was.
    file: Option<String>,
    /// The terminology server's root.
    base: String,
    /// The root readers reach the server at.
    public: String,
    /// The sync service's admin listener.
    sync: Option<String>,
    /// The server's admin listener.
    admin: Option<String>,
    /// How long one call to the server may take.
    timeout: Duration,
    /// The OpenID Provider, when one is configured.
    oidc: Option<OidcShown>,
    /// Whether the session cookie is `Secure`.
    secure_cookie: bool,
    /// The session's idle timeout.
    idle_timeout: Duration,
    /// The session's absolute lifetime.
    absolute_timeout: Duration,
    /// How many sessions the console holds at once.
    max_sessions: u64,
    /// The listener's address.
    listen: String,
    /// Where each key a row names took its value from.
    sources: Vec<(String, KeySource)>,
}

impl Shown {
    /// What the page shows of `settings`.
    #[must_use]
    pub(crate) fn of(settings: &crate::config::settings::Settings) -> Self {
        use ferrohealth_config::provenance::Source;
        let oidc = settings.oidc.as_ref().map(|oidc| OidcShown {
            issuer: oidc.issuer.clone(),
            client_id: oidc.client_id.clone(),
            secret: match (&oidc.client_secret_file, &oidc.client_secret) {
                (Some(file), _) => SecretShown::File(file.display().to_string()),
                (None, Some(_)) => SecretShown::Inline,
                (None, None) => SecretShown::Unset,
            },
            redirect_uri: oidc.redirect_uri.to_string(),
            scopes: oidc.scopes.clone(),
            end_session: oidc.end_session_endpoint.is_some(),
        });
        let sources = ROW_KEYS
            .iter()
            .map(|key| {
                let source = match settings.source_of(key) {
                    Source::File => KeySource::File,
                    Source::Environment => KeySource::Environment,
                    Source::Set => KeySource::Set,
                    Source::Default => KeySource::Default,
                };
                ((*key).to_owned(), source)
            })
            .collect();
        Self {
            file: settings
                .provenance
                .file()
                .map(|path| path.display().to_string()),
            base: settings.ferroterm.base.to_string(),
            public: settings.ferroterm.public.to_string(),
            sync: settings.ferroterm.sync.as_ref().map(ToString::to_string),
            admin: settings.ferroterm.admin.as_ref().map(ToString::to_string),
            timeout: settings.ferroterm.timeout,
            oidc,
            secure_cookie: settings.session.secure_cookie,
            idle_timeout: settings.session.idle_timeout,
            absolute_timeout: settings.session.absolute_timeout,
            max_sessions: u64::try_from(settings.session.max_sessions).unwrap_or(u64::MAX),
            listen: settings.listen.to_string(),
            sources,
        }
    }

    /// Where `key` took its value from.
    fn source_of(&self, key: &str) -> KeySource {
        self.sources
            .iter()
            .find(|(each, _)| each == key)
            .map_or(KeySource::Default, |(_, source)| *source)
    }
}

/// The first key of every row, whose source the row marks.
const ROW_KEYS: [&str; 16] = [
    "ferroterm.base_url",
    "ferroterm.public_url",
    "ferroterm.sync_url",
    "ferroterm.admin_url",
    "ferroterm.timeout_ms",
    "oidc.issuer",
    "oidc.client_id",
    "oidc.client_secret_file",
    "oidc.redirect_uri",
    "oidc.scopes",
    "oidc.end_session_endpoint",
    "session.secure_cookie",
    "session.idle_timeout_s",
    "session.absolute_timeout_s",
    "session.max_sessions",
    "server.listen",
];

fn row(locale: &Locale, label: Msg, description: Msg, value: Html, key: &str) -> Setting {
    Setting {
        label: tr(locale, label),
        description: tr(locale, description),
        value,
        keys: vec![key.to_owned()],
        next_start: false,
        source: None,
    }
}

/// A value in the code face.
fn code(text: impl Into<String>) -> Html {
    crate::markup::code(text.into())
}

/// A duration in whole seconds, minutes or hours, the largest that is whole.
fn duration(locale: &Locale, value: Duration) -> String {
    let seconds = value.as_secs();
    if seconds != 0 && seconds.is_multiple_of(3_600) {
        tr_with(
            locale,
            Msg::ConfigHours,
            &[("count", seconds.div_euclid(3_600).into())],
        )
    } else if seconds != 0 && seconds.is_multiple_of(60) {
        tr_with(
            locale,
            Msg::ConfigMinutes,
            &[("count", seconds.div_euclid(60).into())],
        )
    } else if value.subsec_millis() == 0 {
        tr_with(locale, Msg::ConfigSeconds, &[("count", seconds.into())])
    } else {
        tr_with(
            locale,
            Msg::ConfigMilliseconds,
            &[(
                "count",
                u64::try_from(value.as_millis()).unwrap_or(u64::MAX).into(),
            )],
        )
    }
}

/// On or Off, as a badge with its word.
fn on(locale: &Locale, value: bool) -> Html {
    if value {
        badge(Tone::Ok, tr(locale, Msg::ConfigOn))
    } else {
        badge(Tone::Neutral, tr(locale, Msg::ConfigOff))
    }
}

/// The settings of `tab` from `settings`.
fn settings_of(locale: &Locale, tab: ConfigTab, settings: &Shown) -> Vec<Setting> {
    let rows = match tab {
        ConfigTab::Connection => connection(locale, settings),
        ConfigTab::SignIn => sign_in(locale, settings),
        ConfigTab::Sessions => sessions(locale, settings),
        ConfigTab::Listener => listener(locale, settings),
    };
    rows.into_iter()
        .map(|row| Setting {
            source: row
                .keys
                .first()
                .and_then(|key| marker(settings.source_of(key))),
            ..row
        })
        .collect()
}

/// How a value that does not come from the file is marked: the file is the
/// page's own source, so it carries no mark.
fn marker(source: KeySource) -> Option<ValueSource> {
    match source {
        KeySource::File => None,
        // A `--set` pair wins over the file the way an environment override does.
        KeySource::Environment => Some(ValueSource::Environment),
        KeySource::Set => Some(ValueSource::CommandLine),
        KeySource::Default => Some(ValueSource::Default),
    }
}

/// The settings of the terminology server and the sync service.
fn connection(locale: &Locale, settings: &Shown) -> Vec<Setting> {
    let not_set = || {
        h(format!(
            r#"<span class="fui-muted">{}</span>"#,
            t(tr(locale, Msg::ConfigNotSet))
        ))
    };
    let sync = settings
        .sync
        .as_ref()
        .map_or_else(not_set, |url| code(url.as_str()));
    let admin = settings
        .admin
        .as_ref()
        .map_or_else(not_set, |url| code(url.as_str()));
    vec![
        row(
            locale,
            Msg::ConfigServer,
            Msg::ConfigServerLine,
            code(settings.base.as_str()),
            "ferroterm.base_url",
        ),
        row(
            locale,
            Msg::ConfigPublic,
            Msg::ConfigPublicLine,
            code(settings.public.as_str()),
            "ferroterm.public_url",
        ),
        row(
            locale,
            Msg::ConfigSync,
            Msg::ConfigSyncLine,
            sync,
            "ferroterm.sync_url",
        ),
        row(
            locale,
            Msg::ConfigAdmin,
            Msg::ConfigAdminLine,
            admin,
            "ferroterm.admin_url",
        ),
        row(
            locale,
            Msg::ConfigTimeout,
            Msg::ConfigTimeoutLine,
            t(duration(locale, settings.timeout)),
            "ferroterm.timeout_ms",
        ),
    ]
}

/// The settings of the OpenID Provider.
fn sign_in(locale: &Locale, settings: &Shown) -> Vec<Setting> {
    settings.oidc.as_ref().map_or_else(Vec::new, |oidc| {
        let secret = match &oidc.secret {
            SecretShown::File(file) => SecretState::Set(file.clone()),
            SecretShown::Inline => SecretState::Redacted,
            SecretShown::Unset => SecretState::Unset,
        };
        let sign_out = if oidc.end_session {
            tr(locale, Msg::ConfigSetUp)
        } else {
            tr(locale, Msg::ConfigNotSet)
        };
        vec![
            row(
                locale,
                Msg::ConfigIssuer,
                Msg::ConfigIssuerLine,
                code(oidc.issuer.clone()),
                "oidc.issuer",
            ),
            row(
                locale,
                Msg::ConfigClient,
                Msg::ConfigClientLine,
                code(oidc.client_id.clone()),
                "oidc.client_id",
            ),
            row(
                locale,
                Msg::ConfigSecret,
                Msg::ConfigSecretLine,
                secret_state(locale, &secret),
                "oidc.client_secret_file",
            ),
            row(
                locale,
                Msg::ConfigRedirect,
                Msg::ConfigRedirectLine,
                code(oidc.redirect_uri.clone()),
                "oidc.redirect_uri",
            ),
            row(
                locale,
                Msg::ConfigScopes,
                Msg::ConfigScopesLine,
                code(oidc.scopes.join(" ")),
                "oidc.scopes",
            ),
            row(
                locale,
                Msg::ConfigSignOut,
                Msg::ConfigSignOutLine,
                t(sign_out),
                "oidc.end_session_endpoint",
            ),
        ]
    })
}

/// The settings of the sessions.
fn sessions(locale: &Locale, settings: &Shown) -> Vec<Setting> {
    vec![
        row(
            locale,
            Msg::ConfigSecureCookie,
            Msg::ConfigSecureCookieLine,
            on(locale, settings.secure_cookie),
            "session.secure_cookie",
        ),
        row(
            locale,
            Msg::ConfigIdle,
            Msg::ConfigIdleLine,
            t(duration(locale, settings.idle_timeout)),
            "session.idle_timeout_s",
        ),
        row(
            locale,
            Msg::ConfigAbsolute,
            Msg::ConfigAbsoluteLine,
            t(duration(locale, settings.absolute_timeout)),
            "session.absolute_timeout_s",
        ),
        row(
            locale,
            Msg::ConfigLimit,
            Msg::ConfigLimitLine,
            t(ferrohealth_ui::i18n::number::grouped(
                locale,
                settings.max_sessions,
            )),
            "session.max_sessions",
        ),
    ]
}

/// The settings of the listener.
fn listener(locale: &Locale, settings: &Shown) -> Vec<Setting> {
    vec![row(
        locale,
        Msg::ConfigListen,
        Msg::ConfigListenLine,
        code(settings.listen.clone()),
        "server.listen",
    )]
}

/// The title row's actions: a More menu that copies the path of the
/// configuration file `file`, and none when the console was started with no
/// file named.
fn more(locale: &Locale, file: Option<&std::path::Path>) -> ActionGroup {
    let Some(file) = file else {
        return ActionGroup::none();
    };
    let path = file.display().to_string();
    ActionGroup::more_only(Menu::new(
        MORE_MENU,
        tr(locale, Msg::ConfigMore),
        vec![
            Item::new(Action::new(
                Some(Glyph::COPY),
                tr(locale, Msg::ConfigCopyPath),
                Act::Copy(path.clone()),
            ))
            .note(path),
        ],
    ))
}

/// Configuration, on the tab the route names, from the settings the console
/// runs on.
pub(crate) fn page(request: &Request) -> Html {
    let locale = &request.locale;
    let Some(tab) = ConfigTab::of(request.segment(1).as_deref()) else {
        return crate::pages::errors::not_found_page(request);
    };
    let Some(settings) = request
        .state
        .as_ref()
        .map(|state| Shown::of(state.settings()))
    else {
        return crate::pages::errors::not_found_page(request);
    };
    let settings = &settings;
    let shown_file = settings
        .file
        .as_deref()
        .map(std::path::Path::new)
        .and_then(|path| path.file_name())
        .map_or_else(
            || String::from(DEFAULT_FILE),
            |name| name.to_string_lossy().into_owned(),
        );
    let mut rows = settings_of(locale, tab, settings);
    let empty = tab == ConfigTab::SignIn && rows.is_empty();
    if empty {
        rows.push(Setting {
            label: tr(locale, Msg::ConfigNoSignIn),
            description: tr(locale, Msg::ConfigNoSignInLine),
            value: Html::empty(),
            keys: vec![String::from("oidc")],
            next_start: false,
            source: None,
        });
    }
    request
        .answering
        .title(&tr(locale, Msg::ShellConfiguration));
    let page = settings_page(
        locale,
        SettingsPage {
            header: Header {
                title: tr(locale, Msg::ShellConfiguration).into(),
                subtitle: Some(
                    tr_with(
                        locale,
                        Msg::ConfigSubtitle,
                        &[("file", shown_file.clone().into())],
                    )
                    .into(),
                ),
                subtitle_view: None,
                crumbs: Vec::new(),
                mono: false,
                badge: None,
            },
            actions: more(locale, settings.file.as_deref().map(std::path::Path::new)),
            source: Source::File {
                file: shown_file.clone(),
                how: How::Dialog(HOW_DIALOG.into()),
            },
            tabs: Some(TabRow {
                label: tr(locale, Msg::ConfigSections),
                tabs: ConfigTab::ALL
                    .into_iter()
                    .map(|each| Tab::new(each.key(), each.href(), each.words(locale)))
                    .collect(),
                current: tab.key().to_owned(),
            }),
            settings: rows,
            replica: None,
        },
    );
    let how = info_dialog(
        locale,
        Explanation {
            id: HOW_DIALOG.into(),
            title: tr(locale, Msg::ConfigHow).into(),
        },
        join([
            p(t(tr_with(
                locale,
                Msg::ConfigHowRead,
                &[("file", shown_file.into())],
            ))),
            p(t(tr(locale, Msg::ConfigHowChange))),
            p(t(tr(locale, Msg::ConfigHowShown))),
        ]),
    );
    Html::join([page, how])
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;
    use ferrohealth_ui::view::action_menu::Act;

    use super::{ConfigTab, duration, more};

    #[test]
    fn more_copies_the_path_of_the_file_named() {
        assert!(
            more(&Locale::english(), None).is_empty(),
            "with no file named there is no path to copy"
        );
        let group = more(
            &Locale::english(),
            Some(Path::new("/etc/ferroterm/console.toml")),
        );
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
        assert_eq!(
            "30 s",
            duration(&Locale::english(), Duration::from_secs(30))
        );
        assert_eq!(
            "30 min",
            duration(&Locale::english(), Duration::from_mins(30))
        );
        assert_eq!("8 h", duration(&Locale::english(), Duration::from_hours(8)));
        assert_eq!(
            "1,500 ms",
            duration(&Locale::english(), Duration::from_millis(1_500))
        );
    }
}
