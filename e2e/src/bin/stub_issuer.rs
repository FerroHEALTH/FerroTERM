// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The identity provider the signed-in browser journeys sign in to.
//!
//! It is an OpenID Connect issuer reduced to what a SMART standalone launch of
//! a public client needs (<https://hl7.org/fhir/smart-app-launch/app-launch.html>):
//! a discovery document, an authorization endpoint that redirects straight
//! back with a code, a token endpoint that checks the PKCE verifier against the
//! challenge (RFC 7636 §4.6), a key set, and a revocation endpoint (RFC 7009).
//! Nothing here is a product binary, and nothing here is a security boundary:
//! it exists so the journeys can drive a completed sign-in.
//!
//! It serves TLS, because FerroTERM refuses a non-loopback issuer over plain
//! HTTP, or plain HTTP with `--plain` for a run on loopback. The certificate
//! authority is generated at start and written where the harness can install
//! it, so no key material is committed.
//!
//! The console signs a reader in on its own server (authorization code with
//! PKCE), so the identity token carries the `nonce` the authorization request
//! sent and names the client it was asked for as its audience (OpenID Connect
//! Core 1.0 §3.1.3.7).
//!
//! Which scopes a sign-in ends up with is chosen by the journey: it opens
//! `/profile?name=<profile>&tag=<tag>` on this issuer first, which sets a
//! cookie this issuer reads on the authorization request. The tag keeps two
//! journeys running at once from reading each other's revocations.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use base64::Engine;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::Method;
use hyper::Request;
use hyper::Response;
use hyper::StatusCode;
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use ring::rand::SecureRandom;
use ring::rand::SystemRandom;
use ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING;
use ring::signature::EcdsaKeyPair;
use ring::signature::KeyPair;
use rustls::pki_types::pem::PemObject;
use serde_json::Value;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// Anything that stops the issuer starting.
type Failure = Box<dyn std::error::Error + Send + Sync>;

/// The base64url alphabet without padding, which JOSE fixes (RFC 7515 §2).
const URL_SAFE: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// How long a minted access token is good for.
const LIFETIME_SECONDS: i64 = 300;

/// How long the generated certificates are valid for, in days.
///
/// A leaf a browser accepts is short-lived, and this one outlives a run by
/// enough that a clock skew between the host and a container cannot expire it.
const VALID_DAYS: i64 = 30;

/// The `kid` the key set and every token header carry (RFC 7517 §4.5).
const KEY_ID: &str = "stub-issuer-k1";

/// The audience an access token names when the authorization request names
/// none, the one `scripts/ui-e2e.sh` configures the server with.
const AUDIENCE: &str = "ferroterm";

/// The scopes this issuer knows how to grant.
///
/// The three resource scopes are the ones FerroTERM's write gate honours, in
/// the `user` compartment a person signs in under
/// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
const GRANTABLE: [&str; 5] = [
    "openid",
    "fhirUser",
    "user/CodeSystem.cud",
    "user/ValueSet.cud",
    "user/ConceptMap.cud",
];

/// The scopes a sign-in that identifies the person and changes nothing gets.
const IDENTITY_ONLY: [&str; 2] = ["openid", "fhirUser"];

/// The `fhirUser` claim the identity token carries.
///
/// It is the URL of the resource describing the person
/// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
/// and the viewer's shell shows it as the name of whoever is signed in.
const FHIR_USER: &str = "Practitioner/e2e-terminologist";

/// The name the cookie carries the journey's choice under.
const PROFILE_COOKIE: &str = "ferroterm_e2e_profile";

/// What a sign-in through this issuer ends up granting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    /// Every scope the client asked for that this issuer grants.
    Writer,
    /// The identity scopes alone, which opens no write.
    Reader,
    /// The full grant in the token response, and the identity scopes alone in
    /// the access token.
    ///
    /// This is the issuer that states a grant in its answer and leaves it out
    /// of the credential, so the viewer draws a control the server then
    /// refuses. RFC 6749 §3.3 has the response state the granted scope; the
    /// server reads the token's own `scope` claim.
    ResponseOnly,
}

impl Profile {
    /// The profile `name` selects; an unknown name is the writer.
    fn of(name: &str) -> Self {
        match name {
            "reader" => Self::Reader,
            "response-only" => Self::ResponseOnly,
            _writer => Self::Writer,
        }
    }

    /// The scopes the token response states, out of those `asked` for.
    fn answered(self, asked: &[String]) -> Vec<String> {
        match self {
            Self::Writer | Self::ResponseOnly => keep(asked, &GRANTABLE),
            Self::Reader => keep(asked, &IDENTITY_ONLY),
        }
    }

    /// The scopes the access token itself carries, out of those `asked` for.
    fn in_token(self, asked: &[String]) -> Vec<String> {
        match self {
            Self::Writer => keep(asked, &GRANTABLE),
            Self::Reader | Self::ResponseOnly => keep(asked, &IDENTITY_ONLY),
        }
    }
}

/// The members of `asked` that `allowed` admits, in the order asked.
fn keep(asked: &[String], allowed: &[&str]) -> Vec<String> {
    asked
        .iter()
        .filter(|scope| allowed.iter().any(|known| known == scope))
        .cloned()
        .collect()
}

/// What the authorization request left for the token request to check.
#[derive(Clone, Debug)]
struct Pending {
    /// The `code_challenge` the client sent (RFC 7636 §4.3).
    challenge: String,
    /// The scopes the client asked for.
    asked: Vec<String>,
    /// The `aud` the client sent, which is the FHIR base it will spend the
    /// token against.
    audience: String,
    /// The `client_id` the client named, the identity token's audience.
    client: String,
    /// The `nonce` the identity token must carry back.
    nonce: String,
    /// What the journey chose to be granted.
    profile: Profile,
    /// The journey's own tag, so its revocations are its own.
    tag: String,
}

/// The signing key of this run, and the JWK that publishes it.
#[derive(Debug)]
struct Keys {
    /// The PKCS#8 private key tokens are signed with.
    pkcs8: Vec<u8>,
    /// The public point, `0x04 || X || Y` (RFC 5480 §2.2).
    public: Vec<u8>,
}

impl Keys {
    /// Generates the key this run signs with.
    fn generate(random: &SystemRandom) -> Result<Self, Failure> {
        // `ring`'s failures are deliberately opaque and carry no `Error`
        // implementation, so each one becomes the sentence it stands for
        // (<https://docs.rs/ring/0.17/ring/error/struct.Unspecified.html>).
        let document = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, random)
            .map_err(|_opaque| Failure::from("no signing key could be generated"))?;
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, document.as_ref(), random)
                .map_err(|_opaque| Failure::from("the generated signing key does not read back"))?;
        Ok(Self {
            pkcs8: document.as_ref().to_vec(),
            public: pair.public_key().as_ref().to_vec(),
        })
    }

    /// The key set this issuer publishes (RFC 7517 §4, RFC 7518 §6.2.1).
    fn jwks(&self) -> Result<Value, Failure> {
        let x = self.public.get(1..33).ok_or("the point carries no X")?;
        let y = self.public.get(33..65).ok_or("the point carries no Y")?;
        Ok(json!({"keys": [{
            "kty": "EC",
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig",
            "kid": KEY_ID,
            "x": URL_SAFE.encode(x),
            "y": URL_SAFE.encode(y),
        }]}))
    }

    /// `claims` signed as an ES256 token of type `typ` naming this key.
    ///
    /// ES256 rather than RS256: this issuer generates its key at start, `ring`
    /// generates no RSA key, and the crate that does carries RUSTSEC-2023-0071.
    /// FerroTERM accepts every asymmetric algorithm RFC 7518 defines.
    fn sign(&self, claims: &Value, typ: &str) -> Result<String, Failure> {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(KEY_ID.to_owned());
        header.typ = Some(typ.to_owned());
        let key = jsonwebtoken::EncodingKey::from_ec_der(&self.pkcs8);
        Ok(jsonwebtoken::encode(&header, claims, &key)?)
    }
}

/// The addresses this issuer publishes and the state it keeps.
#[derive(Debug)]
struct Stub {
    /// The issuer identifier, which is also where the discovery document and
    /// the key set are read from.
    issuer: String,
    /// The address the browser reaches the endpoints at, which is a different
    /// name from the server's whenever the two resolve this host differently.
    public: String,
    /// The signing key.
    keys: Keys,
    /// The random source codes are drawn from.
    random: SystemRandom,
    /// The codes handed out and not yet spent.
    pending: Mutex<HashMap<String, Pending>>,
    /// Which journey a minted token belongs to, by token.
    minted: Mutex<HashMap<String, String>>,
    /// How many tokens each journey has revoked, by tag.
    revoked: Mutex<HashMap<String, usize>>,
}

impl Stub {
    /// The OpenID Connect discovery document (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §3).
    ///
    /// `token_endpoint_auth_methods_supported` names `none` and
    /// `grant_types_supported` names the authorization code grant, which is
    /// what makes FerroTERM declare `client-public` and `launch-standalone`
    /// and the viewer offer a sign-in
    /// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    fn discovery(&self) -> Value {
        let issuer = &self.issuer;
        let public = &self.public;
        json!({
            "issuer": issuer,
            "jwks_uri": format!("{issuer}/jwks"),
            "authorization_endpoint": format!("{public}/authorize"),
            "token_endpoint": format!("{public}/token"),
            "revocation_endpoint": format!("{public}/revoke"),
            "grant_types_supported": ["authorization_code"],
            "scopes_supported": GRANTABLE,
            "response_types_supported": ["code"],
            "token_endpoint_auth_methods_supported": ["none"],
            "code_challenge_methods_supported": ["S256"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["ES256"],
        })
    }

    /// A random identifier, in the base64url alphabet a URL carries verbatim.
    fn identifier(&self) -> Result<String, Failure> {
        let mut bytes = [0_u8; 24];
        self.random
            .fill(&mut bytes)
            .map_err(|_opaque| Failure::from("no random bytes could be drawn"))?;
        Ok(URL_SAFE.encode(bytes))
    }

    /// Answers one request.
    fn answer(&self, method: &Method, path: &str, query: &str, cookie: &str, body: &[u8]) -> Reply {
        if method == Method::OPTIONS {
            return Reply::preflight();
        }
        match (method, path) {
            (&Method::GET, "/.well-known/openid-configuration") => {
                Reply::json(StatusCode::OK, &self.discovery())
            }
            (&Method::GET, "/jwks") => match self.keys.jwks() {
                Ok(document) => Reply::json(StatusCode::OK, &document),
                Err(error) => Reply::text(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
            },
            (&Method::GET, "/profile") => Self::profile(query),
            (&Method::GET, "/authorize") => self.authorize(query, cookie),
            (&Method::POST, "/token") => self.token(body),
            (&Method::POST, "/revoke") => self.revoke(body),
            (&Method::GET, "/revocations") => self.revocations(query),
            _unknown => Reply::text(StatusCode::NOT_FOUND, "no such endpoint"),
        }
    }

    /// `GET /profile?name=…&tag=…`: remembers what the journey signs in as.
    fn profile(query: &str) -> Reply {
        let asked = parameters(query);
        let name = asked.get("name").cloned().unwrap_or_default();
        let tag = asked.get("tag").cloned().unwrap_or_default();
        let mut reply = Reply::html(
            StatusCode::OK,
            &format!("<p id=\"profile\">{name} {tag}</p>"),
        );
        // SameSite=Lax is sent on the top-level navigation the authorization
        // request is, which is the one request that reads it
        // (<https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie>).
        reply.cookie = Some(format!(
            "{PROFILE_COOKIE}={name}:{tag}; Path=/; SameSite=Lax; Secure"
        ));
        reply
    }

    /// `GET /authorize`: hands out a code and sends the browser back.
    ///
    /// Nothing is shown and nobody is asked: the journey is about what the
    /// viewer does with what comes back (RFC 6749 §4.1.2).
    fn authorize(&self, query: &str, cookie: &str) -> Reply {
        let asked = parameters(query);
        let Some(redirect_uri) = asked.get("redirect_uri") else {
            return Reply::text(StatusCode::BAD_REQUEST, "the request names no redirect_uri");
        };
        let (profile, tag) = chosen(cookie);
        let pending = Pending {
            challenge: asked.get("code_challenge").cloned().unwrap_or_default(),
            asked: asked
                .get("scope")
                .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
            audience: asked.get("aud").cloned().unwrap_or_default(),
            client: asked.get("client_id").cloned().unwrap_or_default(),
            nonce: asked.get("nonce").cloned().unwrap_or_default(),
            profile,
            tag,
        };
        let code = match self.identifier() {
            Ok(code) => code,
            Err(error) => {
                return Reply::text(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
            }
        };
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(code.clone(), pending);
        let state = asked.get("state").cloned().unwrap_or_default();
        let back = format!(
            "{redirect_uri}?code={}&state={}",
            encoded(&code),
            encoded(&state)
        );
        Reply::redirect(&back)
    }

    /// `POST /token`: checks the verifier and mints the tokens.
    fn token(&self, body: &[u8]) -> Reply {
        let sent = parameters(&String::from_utf8_lossy(body));
        let Some(code) = sent.get("code") else {
            return Reply::refusal("invalid_request", "the request carries no code");
        };
        let Some(pending) = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(code)
        else {
            return Reply::refusal("invalid_grant", "that code was never handed out");
        };
        let verifier = sent.get("code_verifier").cloned().unwrap_or_default();
        if challenge_of(&verifier) != pending.challenge {
            // RFC 7636 §4.6: the verifier the client sends must hash to the
            // challenge the authorization request carried.
            return Reply::refusal("invalid_grant", "the code verifier does not match");
        }
        match self.minted_for(&pending) {
            Ok(answer) => Reply::json(StatusCode::OK, &answer),
            Err(error) => Reply::text(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        }
    }

    /// The token response for a checked authorization (RFC 6749 §5.1).
    fn minted_for(&self, pending: &Pending) -> Result<Value, Failure> {
        let now = jiff::Timestamp::now().as_second();
        let expires = now + LIFETIME_SECONDS;
        // RFC 9068 §2: an access token is typed `at+jwt` and carries `client_id`
        // and `jti` beside the claims every JWT carries.
        let audience = if pending.audience.is_empty() {
            AUDIENCE
        } else {
            pending.audience.as_str()
        };
        let access = self.keys.sign(
            &json!({
                "iss": self.issuer,
                "sub": FHIR_USER,
                "aud": audience,
                "client_id": pending.client,
                "jti": format!("{}-{now}", pending.tag),
                "iat": now,
                "nbf": now,
                "exp": expires,
                "scope": pending.profile.in_token(&pending.asked).join(" "),
            }),
            "at+jwt",
        )?;
        let mut identity_claims = json!({
            "iss": self.issuer,
            "sub": FHIR_USER,
            "aud": pending.client,
            "iat": now,
            "exp": expires,
            "fhirUser": FHIR_USER,
            "preferred_username": FHIR_USER,
        });
        if !pending.nonce.is_empty()
            && let Some(claims) = identity_claims.as_object_mut()
        {
            claims.insert(String::from("nonce"), Value::from(pending.nonce.clone()));
        }
        let identity = self.keys.sign(&identity_claims, "JWT")?;
        self.minted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(access.clone(), pending.tag.clone());
        Ok(json!({
            "access_token": access,
            "token_type": "Bearer",
            "expires_in": LIFETIME_SECONDS,
            "scope": pending.profile.answered(&pending.asked).join(" "),
            "id_token": identity,
        }))
    }

    /// `POST /revoke`: records the revocation (RFC 7009 §2.1).
    ///
    /// The answer is `200` whether or not the token is known, which RFC 7009
    /// §2.2 requires.
    fn revoke(&self, body: &[u8]) -> Reply {
        let sent = parameters(&String::from_utf8_lossy(body));
        if let Some(token) = sent.get("token")
            && let Some(tag) = self
                .minted
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(token)
        {
            let mut revoked = self.revoked.lock().unwrap_or_else(PoisonError::into_inner);
            let count = revoked.entry(tag).or_insert(0);
            *count += 1;
        }
        Reply::text(StatusCode::OK, "")
    }

    /// `GET /revocations?tag=…`: what one journey has revoked so far.
    ///
    /// The journey opens this in the browser after signing out, which is how a
    /// revocation the viewer sent from a page is observed.
    fn revocations(&self, query: &str) -> Reply {
        let tag = parameters(query).get("tag").cloned().unwrap_or_default();
        let count = self
            .revoked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&tag)
            .copied()
            .unwrap_or_default();
        Reply::html(
            StatusCode::OK,
            &format!("<p id=\"revocations\">{tag} revoked {count}</p>"),
        )
    }
}

/// The profile and the tag a `Cookie` header names.
fn chosen(cookie: &str) -> (Profile, String) {
    let value = cookie
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == PROFILE_COOKIE)
        .map(|(_, value)| value)
        .unwrap_or_default();
    let (name, tag) = value.split_once(':').unwrap_or((value, ""));
    (Profile::of(name), tag.to_owned())
}

/// The `S256` challenge of `verifier` (RFC 7636 §4.2).
fn challenge_of(verifier: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes());
    URL_SAFE.encode(digest.as_ref())
}

/// The parameters of a query string or a form body.
fn parameters(text: &str) -> HashMap<String, String> {
    form_urlencoded::parse(text.as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}

/// `text` percent-encoded down to the unreserved set (RFC 3986 §2.3).
fn encoded(text: &str) -> String {
    form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// One answer, before it becomes an HTTP response.
#[derive(Debug)]
struct Reply {
    /// The status to answer.
    status: StatusCode,
    /// The media type of the body.
    content_type: &'static str,
    /// The body.
    body: String,
    /// Where the browser is sent, on a redirect.
    location: Option<String>,
    /// The cookie to set, when one is.
    cookie: Option<String>,
}

impl Reply {
    /// A reply with a body and nothing else.
    fn of(status: StatusCode, content_type: &'static str, body: &str) -> Self {
        Self {
            status,
            content_type,
            body: body.to_owned(),
            location: None,
            cookie: None,
        }
    }

    /// A JSON document.
    fn json(status: StatusCode, document: &Value) -> Self {
        Self::of(status, "application/json", &document.to_string())
    }

    /// A plain-text answer.
    fn text(status: StatusCode, body: &str) -> Self {
        Self::of(status, "text/plain; charset=utf-8", body)
    }

    /// A page the browser renders, for a journey to read.
    ///
    /// The document declares its own icon, because a browser that finds none
    /// probes `/favicon.ico` by itself and the 404 lands in the console the
    /// journeys read (<https://html.spec.whatwg.org/multipage/links.html#rel-icon>).
    fn html(status: StatusCode, body: &str) -> Self {
        Self::of(
            status,
            "text/html; charset=utf-8",
            &format!(
                "<!doctype html><html lang=\"en\"><title>stub issuer</title>\
                 <link rel=\"icon\" href=\"data:,\">{body}</html>"
            ),
        )
    }

    /// The OAuth error answer of a refused token request (RFC 6749 §5.2).
    fn refusal(error: &str, description: &str) -> Self {
        Self::json(
            StatusCode::BAD_REQUEST,
            &json!({"error": error, "error_description": description}),
        )
    }

    /// Sends the browser to `address` (RFC 6749 §4.1.2).
    fn redirect(address: &str) -> Self {
        let mut reply = Self::text(StatusCode::FOUND, "");
        reply.location = Some(address.to_owned());
        reply
    }

    /// The answer to a preflight, which a browser sends before a cross-origin
    /// `POST` of a form body with a content type it did not choose.
    ///
    /// SMART requires an issuer serving browser clients to support CORS on the
    /// endpoints one reaches
    /// (<https://hl7.org/fhir/smart-app-launch/conformance.html>), and the
    /// viewer is such a client: it exchanges its code from the page.
    fn preflight() -> Self {
        Self::text(StatusCode::NO_CONTENT, "")
    }

    /// The reply as the response hyper writes.
    fn respond(self) -> Response<Full<Bytes>> {
        let mut builder = Response::builder()
            .status(self.status)
            .header(hyper::header::CONTENT_TYPE, self.content_type)
            // Every answer here is a public document or a response to a
            // request that carries no cookie, so the origin is not narrowed.
            .header(hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(
                hyper::header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, OPTIONS",
            )
            .header(
                hyper::header::ACCESS_CONTROL_ALLOW_HEADERS,
                "Authorization, Content-Type",
            );
        if let Some(location) = self.location {
            builder = builder.header(hyper::header::LOCATION, location);
        }
        if let Some(cookie) = self.cookie {
            builder = builder.header(hyper::header::SET_COOKIE, cookie);
        }
        builder
            .body(Full::new(Bytes::from(self.body)))
            .unwrap_or_else(|_refused| {
                let mut fallback = Response::new(Full::new(Bytes::new()));
                *fallback.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                fallback
            })
    }
}

/// What the harness starts this issuer with.
#[derive(Debug)]
struct Options {
    /// The address to bind.
    listen: SocketAddr,
    /// The issuer identifier, which FerroTERM is configured with.
    issuer: String,
    /// The address the browser reaches the endpoints at.
    public: String,
    /// Where the certificate authority is written, for both to trust.
    ca: PathBuf,
    /// The names the issuer's own certificate is issued for.
    names: Vec<String>,
    /// The name a second certificate is issued for, when the harness asks for
    /// one: the TLS terminator in front of the viewer, which the sign-in needs
    /// because a browser gives `crypto.subtle` to a secure context alone
    /// (<https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts>).
    serve_name: Option<String>,
    /// Where that second certificate and its key are written.
    serve_dir: Option<PathBuf>,
    /// Whether the issuer serves plain HTTP, for a run on loopback.
    plain: bool,
}

impl Options {
    /// Reads the options from the command line.
    fn read(arguments: impl Iterator<Item = String>) -> Result<Self, Failure> {
        let mut listen = String::from("127.0.0.1:8443");
        let mut issuer = String::new();
        let mut public = String::new();
        let mut ca = PathBuf::from("ca.pem");
        let mut names = Vec::new();
        let mut serve_name = None;
        let mut serve_dir = None;
        let mut plain = false;
        let mut arguments = arguments;
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .ok_or_else(|| Failure::from(format!("{argument} takes a value")))
            };
            if argument == "--plain" {
                plain = true;
                continue;
            }
            match argument.as_str() {
                "--listen" => listen = value()?,
                "--issuer" => issuer = value()?,
                "--public" => public = value()?,
                "--ca" => ca = PathBuf::from(value()?),
                "--name" => names.push(value()?),
                "--serve-name" => serve_name = Some(value()?),
                "--serve-dir" => serve_dir = Some(PathBuf::from(value()?)),
                other => return Err(Failure::from(format!("unknown argument `{other}`"))),
            }
        }
        if issuer.is_empty() {
            return Err(Failure::from("--issuer names the issuer identifier"));
        }
        if public.is_empty() {
            public.clone_from(&issuer);
        }
        if names.is_empty() {
            names.push(String::from("localhost"));
        }
        if serve_name.is_some() != serve_dir.is_some() {
            return Err(Failure::from("--serve-name and --serve-dir go together"));
        }
        Ok(Self {
            listen: listen.parse()?,
            issuer: issuer.trim_end_matches('/').to_owned(),
            public: public.trim_end_matches('/').to_owned(),
            ca,
            names,
            serve_name,
            serve_dir,
            plain,
        })
    }
}

/// One certificate and the key that goes with it, as PEM.
#[derive(Debug)]
struct Issued {
    /// The leaf and the authority, in that order, which is the chain a server
    /// sends (RFC 8446 §4.4.2).
    chain: String,
    /// The private key, PKCS#8.
    key: String,
}

/// The authority of this run, which signs every certificate it issues.
///
/// One run, one authority: the harness installs it in the server container and
/// in the browser once, and the issuer and the TLS terminator in front of the
/// viewer are both issued from it.
#[derive(Debug)]
struct Authority {
    /// The authority's own certificate.
    certificate: rcgen::Certificate,
    /// The parameters it was issued under, which signing a leaf needs again.
    parameters: rcgen::CertificateParams,
    /// Its key.
    key: rcgen::KeyPair,
    /// When the certificates it issues become valid.
    start: time::OffsetDateTime,
}

impl Authority {
    /// Generates the authority of this run.
    fn generate() -> Result<Self, Failure> {
        let start = time::OffsetDateTime::from_unix_timestamp(jiff::Timestamp::now().as_second())?;
        let key = rcgen::KeyPair::generate()?;
        let mut parameters = rcgen::CertificateParams::new(Vec::new())?;
        parameters.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
        parameters.not_before = start - time::Duration::days(1);
        parameters.not_after = start + time::Duration::days(VALID_DAYS);
        parameters
            .distinguished_name
            .push(rcgen::DnType::CommonName, "FerroTERM E2E authority");
        parameters.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        let certificate = parameters.self_signed(&key)?;
        Ok(Self {
            certificate,
            parameters,
            key,
            start,
        })
    }

    /// The authority certificate, PEM, which is what both sides install.
    fn pem(&self) -> String {
        self.certificate.pem()
    }

    /// A server certificate for `names`, signed by this authority.
    fn issue(&self, names: &[String], common_name: &str) -> Result<Issued, Failure> {
        let key = rcgen::KeyPair::generate()?;
        let mut parameters = rcgen::CertificateParams::new(names.to_vec())?;
        parameters.is_ca = rcgen::IsCa::NoCa;
        parameters.not_before = self.start - time::Duration::days(1);
        parameters.not_after = self.start + time::Duration::days(VALID_DAYS);
        parameters.use_authority_key_identifier_extension = true;
        parameters.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        parameters
            .distinguished_name
            .push(rcgen::DnType::CommonName, common_name);
        let issuer = rcgen::Issuer::from_params(&self.parameters, &self.key);
        let certificate = parameters.signed_by(&key, &issuer)?;
        Ok(Issued {
            chain: format!("{}{}", certificate.pem(), self.certificate.pem()),
            key: key.serialize_pem(),
        })
    }
}

/// The TLS configuration the issuer itself serves on.
fn tls(issued: &Issued) -> Result<rustls::ServerConfig, Failure> {
    let chain: Result<Vec<rustls::pki_types::CertificateDer<'static>>, _> =
        rustls::pki_types::CertificateDer::pem_slice_iter(issued.chain.as_bytes()).collect();
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(issued.key.as_bytes())?;
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    Ok(rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain?, key)?)
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let options = Options::read(std::env::args().skip(1))?;
    let authority = Authority::generate()?;
    std::fs::write(&options.ca, authority.pem())?;
    if let (Some(name), Some(directory)) = (&options.serve_name, &options.serve_dir) {
        let issued = authority.issue(std::slice::from_ref(name), name)?;
        std::fs::create_dir_all(directory)?;
        std::fs::write(directory.join("cert.pem"), issued.chain)?;
        std::fs::write(directory.join("key.pem"), issued.key)?;
    }
    let server_config = tls(&authority.issue(&options.names, "FerroTERM E2E stub issuer")?)?;
    let random = SystemRandom::new();
    let issuer = Arc::new(Stub {
        issuer: options.issuer.clone(),
        public: options.public.clone(),
        keys: Keys::generate(&random)?,
        random,
        pending: Mutex::new(HashMap::new()),
        minted: Mutex::new(HashMap::new()),
        revoked: Mutex::new(HashMap::new()),
    });
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind(options.listen).await?;
    // The harness waits for this line before it starts anything that talks to
    // the issuer.
    println!("listening on {} as {}", options.listen, options.issuer);
    if options.plain {
        return serve_plain(listener, issuer).await;
    }
    loop {
        let (stream, _peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let issuer = Arc::clone(&issuer);
        tokio::spawn(async move {
            let accepted = match acceptor.accept(stream).await {
                Ok(accepted) => accepted,
                Err(error) => {
                    eprintln!("the handshake failed: {error}");
                    return;
                }
            };
            let service = service_fn(move |request: Request<Incoming>| {
                let issuer = Arc::clone(&issuer);
                async move {
                    Ok::<_, std::convert::Infallible>(handle(&issuer, request).await.respond())
                }
            });
            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(accepted), service)
                .await
            {
                eprintln!("the connection ended: {error}");
            }
        });
    }
}

/// Serves plain HTTP on `listener`, for a run on loopback.
async fn serve_plain(listener: TcpListener, issuer: Arc<Stub>) -> Result<(), Failure> {
    loop {
        let (stream, _peer) = listener.accept().await?;
        let issuer = Arc::clone(&issuer);
        tokio::spawn(async move {
            let service = service_fn(move |request: Request<Incoming>| {
                let issuer = Arc::clone(&issuer);
                async move {
                    Ok::<_, std::convert::Infallible>(handle(&issuer, request).await.respond())
                }
            });
            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                eprintln!("the connection ended: {error}");
            }
        });
    }
}

/// Reads one request and answers it.
async fn handle(issuer: &Stub, request: Request<Incoming>) -> Reply {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or_default().to_owned();
    let cookie = request
        .headers()
        .get(hyper::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            return Reply::text(StatusCode::BAD_REQUEST, &format!("the body ended: {error}"));
        }
    };
    issuer.answer(&method, &path, &query, &cookie, &body)
}

#[cfg(test)]
mod tests {
    use super::Profile;
    use super::challenge_of;
    use super::chosen;
    use super::keep;

    /// The verifier and challenge of RFC 7636 appendix B.
    #[test]
    fn the_challenge_is_the_one_rfc_7636_publishes() {
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_profile_cookie_names_the_grant_and_the_journey() {
        assert_eq!(
            chosen("ferroterm_e2e_profile=reader:abc"),
            (Profile::Reader, String::from("abc"))
        );
        assert_eq!(
            chosen("other=1; ferroterm_e2e_profile=response-only:xyz"),
            (Profile::ResponseOnly, String::from("xyz"))
        );
        assert_eq!(
            chosen(""),
            (Profile::Writer, String::new()),
            "no cookie is the full grant"
        );
    }

    #[test]
    fn a_reader_is_granted_the_identity_scopes_alone() {
        let asked: Vec<String> = ["openid", "fhirUser", "user/ValueSet.cud"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            Profile::Reader.answered(&asked),
            vec![String::from("openid"), String::from("fhirUser")]
        );
        assert_eq!(Profile::Writer.answered(&asked), asked);
        assert_eq!(
            Profile::ResponseOnly.answered(&asked),
            asked,
            "the response states the whole grant"
        );
        assert_eq!(
            Profile::ResponseOnly.in_token(&asked),
            vec![String::from("openid"), String::from("fhirUser")],
            "and the token carries none of the writes"
        );
    }

    #[test]
    fn a_scope_this_issuer_does_not_grant_is_dropped() {
        let asked = vec![String::from("system/CodeSystem.cud")];
        assert!(keep(&asked, &["openid"]).is_empty());
    }
}
