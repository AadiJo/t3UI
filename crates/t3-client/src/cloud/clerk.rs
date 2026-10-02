//! A Clerk Frontend API (FAPI) client in "native" mode, the way `@clerk/electron` and Clerk's
//! mobile SDKs talk to Clerk (connections.md 4.3).
//!
//! Native mode has no cookies. Every request carries `_is_native=1` and, once we have one, the
//! Clerk client token as `Authorization: Bearer <jwt>`. Clerk returns the (possibly rotated)
//! client token in the `Authorization` response header of every request, including the very
//! first unauthenticated one, and we persist the latest under [`CLIENT_TOKEN_KEY`].
//!
//! Wire rules (read from clerk-js 6.25.1, `@clerk/electron` 0.0.11): bodies are
//! `application/x-www-form-urlencoded` with snake_case keys; verbs other than GET/POST go as POST
//! with `_method=<VERB>`; success bodies are `{"response": <resource>, "client": <client>}` or a
//! bare resource; errors are `{"errors":[{"code","message","long_message","meta"}],
//! "clerk_trace_id"}`.
//!
//! This module is transport only. [`super::T3Connect`] drives the sign-in state machine.

use std::{sync::Arc, time::Duration};

use parking_lot::Mutex;
use reqwest::Method;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use url::Url;

use super::net::{self, NetError};
use crate::store::{SecretStore, StoreError};

/// FAPI version clerk-js 6.25.1 pins.
pub const API_VERSION: &str = "2026-05-12";
/// Secret store key of the Clerk client token.
pub const CLIENT_TOKEN_KEY: &str = "t3-connect:clerk-client";
const TIMEOUT: Duration = Duration::from_secs(15);

/// FAPI client for one Clerk instance. Cheap to share behind an `Arc`.
pub struct ClerkClient {
    /// `https://<frontend api>/v1/`.
    api: Url,
    secrets: Arc<dyn SecretStore>,
    token: Mutex<Option<String>>,
}

impl std::fmt::Debug for ClerkClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClerkClient")
            .field("api", &self.api.as_str())
            .field("has_client", &self.token.lock().is_some())
            .finish()
    }
}

impl ClerkClient {
    /// A client for `frontend_api` (e.g. `https://clerk.t3.codes`), resuming the stored client
    /// token if there is one.
    pub fn new(frontend_api: &Url, secrets: Arc<dyn SecretStore>) -> Result<Self, StoreError> {
        let api = frontend_api
            .join("/v1/")
            .expect("a base URL accepts an absolute path");
        let token = secrets.get(CLIENT_TOKEN_KEY)?;
        Ok(ClerkClient {
            api,
            secrets,
            token: Mutex::new(token),
        })
    }

    /// Whether a Clerk client exists for this install (we hold its token).
    pub fn has_client(&self) -> bool {
        self.token.lock().is_some()
    }

    /// Drops the client token, so the next request starts a fresh anonymous Clerk client.
    pub fn forget(&self) -> Result<(), StoreError> {
        *self.token.lock() = None;
        self.secrets.delete(CLIENT_TOKEN_KEY)
    }

    /// `GET /v1/client`: the current client with its sessions. Creates an empty client (and
    /// issues a token) when we have none.
    pub async fn client(&self) -> Result<Option<ClientResource>, ClerkError> {
        let reply: Envelope<Option<ClientResource>> =
            self.request(Method::GET, "client", &[], &[]).await?;
        Ok(reply.response)
    }

    /// `POST /v1/client/sign_ins` with `params` (`identifier=..` or `strategy=oauth_..`).
    pub async fn create_sign_in(
        &self,
        params: &[(&str, &str)],
    ) -> Result<Envelope<SignInResource>, ClerkError> {
        self.request(Method::POST, "client/sign_ins", &[], params)
            .await
    }

    /// `POST /v1/client/sign_ins/{id}/prepare_first_factor`.
    pub async fn prepare_first_factor(
        &self,
        sign_in_id: &str,
        params: &[(&str, &str)],
    ) -> Result<Envelope<SignInResource>, ClerkError> {
        let path = format!(
            "client/sign_ins/{}/prepare_first_factor",
            encode(sign_in_id)
        );
        self.request(Method::POST, &path, &[], params).await
    }

    /// `POST /v1/client/sign_ins/{id}/attempt_first_factor`.
    pub async fn attempt_first_factor(
        &self,
        sign_in_id: &str,
        params: &[(&str, &str)],
    ) -> Result<Envelope<SignInResource>, ClerkError> {
        let path = format!(
            "client/sign_ins/{}/attempt_first_factor",
            encode(sign_in_id)
        );
        self.request(Method::POST, &path, &[], params).await
    }

    /// `GET /v1/client/sign_ins/{id}`, with the `rotating_token_nonce` an OAuth callback carries.
    pub async fn reload_sign_in(
        &self,
        sign_in_id: &str,
        rotating_token_nonce: Option<&str>,
    ) -> Result<Envelope<SignInResource>, ClerkError> {
        let path = format!("client/sign_ins/{}", encode(sign_in_id));
        let query: Vec<(&str, &str)> = rotating_token_nonce
            .map(|nonce| ("rotating_token_nonce", nonce))
            .into_iter()
            .collect();
        self.request(Method::GET, &path, &query, &[]).await
    }

    /// `POST /v1/client/sessions/{id}/tokens/{template}`: a short-lived session JWT for a JWT
    /// template (`t3-relay`). Always minted fresh, like upstream's `skipCache: true`.
    pub async fn session_token(
        &self,
        session_id: &str,
        template: &str,
    ) -> Result<String, ClerkError> {
        #[derive(Deserialize)]
        struct Token {
            jwt: String,
        }
        let path = format!(
            "client/sessions/{}/tokens/{}",
            encode(session_id),
            encode(template)
        );
        let reply: Envelope<Token> = self.request(Method::POST, &path, &[], &[]).await?;
        Ok(reply.response.jwt)
    }

    /// `POST /v1/client/sessions/{id}/touch`: marks the session active (clerk-js does this on
    /// focus). Returns the refreshed client when Clerk sends one.
    pub async fn touch_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ClientResource>, ClerkError> {
        let path = format!("client/sessions/{}/touch", encode(session_id));
        let reply: Envelope<Value> = self.request(Method::POST, &path, &[], &[]).await?;
        Ok(reply.client)
    }

    /// `DELETE /v1/client/sessions`: signs every session of this client out (clerk-js
    /// `signOut` with a persisted client).
    pub async fn remove_sessions(&self) -> Result<(), ClerkError> {
        self.request::<Value>(Method::DELETE, "client/sessions", &[], &[])
            .await
            .map(drop)
    }

    /// One FAPI request. Records a rotated client token before looking at the status, since
    /// Clerk rotates it on error responses too.
    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        form: &[(&str, &str)],
    ) -> Result<Envelope<T>, ClerkError> {
        let url = self.url(&method, path, query);
        let wire_method = if method == Method::GET {
            Method::GET
        } else {
            Method::POST
        };
        let mut request = net::client().request(wire_method.clone(), url);
        if let Some(token) = self.token.lock().clone() {
            request = request.bearer_auth(token);
        }
        if wire_method == Method::POST {
            let mut body = url::form_urlencoded::Serializer::new(String::new());
            body.extend_pairs(form);
            request = request
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body.finish());
        }
        let reply = net::send(request, TIMEOUT)
            .await
            .map_err(ClerkError::from)?;
        if let Some(authorization) = reply.authorization {
            self.store_token(&authorization)?;
        }
        decode_reply(reply.status, &reply.body)
    }

    fn url(&self, method: &Method, path: &str, query: &[(&str, &str)]) -> Url {
        let mut url = self.api.join(path).expect("FAPI paths are relative");
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("_is_native", "1")
                .append_pair("__clerk_api_version", API_VERSION);
            pairs.extend_pairs(query);
            if method != Method::GET && method != Method::POST {
                pairs.append_pair("_method", method.as_str());
            }
        }
        url
    }

    fn store_token(&self, authorization: &str) -> Result<(), ClerkError> {
        let token = authorization
            .strip_prefix("Bearer ")
            .unwrap_or(authorization)
            .trim();
        if token.is_empty() {
            return Ok(());
        }
        let mut current = self.token.lock();
        if current.as_deref() != Some(token) {
            self.secrets
                .set(CLIENT_TOKEN_KEY, token)
                .map_err(ClerkError::Store)?;
            *current = Some(token.to_owned());
        }
        Ok(())
    }
}

/// Percent-encodes one path segment (ids are `sia_..`/`sess_..`, but never trust that).
fn encode(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
}

/// Decodes a FAPI reply: a 2xx `{"response", "client"}` envelope (or a bare resource), or the
/// `errors` array of a failure.
fn decode_reply<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<Envelope<T>, ClerkError> {
    if !(200..300).contains(&status) {
        #[derive(Deserialize)]
        struct Errors {
            #[serde(default)]
            errors: Vec<ClerkApiError>,
            clerk_trace_id: Option<String>,
        }
        let parsed: Option<Errors> = serde_json::from_slice(body).ok();
        return Err(ClerkError::Api {
            status,
            errors: parsed
                .as_ref()
                .map(|e| e.errors.clone())
                .unwrap_or_default(),
            trace_id: parsed.and_then(|e| e.clerk_trace_id),
        });
    }
    let value: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(body).map_err(|e| ClerkError::Decode(e.to_string()))?
    };
    let (response, client) = match value {
        Value::Object(mut object) if object.contains_key("response") => (
            object.remove("response").unwrap_or(Value::Null),
            object.remove("client").unwrap_or(Value::Null),
        ),
        other => (other, Value::Null),
    };
    Ok(Envelope {
        response: serde_json::from_value(response)
            .map_err(|e| ClerkError::Decode(e.to_string()))?,
        client: serde_json::from_value(client).unwrap_or(None),
    })
}

/// A FAPI success: the resource and, for mutations, the updated client.
#[derive(Debug, Clone)]
pub struct Envelope<T> {
    pub response: T,
    pub client: Option<ClientResource>,
}

/// The Clerk client (one per install in native mode).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClientResource {
    pub id: String,
    #[serde(default)]
    pub sessions: Vec<SessionResource>,
    pub last_active_session_id: Option<String>,
}

impl ClientResource {
    /// The signed-in session: `last_active_session_id` if active, else the first active one.
    pub fn active_session(&self) -> Option<&SessionResource> {
        let active = |s: &&SessionResource| s.status == "active";
        self.last_active_session_id
            .as_deref()
            .and_then(|id| self.sessions.iter().filter(active).find(|s| s.id == id))
            .or_else(|| self.sessions.iter().find(active))
    }

    pub fn session(&self, id: &str) -> Option<&SessionResource> {
        self.sessions.iter().find(|s| s.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionResource {
    pub id: String,
    /// `active`, `pending`, `ended`, `expired`, `removed`, `abandoned`, `revoked`.
    pub status: String,
    pub user: Option<UserResource>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UserResource {
    pub id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub image_url: Option<String>,
    pub primary_email_address_id: Option<String>,
    #[serde(default)]
    pub email_addresses: Vec<EmailAddressResource>,
}

impl UserResource {
    pub fn primary_email(&self) -> Option<&str> {
        let primary = self.primary_email_address_id.as_deref();
        self.email_addresses
            .iter()
            .find(|e| Some(e.id.as_str()) == primary)
            .or_else(|| self.email_addresses.first())
            .map(|e| e.email_address.as_str())
    }

    /// "First Last", or `None` when neither is set.
    pub fn full_name(&self) -> Option<String> {
        let name = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!name.is_empty()).then_some(name)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EmailAddressResource {
    pub id: String,
    pub email_address: String,
}

/// A sign-in attempt.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SignInResource {
    pub id: String,
    /// `needs_identifier`, `needs_first_factor`, `needs_second_factor`, `needs_client_trust`,
    /// `needs_new_password`, `complete`.
    pub status: String,
    #[serde(default)]
    pub supported_first_factors: Option<Vec<FactorResource>>,
    pub first_factor_verification: Option<VerificationResource>,
    pub created_session_id: Option<String>,
    /// Present when Clerk wants a bot check only its JS SDK can run.
    pub protect_check: Option<Value>,
}

impl SignInResource {
    /// The email-code factor Clerk offers for this sign-in.
    pub fn email_code_factor(&self) -> Option<&FactorResource> {
        self.supported_first_factors
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|f| f.strategy == "email_code" && f.email_address_id.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FactorResource {
    pub strategy: String,
    pub email_address_id: Option<String>,
    /// Masked identifier for display (`j***@example.com`).
    pub safe_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VerificationResource {
    /// `unverified`, `verified`, `transferable`, `failed`, `expired`.
    pub status: Option<String>,
    pub strategy: Option<String>,
    pub external_verification_redirect_url: Option<String>,
    pub error: Option<ClerkApiError>,
}

/// One entry of a FAPI `errors` array.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClerkApiError {
    pub code: String,
    #[serde(default)]
    pub message: String,
    pub long_message: Option<String>,
}

/// Why a FAPI call failed.
#[derive(Debug, thiserror::Error)]
pub enum ClerkError {
    #[error("The T3 account service did not respond in time.")]
    Timeout,
    #[error("Could not reach the T3 account service ({0}).")]
    Network(String),
    #[error("{}", api_message(errors, *status))]
    Api {
        status: u16,
        errors: Vec<ClerkApiError>,
        trace_id: Option<String>,
    },
    #[error("Unexpected response from the T3 account service: {0}")]
    Decode(String),
    #[error("Could not save T3 Connect credentials: {0}")]
    Store(StoreError),
}

impl From<NetError> for ClerkError {
    fn from(error: NetError) -> Self {
        match error {
            NetError::Timeout => ClerkError::Timeout,
            NetError::Network(cause) => ClerkError::Network(cause),
        }
    }
}

impl ClerkError {
    /// The first error code (`form_code_incorrect`, `signed_out`, ...).
    pub fn code(&self) -> Option<&str> {
        match self {
            ClerkError::Api { errors, .. } => errors.first().map(|e| e.code.as_str()),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            ClerkError::Api { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn trace_id(&self) -> Option<&str> {
        match self {
            ClerkError::Api { trace_id, .. } => trace_id.as_deref(),
            _ => None,
        }
    }

    /// Clerk no longer recognizes our session (signed out elsewhere, expired, revoked).
    pub fn is_signed_out(&self) -> bool {
        matches!(self.status(), Some(401 | 404))
            || matches!(
                self.code(),
                Some("signed_out" | "authentication_invalid" | "resource_not_found")
            )
    }
}

fn api_message(errors: &[ClerkApiError], status: u16) -> String {
    errors
        .first()
        .map(|e| {
            e.long_message
                .clone()
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| e.message.clone())
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| format!("The T3 account service responded with HTTP {status}."))
}

// Failure modes (Clerk FAPI shapes were read from clerk-js, not a spec, so decoding must bend):
//  1. A bare resource instead of a `{"response"}` envelope (clerk-js accepts both).
//  2. `client: null` on GETs, or a missing `client` -> must not fail the response.
//  3. Error bodies without `long_message`, or not JSON at all -> still a readable message.
//  4. `supported_first_factors: null` on a completed sign-in.
//  5. Unknown fields everywhere (Clerk adds fields per API version).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_and_bare_resources_decode() {
        let wrapped = br#"{"response":{"object":"client","id":"client_1","sessions":[],"sign_in":null,"last_active_session_id":null,"extra":1},"client":null}"#;
        let reply: Envelope<Option<ClientResource>> = decode_reply(200, wrapped).unwrap();
        assert_eq!(reply.response.unwrap().id, "client_1");
        assert!(reply.client.is_none());

        #[derive(Deserialize)]
        struct Token {
            jwt: String,
        }
        let bare = br#"{"object":"token","jwt":"a.b.c"}"#;
        let reply: Envelope<Token> = decode_reply(200, bare).unwrap();
        assert_eq!(reply.response.jwt, "a.b.c");
    }

    #[test]
    fn sign_in_with_client_and_null_factors() {
        let body = br#"{"response":{"object":"sign_in_attempt","id":"sia_1","status":"complete",
            "supported_first_factors":null,"first_factor_verification":{"status":"verified","strategy":"email_code"},
            "created_session_id":"sess_1"},
          "client":{"object":"client","id":"client_1","last_active_session_id":"sess_1",
            "sessions":[{"object":"session","id":"sess_1","status":"active","user":{"id":"user_1",
              "first_name":"Ada","last_name":null,"image_url":"https://img.example/a.png",
              "primary_email_address_id":"idn_2","email_addresses":[
                {"id":"idn_1","email_address":"old@example.com"},{"id":"idn_2","email_address":"ada@example.com"}]}}]}}"#;
        let reply: Envelope<SignInResource> = decode_reply(200, body).unwrap();
        assert_eq!(reply.response.created_session_id.as_deref(), Some("sess_1"));
        assert!(reply.response.email_code_factor().is_none());
        let client = reply.client.unwrap();
        let user = client.active_session().unwrap().user.as_ref().unwrap();
        assert_eq!(user.primary_email(), Some("ada@example.com"));
        assert_eq!(user.full_name().as_deref(), Some("Ada"));
    }

    #[test]
    fn error_messages() {
        // Recorded from clerk.t3.codes (unauthenticated request to a session endpoint).
        let body = br#"{"errors":[{"message":"Signed out","long_message":"You are signed out","code":"signed_out"}],"clerk_trace_id":"8331ed98be18f2989c30d40c909d03ac"}"#;
        let error = decode_reply::<Value>(401, body).unwrap_err();
        assert_eq!(error.to_string(), "You are signed out");
        assert_eq!(error.code(), Some("signed_out"));
        assert_eq!(error.trace_id(), Some("8331ed98be18f2989c30d40c909d03ac"));
        assert!(error.is_signed_out());

        let incorrect = br#"{"errors":[{"message":"Incorrect code","code":"form_code_incorrect","meta":{"param_name":"code"}}]}"#;
        let error = decode_reply::<Value>(422, incorrect).unwrap_err();
        assert_eq!(error.to_string(), "Incorrect code");
        assert!(!error.is_signed_out());

        let html = decode_reply::<Value>(502, b"<html>bad gateway</html>").unwrap_err();
        assert_eq!(
            html.to_string(),
            "The T3 account service responded with HTTP 502."
        );
    }

    #[test]
    fn urls_carry_native_flags_and_method_override() {
        let secrets: Arc<dyn SecretStore> = Arc::new(crate::store::FileSecretStore::at(
            std::env::temp_dir().join(format!("t3ui-clerk-url-{}.json", std::process::id())),
        ));
        let clerk =
            ClerkClient::new(&Url::parse("https://clerk.t3.codes").unwrap(), secrets).unwrap();
        assert_eq!(
            clerk
                .url(
                    &Method::GET,
                    "client/sign_ins/sia_1",
                    &[("rotating_token_nonce", "n")]
                )
                .as_str(),
            "https://clerk.t3.codes/v1/client/sign_ins/sia_1?_is_native=1&__clerk_api_version=2026-05-12&rotating_token_nonce=n"
        );
        assert_eq!(
            clerk.url(&Method::DELETE, "client/sessions", &[]).as_str(),
            "https://clerk.t3.codes/v1/client/sessions?_is_native=1&__clerk_api_version=2026-05-12&_method=DELETE"
        );
    }
}
