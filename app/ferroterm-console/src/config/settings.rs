// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checked settings the console runs on, resolved from the written
//! [`Config`] with every `_file` secret read.

use std::net::SocketAddr;
use std::time::Duration;

use ferrohealth_config::loader::Loaded;
use ferrohealth_config::provenance::{Provenance, Source};
use ferrohealth_config::removed::Removals;
use ferrohealth_session::config::{OidcSection, OidcSettings, SessionSection, SessionSettings};
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::credentials::CredentialsSection;
use ferrohealth_smart::config::endpoint::EndpointSection;
use ferrohealth_telemetry::section::TelemetrySection;
use ferrohealth_ui::i18n::Locale;
use url::Url;

use crate::config::error::Error;
use crate::config::{Config, RETIRED};

/// The resolved configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The socket address the console listens on.
    pub listen: SocketAddr,
    /// The locale a page renders in when the request names none the
    /// console's catalog holds.
    pub locale: Locale,
    /// The terminology server the console is a client of.
    pub ferroterm: FerrotermSettings,
    /// `[session]` as written, which the Configuration page shows.
    pub session: SessionSection,
    /// `[oidc]` as written, which the Configuration page shows; none when
    /// the console offers no sign-in.
    pub oidc: Option<OidcSection>,
    /// The checked sign-in the session store and the relying party run on.
    pub sign_in: SignInSettings,
    /// Where each key took its value from, and the configuration file the
    /// settings were read from, which the Configuration page names.
    pub provenance: Provenance,
    /// The console log.
    pub telemetry: TelemetrySection,
}

impl Settings {
    /// Resolves the loaded configuration `loaded`, keeping where each key
    /// came from.
    ///
    /// # Errors
    /// Returns every error of [`Config::resolve`].
    pub fn from_loaded(loaded: Loaded<Config>) -> Result<Self, Error> {
        let (config, provenance) = loaded.into_parts();
        let mut settings = config.resolve()?;
        settings.provenance = provenance;
        Ok(settings)
    }

    /// Where `key` took its value from. A secret set through its `_file`
    /// key counts as set, either way round
    /// (`ferrohealth_config::provenance::Provenance::secret_source`).
    #[must_use]
    pub fn source_of(&self, key: &str) -> Source {
        self.provenance.secret_source(key)
    }
}

/// The resolved `[ferroterm]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FerrotermSettings {
    /// The root of the server's HTTP surface, an `http` or `https` URL whose
    /// path ends in `/`, so a relative route joins beneath it.
    pub base: Url,
    /// The root the readers reach the server at, which the console shows.
    pub public: Url,
    /// The root of the sync service's admin listener, when one is configured.
    pub sync: Option<Url>,
    /// The root of the terminology server's admin listener, when one is
    /// configured.
    pub admin: Option<Url>,
    /// How long one call to the server may take.
    pub timeout: Duration,
    /// `[ferroterm.server]`, which the server's outbound client is built
    /// from.
    pub server_endpoint: Box<EndpointSection>,
    /// `[ferroterm.sync]`, when one is configured.
    pub sync_endpoint: Option<Box<EndpointSection>>,
    /// `[ferroterm.admin]`, when one is configured.
    pub admin_endpoint: Option<Box<EndpointSection>>,
}

impl FerrotermSettings {
    /// The public root as the console shows it, without its final `/`.
    #[must_use]
    pub fn shown_root(&self) -> String {
        self.public.as_str().trim_end_matches('/').to_owned()
    }
}

/// The checked `[session]` and `[oidc]`.
#[derive(Debug, Clone)]
pub struct SignInSettings {
    /// The session store's settings.
    pub store: SessionSettings,
    /// The OpenID Provider's, when one is configured.
    pub provider: Option<OidcSettings>,
    /// The console's own origin, `scheme://host:port`: the origin of its
    /// redirection endpoint, which an unsafe request must come from; none
    /// without a provider.
    pub origin: Option<String>,
}

impl Config {
    /// Checks every value and reads every `_file` secret.
    ///
    /// # Errors
    /// Returns [`Error::Removed`] for a key the console no longer reads,
    /// [`Error::Address`] for a listen address that does not parse,
    /// [`Error::Locale`] for a `server.locale` that is no language tag,
    /// [`Error::Missing`], [`Error::Url`] and [`Error::UrlShape`] for a URL
    /// that is absent, does not parse or is not one its key admits,
    /// [`Error::Endpoint`] for an endpoint table whose timeout or retry
    /// breaks a rule, [`Error::Credentials`] for one with a credential of its
    /// own, [`Error::SignIn`] for an `[oidc]`
    /// or a `[session]` `ferrohealth-session` refuses, the secret errors of
    /// `oidc.client_secret` among them, and [`Error::InsecureCookie`] for
    /// `session.secure_cookie = false` on a console that is not `http` on
    /// loopback.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let mut removals = Removals::new();
        let set = [
            self.ferroterm.base_url,
            self.ferroterm.timeout,
            self.ferroterm.sync_url,
            self.ferroterm.admin_url,
        ];
        for (removed, retired) in set.into_iter().zip(&RETIRED) {
            removals.check(removed, retired);
        }
        removals.finish()?;
        let listen = self
            .server
            .listen
            .parse()
            .map_err(|source| Error::Address {
                key: String::from("server.listen"),
                source,
            })?;
        let locale = Locale::parse(&self.server.locale).map_err(|source| Error::Locale {
            key: String::from("server.locale"),
            source,
        })?;
        let base = endpoint_url("ferroterm.server", &self.ferroterm.server)?;
        let public = self
            .ferroterm
            .public_url
            .as_deref()
            .map(|text| base_url("ferroterm.public_url", text))
            .transpose()?
            .unwrap_or_else(|| base.clone());
        let sync = self
            .ferroterm
            .sync
            .as_ref()
            .map(|endpoint| endpoint_url("ferroterm.sync", endpoint))
            .transpose()?;
        let admin = self
            .ferroterm
            .admin
            .as_ref()
            .map(|endpoint| endpoint_url("ferroterm.admin", endpoint))
            .transpose()?;
        let timeout = self.ferroterm.server.timeout.get();
        let refused = |error| Error::SignIn(Box::new(error));
        let provider = self
            .oidc
            .as_ref()
            .map(|oidc| oidc.check_with_tls(&ServerTls).map_err(refused))
            .transpose()?;
        let store = match &provider {
            Some(provider) => self.session.check(provider),
            None => self.session.check_with(false),
        }
        .map_err(refused)?;
        // NOTE: `ferrohealth_session::config::OidcSection::check_with_tls` parsed the
        // redirect URI, so a URL that does not parse here names no origin.
        let redirect = self
            .oidc
            .as_ref()
            .and_then(|oidc| Url::parse(&oidc.redirect_uri).ok());
        // NOTE: RFC 6265bis §4.1.2.5: a cookie without `Secure` travels over plain
        // HTTP, so it is admitted only where the console itself is HTTP on loopback.
        if !store.secure_cookie()
            && redirect
                .as_ref()
                .is_some_and(|redirect| !(redirect.scheme() == "http" && is_loopback(redirect)))
        {
            return Err(Error::InsecureCookie {
                key: String::from("session.secure_cookie"),
            });
        }
        let origin = redirect.map(|redirect| redirect.origin().ascii_serialization());
        Ok(Settings {
            listen,
            locale,
            ferroterm: FerrotermSettings {
                base,
                public,
                sync,
                admin,
                timeout,
                server_endpoint: self.ferroterm.server.clone(),
                sync_endpoint: self.ferroterm.sync.clone(),
                admin_endpoint: self.ferroterm.admin.clone(),
            },
            session: self.session.clone(),
            oidc: self.oidc.clone(),
            sign_in: SignInSettings {
                store,
                provider,
                origin,
            },
            provenance: Provenance::default(),
            telemetry: self.telemetry.clone(),
        })
    }
}

/// Parses an `http` or `https` URL with a host and no userinfo.
fn web_url(key: &str, text: &str) -> Result<Url, Error> {
    if text.trim().is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    let url = Url::parse(text).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    let shape = |reason| Error::UrlShape {
        key: key.to_owned(),
        reason,
    };
    if !matches!(url.scheme(), "http" | "https") {
        return Err(shape("must be an http or https URL"));
    }
    if url.host().is_none() {
        return Err(shape("must name a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(shape("must not carry userinfo"));
    }
    Ok(url)
}

/// Parses an `http` or `https` URL with a host, no userinfo, no query and no
/// fragment, and ends its path in `/`.
fn base_url(key: &str, text: &str) -> Result<Url, Error> {
    let mut url = web_url(key, text)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::UrlShape {
            key: key.to_owned(),
            reason: "must not carry a query or a fragment",
        });
    }
    // NOTE: RFC 3986 §5.2.3: a reference merges with the base path up to its
    // last `/`, so the base ends in one or its last segment would be dropped.
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// The checked root of the endpoint table at `key`: its URL as
/// [`base_url`] admits it, its timeouts and retry as
/// `EndpointSection::validate` admits them, and no credential of its own,
/// since every call carries the signed-in reader's.
fn endpoint_url(key: &str, endpoint: &EndpointSection) -> Result<Url, Error> {
    let url = base_url(&format!("{key}.url"), endpoint.url.expose())?;
    endpoint.validate().map_err(|source| Error::Endpoint {
        key: key.to_owned(),
        source: Box::new(source),
    })?;
    if endpoint.credentials != CredentialsSection::default() {
        return Err(Error::Credentials {
            key: format!("{key}.credentials"),
        });
    }
    Ok(url)
}

/// Whether `url` names a loopback host: `localhost`, `127.0.0.0/8` or `::1`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::Duration;

    use ferrohealth_config::loader::Loader;
    use ferrohealth_config::provenance::Source;
    use ferrohealth_secret::file::SecretError;
    use ferrohealth_secret::secret::Secret;
    use ferrohealth_session::config::{ConfigError, OidcSection};

    use super::Settings;
    use crate::config::error::Error;
    use crate::config::{Config, PREFIX};

    /// A secret file holding `text` in a fresh directory.
    fn secret_file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("client_secret");
        std::fs::write(&path, text).expect("the file is written");
        (dir, path)
    }

    /// A console signed in at a provider whose every endpoint `[oidc]`
    /// names, with the client secret `inline` and the secret file `file`.
    fn oidc(inline: Option<&str>, file: Option<&Path>) -> Config {
        Config {
            oidc: Some(OidcSection {
                issuer: String::from("https://idp.example"),
                authorization_endpoint: Some(String::from("https://idp.example/authorize")),
                token_endpoint: Some(String::from("https://idp.example/token")),
                jwks_uri: Some(String::from("https://idp.example/jwks")),
                client_id: String::from("ferroterm-console"),
                client_secret: inline.map(Secret::new),
                client_secret_file: file.map(Path::to_path_buf),
                redirect_uri: String::from("https://console.example/auth/callback"),
                ..OidcSection::default()
            }),
            ..Config::default()
        }
    }

    /// The secret error `config` is refused with.
    fn refused(config: &Config) -> SecretError {
        match config.resolve() {
            Err(Error::SignIn(error)) => match *error {
                ConfigError::Secret(error) => error,
                other => panic!("a secret refusal, got {other:?}"),
            },
            other => panic!("a secret refusal, got {other:?}"),
        }
    }

    #[test]
    fn the_settings_name_the_file_they_were_read_from() {
        let path = std::env::temp_dir().join(format!(
            "ferroterm-console-{}-named.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[ferroterm.server]\nurl = \"http://ferroterm:8080/\"\n",
        )
        .expect("the file is written");
        let settings = Config::load(Some(&path), &[])
            .and_then(Settings::from_loaded)
            .expect("the file reads");
        std::fs::remove_file(&path).expect("the file is removed");
        assert_eq!(Some(path.as_path()), settings.provenance.file());
        let none = Config::from_sources(None, &BTreeMap::new())
            .and_then(|config| config.resolve())
            .expect("the defaults resolve");
        assert_eq!(None, none.provenance.file(), "no file was read");
    }

    #[test]
    fn the_configuration_knows_where_each_key_came_from() {
        let environment = BTreeMap::from([(
            String::from("FERROTERM_CONSOLE__SESSION__SECURE_COOKIE"),
            String::from("false"),
        )]);
        let loaded = Loader::new(PREFIX)
            .expect("the prefix")
            .load_sources::<Config>(
                Some("[ferroterm.server]\nurl = \"http://ferroterm:8080/\"\n\n[oidc]\nclient_secret_file = \"/run/secrets/x\"\n"),
                None,
                &environment,
            )
            .expect("the sources read");
        let (config, provenance) = loaded.into_parts();
        let mut settings = Config {
            oidc: None,
            ..config
        }
        .resolve()
        .expect("resolves");
        settings.provenance = provenance;
        assert_eq!(Source::File, settings.source_of("ferroterm.server.url"));
        assert_eq!(
            Source::Environment,
            settings.source_of("session.secure_cookie")
        );
        assert_eq!(
            Source::Default,
            settings.source_of("ferroterm.server.timeout")
        );
        assert_eq!(Source::File, settings.source_of("oidc.client_secret_file"));
        assert_eq!(
            Source::File,
            settings.source_of("oidc.client_secret"),
            "a secret set through its file counts as set"
        );
    }

    #[test]
    fn a_retired_key_is_an_unknown_key_and_a_zero_timeout_is_refused() {
        let refused =
            Config::from_sources(Some("[ferroterm]\ntimeout_ms = 30000\n"), &BTreeMap::new())
                .expect_err("the strict load refuses a key the console does not read");
        assert!(matches!(refused, Error::Load(_)), "{refused:?}");
        assert!(
            Config::from_sources(
                Some("[ferroterm.server]\nurl = \"http://ferroterm:8080/\"\ntimeout = \"0s\"\n"),
                &BTreeMap::new()
            )
            .expect("reads")
            .resolve()
            .is_err_and(|error| matches!(error, Error::Endpoint { .. })),
            "a zero timeout is refused"
        );
    }

    #[test]
    fn a_key_0_4_5_retired_is_refused_naming_its_replacement() {
        let config = Config::from_sources(
            Some("[ferroterm]\nbase_url = \"http://ferroterm:8080/\"\ntimeout = \"30s\"\nsync_url = \"http://s/\"\nadmin_url = \"http://a/\"\n"),
            &BTreeMap::new(),
        )
        .expect("the strict load still reads a retired key");
        let Err(Error::Removed(refused)) = config.resolve() else {
            panic!("the retired keys are refused");
        };
        let refused = refused.to_string();
        for (old, new) in [
            ("ferroterm.base_url", "ferroterm.server.url"),
            ("ferroterm.timeout", "ferroterm.server.timeout"),
            ("ferroterm.sync_url", "ferroterm.sync.url"),
            ("ferroterm.admin_url", "ferroterm.admin.url"),
        ] {
            assert!(
                refused.contains(&format!("{old} was replaced by {new}")),
                "{old} names {new}, all at once: {refused}"
            );
        }
    }

    #[test]
    fn every_retired_key_belongs_to_the_release_being_prepared() {
        assert!(
            ferrohealth_config::removed::stale(&crate::config::RETIRED, crate::config::PREPARING)
                .is_empty(),
            "a refusal lives for one release"
        );
    }

    #[test]
    fn a_server_table_written_with_only_its_url_keeps_the_console_timeout() {
        let config = Config::from_sources(
            Some("[ferroterm.server]\nurl = \"http://ferroterm:8080/\"\n"),
            &BTreeMap::new(),
        )
        .expect("reads");
        assert_eq!(
            Duration::from_secs(30),
            Duration::from(config.ferroterm.server.timeout),
            "the console's 30 s timeout holds under a table that names only its url"
        );
        assert_eq!(
            "http://ferroterm:8080/",
            config.ferroterm.server.url.expose(),
            "the url the file writes wins over the default"
        );
    }

    #[test]
    fn an_endpoint_table_with_its_own_credential_is_refused() {
        let config = Config::from_sources(
            Some("[ferroterm.server]\nurl = \"http://ferroterm:8080/\"\n\n[ferroterm.server.credentials]\nuser = \"u\"\npassword = \"p\"\n"),
            &BTreeMap::new(),
        )
        .expect("reads");
        assert!(
            matches!(config.resolve(), Err(Error::Credentials { key }) if key == "ferroterm.server.credentials"),
            "every call carries the reader's credential"
        );
    }

    #[test]
    fn an_override_name_without_a_known_key_is_refused() {
        let environment =
            BTreeMap::from([(String::from("FERROTERM_CONSOLE__LISTEN"), String::from("x"))]);
        let error = Config::from_sources(None, &environment)
            .expect_err("a key the tree does not have is refused");
        assert!(matches!(error, Error::Load(_)), "{error:?}");
    }

    #[test]
    fn a_name_under_the_prefix_that_is_no_override_is_left_alone() {
        let other = BTreeMap::from([(String::from("FERROTERM_CONSOLE_LISTEN"), String::from("x"))]);
        Config::from_sources(None, &other).expect("only an override is checked against the tree");
    }

    #[test]
    fn an_override_into_a_value_that_is_no_table_is_refused() {
        let environment = BTreeMap::from([(
            String::from("FERROTERM_CONSOLE__SERVER__LISTEN__PORT"),
            String::from("x"),
        )]);
        let error = Config::from_sources(
            Some("[server]\nlisten = \"127.0.0.1:3000\"\n"),
            &environment,
        )
        .expect_err("a key under a value is refused");
        assert!(matches!(error, Error::Load(_)), "{error:?}");
    }

    #[test]
    fn an_environment_value_reads_as_the_key_wants() {
        let environment = BTreeMap::from([
            (
                String::from("FERROTERM_CONSOLE__FERROTERM__SERVER__TIMEOUT"),
                String::from("5s"),
            ),
            (
                String::from("FERROTERM_CONSOLE__SESSION__SECURE_COOKIE"),
                String::from("false"),
            ),
            (
                String::from("FERROTERM_CONSOLE__SERVER__LISTEN"),
                String::from("0.0.0.0:3000"),
            ),
        ]);
        let config = Config::from_sources(None, &environment).expect("reads");
        assert_eq!(
            Duration::from_secs(5),
            config.ferroterm.server.timeout.get()
        );
        assert!(!config.session.secure_cookie);
        assert_eq!("0.0.0.0:3000", config.server.listen);
    }

    #[test]
    fn an_inline_secret_or_a_secret_file_or_none_at_all_resolves() {
        let (_dir, path) = secret_file("file-secret\n");
        for config in [
            oidc(Some("inline-secret"), None),
            oidc(None, Some(&path)),
            oidc(None, None),
        ] {
            config.resolve().expect("the secret resolves");
        }
    }

    #[test]
    fn a_secret_set_both_inline_and_in_a_file_is_refused_by_its_key() {
        let (_dir, path) = secret_file("file-secret");
        let error = refused(&oidc(Some("inline-secret"), Some(&path)));
        assert!(
            matches!(&error, SecretError::Both { key } if key == "oidc.client_secret"),
            "{error:?}"
        );
    }

    #[test]
    fn a_secret_file_that_cannot_be_read_is_refused_by_its_key() {
        let missing = Path::new("/nonexistent/ferroterm-console/client_secret");
        let error = refused(&oidc(None, Some(missing)));
        assert!(
            matches!(&error, SecretError::Read { key, path, .. }
                if key == "oidc.client_secret" && path == missing),
            "{error:?}"
        );
    }

    #[test]
    fn an_empty_secret_file_is_refused() {
        let (_dir, path) = secret_file("\n");
        let error = refused(&oidc(None, Some(&path)));
        assert!(
            matches!(&error, SecretError::Empty { key, .. } if key == "oidc.client_secret"),
            "{error:?}"
        );
    }

    #[test]
    fn a_refusal_never_shows_the_secret() {
        let (_dir, path) = secret_file("file-secret");
        let error = oidc(Some("inline-secret"), Some(&path))
            .resolve()
            .expect_err("both are refused");
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("inline-secret"), "{shown}");
        assert!(!shown.contains("file-secret"), "{shown}");
    }

    #[test]
    fn an_oidc_section_a_console_cannot_sign_in_with_safely_is_refused_by_key() {
        let refused = |change: fn(&mut OidcSection)| {
            let mut config = oidc(None, None);
            if let Some(section) = config.oidc.as_mut() {
                change(section);
            }
            match config.resolve() {
                Err(Error::SignIn(error)) => *error,
                other => panic!("an [oidc] refusal, got {other:?}"),
            }
        };
        let plain =
            refused(|oidc| oidc.token_endpoint = Some(String::from("http://idp.example/t")));
        assert!(
            matches!(&plain, ConfigError::UrlShape { key, .. } if *key == "oidc.token_endpoint"),
            "{plain:?}"
        );
        let offline = refused(|oidc| oidc.scopes.push(String::from("offline_access")));
        assert!(
            matches!(&offline, ConfigError::OfflineAccessScope),
            "{offline:?}"
        );
        let no_openid = refused(|oidc| oidc.scopes = vec![String::from("profile")]);
        assert!(matches!(&no_openid, ConfigError::NoOpenId), "{no_openid:?}");
        let fragment = refused(|oidc| oidc.redirect_uri.push_str("#x"));
        assert!(
            matches!(&fragment, ConfigError::UrlShape { key, .. } if *key == "oidc.redirect_uri"),
            "{fragment:?}"
        );
    }

    #[test]
    fn cookies_without_secure_are_refused_off_loopback_and_kept_on_it() {
        let mut config = oidc(None, None);
        config.session.secure_cookie = false;
        let error = config.resolve().expect_err("an https console");
        assert!(
            matches!(&error, Error::InsecureCookie { key } if key == "session.secure_cookie"),
            "{error:?}"
        );
        if let Some(oidc) = config.oidc.as_mut() {
            oidc.redirect_uri = String::from("http://127.0.0.1:3000/auth/callback");
        }
        let settings = config.resolve().expect("a console on loopback");
        assert_eq!(
            Some("http://127.0.0.1:3000"),
            settings.sign_in.origin.as_deref()
        );
    }

    #[test]
    fn the_locale_is_english_unless_configured_and_a_bad_tag_is_refused_by_key() {
        let settings = Config::from_sources(None, &BTreeMap::new())
            .expect("the defaults read")
            .resolve()
            .expect("the defaults resolve");
        assert_eq!("en", settings.locale.tag());
        let configured =
            Config::from_sources(Some("[server]\nlocale = \"en-GB\"\n"), &BTreeMap::new())
                .expect("the tree reads")
                .resolve()
                .expect("a language tag resolves");
        assert_eq!("en-GB", configured.locale.tag());
        let refused =
            Config::from_sources(Some("[server]\nlocale = \"not a tag\"\n"), &BTreeMap::new())
                .expect("the tree reads")
                .resolve()
                .expect_err("no language tag");
        assert!(
            matches!(&refused, Error::Locale { key, .. } if key == "server.locale"),
            "{refused:?}"
        );
    }
}
