//! [`T3Connect`]: the signed-in T3 account, its linked environments, and endpoints for them.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use t3_protocol::EnvironmentId;
use tokio::sync::watch;
use url::Url;

use super::{
    CloudConfig,
    clerk::{ClerkClient, ClerkError, ClientResource, Envelope, SignInResource},
    dpop::{self, DpopKey},
    endpoint::{Bootstrap, BootstrapSource, DpopEndpoint},
    oauth::{self, Callback, OAuthProvider, WebAuthError, WebAuthenticator},
    relay::{NETWORK_BLOCKING_HINT, RelayClient, RelayEnvironment},
};
use crate::{
    auth::{BoxFuture, ClientInfo, Endpoint},
    connection::{BlockedReason, ConnectionFailure, TransientReason},
    store::{
        EnvironmentCatalog, KnownTarget, SavedEnvironment, SavedTarget, SecretStore, StoreError,
    },
};

/// Secret store key of the signed-in [`Account`] (JSON).
pub const ACCOUNT_KEY: &str = "t3-connect:account";

/// The signed-in T3 account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Clerk user id (`user_...`); relay environments are saved under it.
    pub user_id: String,
    pub session_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub image_url: Option<String>,
}

impl Account {
    /// Name, else email, else the user id.
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .or(self.email.as_deref())
            .unwrap_or(&self.user_id)
    }
}

/// Everything the UI shows about T3 Connect. Published on [`T3Connect::state`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloudState {
    /// `None`: signed out.
    pub account: Option<Account>,
    pub discovery: Discovery,
}

/// The linked-environment list (upstream `RelayEnvironmentDiscoveryState`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Discovery {
    /// A refresh is running. The list is cleared when one starts.
    pub refreshing: bool,
    /// Why the last refresh failed (listing, not per-environment status).
    pub error: Option<ConnectionFailure>,
    pub environments: Vec<DiscoveredEnvironment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredEnvironment {
    pub environment: RelayEnvironment,
    pub availability: Availability,
}

/// What the relay says about a linked environment.
#[derive(Debug, Clone, PartialEq)]
pub enum Availability {
    /// Status request in flight.
    Checking,
    Online,
    /// The relay could not reach it; `reason` like "Managed endpoint health request timed out."
    Offline {
        reason: Option<String>,
    },
    /// The status request itself failed.
    Error(ConnectionFailure),
}

/// A user-facing failure of a T3 Connect action (sign-in, sign-out, listing).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CloudError {
    pub message: String,
    pub trace_id: Option<String>,
    /// The user cancelled (closed the browser sheet). Not worth an error toast.
    pub cancelled: bool,
}

impl CloudError {
    fn new(message: impl Into<String>) -> Self {
        CloudError {
            message: message.into(),
            trace_id: None,
            cancelled: false,
        }
    }
}

impl From<ClerkError> for CloudError {
    fn from(error: ClerkError) -> Self {
        let message = match error.code() {
            Some("form_identifier_not_found") => {
                "No T3 account uses this email. Create one at accounts.t3.codes, then sign in here."
                    .to_owned()
            }
            _ => error.to_string(),
        };
        CloudError {
            message,
            trace_id: error.trace_id().map(str::to_owned),
            cancelled: false,
        }
    }
}

impl From<StoreError> for CloudError {
    fn from(error: StoreError) -> Self {
        CloudError::new(format!("Could not save T3 Connect credentials: {error}"))
    }
}

impl From<WebAuthError> for CloudError {
    fn from(error: WebAuthError) -> Self {
        CloudError {
            cancelled: error == WebAuthError::Cancelled,
            ..CloudError::new(error.to_string())
        }
    }
}

/// Handle to T3 Connect. Cheap to clone. Its async methods run on the networking runtime, so
/// any executor (GPUI's) can await them.
///
/// ```ignore
/// let connect = T3Connect::new(CloudConfig::production(), secrets)?;
/// let pending = connect.start_email_sign_in("me@example.com").await?;   // emails a code
/// let account = pending.verify("123456").await?;                        // signed in
/// connect.refresh();                                                     // fills state().discovery
/// let saved = connect.saved_environment(&record)?;                       // catalog entry
/// let env = Environment::start(EnvironmentOptions::from_saved(&saved, connect.endpoint(saved.environment_id.clone())));
/// ```
#[derive(Clone)]
pub struct T3Connect {
    inner: Arc<Inner>,
}

struct Inner {
    config: CloudConfig,
    secrets: Arc<dyn SecretStore>,
    key: Arc<DpopKey>,
    clerk: ClerkClient,
    relay: RelayClient,
    client: ClientInfo,
    account: Mutex<Option<Account>>,
    state: watch::Sender<CloudState>,
    /// Bumped by every refresh and sign-in change; stale refreshes drop their results.
    generation: AtomicU64,
}

impl T3Connect {
    /// Loads the install's DPoP key (creating and storing one on first use) and the stored
    /// session, if any. The stored account shows as signed in right away; [`restore`]
    /// (Self::restore) checks it with Clerk. Small secret-store reads only.
    pub fn new(config: CloudConfig, secrets: Arc<dyn SecretStore>) -> Result<Self, StoreError> {
        let key = match secrets
            .get(dpop::SECRET_KEY)?
            .map(|s| DpopKey::from_secret(&s))
        {
            Some(Ok(key)) => key,
            stored => {
                if stored.is_some() {
                    tracing::warn!("replacing an unreadable T3 Connect key");
                }
                let key = DpopKey::generate();
                secrets.set(dpop::SECRET_KEY, &key.secret())?;
                key
            }
        };
        let key = Arc::new(key);
        let account: Option<Account> = secrets
            .get(ACCOUNT_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok());
        let clerk = ClerkClient::new(&config.clerk_frontend_api, secrets.clone())?;
        let relay = RelayClient::new(&config.relay_url, config.relay_client_id, key.clone());
        let state = watch::channel(CloudState {
            account: account.clone(),
            discovery: Discovery::default(),
        })
        .0;
        Ok(T3Connect {
            inner: Arc::new(Inner {
                config,
                secrets,
                key,
                clerk,
                relay,
                client: ClientInfo::default(),
                account: Mutex::new(account),
                state,
                generation: AtomicU64::new(0),
            }),
        })
    }

    pub fn config(&self) -> &CloudConfig {
        &self.inner.config
    }

    /// Account and discovery state. `borrow()` for now, `changed().await` for updates.
    pub fn state(&self) -> watch::Receiver<CloudState> {
        self.inner.state.subscribe()
    }

    pub fn account(&self) -> Option<Account> {
        self.inner.account.lock().clone()
    }

    /// The install's DPoP key thumbprint (diagnostics).
    pub fn key_thumbprint(&self) -> &str {
        self.inner.key.thumbprint()
    }

    /// Checks the stored session with Clerk (call once at launch). Signs out locally if Clerk
    /// no longer knows it; keeps it on network errors. Refreshes discovery when signed in.
    pub async fn restore(&self) -> Result<(), CloudError> {
        let inner = self.inner.clone();
        crate::runtime::spawn(async move {
            let Some(account) = inner.account.lock().clone() else {
                return Ok(());
            };
            match inner.clerk.client().await {
                Ok(client) => {
                    let session = client
                        .as_ref()
                        .and_then(|c| c.session(&account.session_id))
                        .filter(|s| s.status == "active");
                    let Some(session) = session else {
                        inner.signed_out();
                        return Ok(());
                    };
                    if let Some(updated) = account_from(session.id.as_str(), client.as_ref())
                        && updated != account
                    {
                        inner.set_account(Some(updated))?;
                    }
                    if let Err(error) = inner.clerk.touch_session(&account.session_id).await {
                        tracing::debug!(%error, "touching the T3 Connect session failed");
                    }
                }
                Err(error) if error.is_signed_out() => {
                    inner.signed_out();
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            }
            Inner::refresh(&inner).await;
            Ok(())
        })
        .await
    }

    /// Starts an email-code sign-in: Clerk emails a 6-digit code to `email`.
    pub async fn start_email_sign_in(&self, email: &str) -> Result<EmailSignIn, CloudError> {
        let email = email.trim().to_owned();
        if email.is_empty() {
            return Err(CloudError::new("Enter your email address."));
        }
        let connect = self.clone();
        crate::runtime::spawn(async move {
            let inner = &connect.inner;
            let created = inner
                .clerk
                .create_sign_in(&[("identifier", email.as_str())])
                .await?;
            let sign_in = created.response;
            unsupported_check(&sign_in)?;
            let factor = sign_in.email_code_factor().cloned().ok_or_else(|| {
                CloudError::new(
                    "This account cannot sign in with an email code. Use one of the providers instead.",
                )
            })?;
            let email_address_id = factor.email_address_id.clone().unwrap_or_default();
            inner
                .clerk
                .prepare_first_factor(
                    &sign_in.id,
                    &[
                        ("strategy", "email_code"),
                        ("email_address_id", email_address_id.as_str()),
                    ],
                )
                .await?;
            Ok(EmailSignIn {
                connect: connect.clone(),
                sign_in_id: sign_in.id,
                email_address_id,
                masked_email: factor.safe_identifier.unwrap_or_else(|| email.clone()),
                email,
            })
        })
        .await
    }

    /// Signs in with a social provider through `authenticator` (a browser sheet).
    pub async fn sign_in_with(
        &self,
        provider: OAuthProvider,
        authenticator: Arc<dyn WebAuthenticator>,
    ) -> Result<Account, CloudError> {
        let inner = self.inner.clone();
        crate::runtime::spawn(async move {
            let redirect = inner.config.oauth_redirect_url.to_string();
            let created = inner
                .clerk
                .create_sign_in(&[
                    ("strategy", provider.strategy()),
                    ("redirect_url", redirect.as_str()),
                    ("action_complete_redirect_url", redirect.as_str()),
                ])
                .await?;
            let sign_in = created.response;
            unsupported_check(&sign_in)?;
            let verification = sign_in.first_factor_verification.clone();
            let url = verification
                .as_ref()
                .filter(|v| v.status.as_deref() == Some("unverified"))
                .and_then(|v| v.external_verification_redirect_url.as_deref())
                .and_then(|url| Url::parse(url).ok())
                .ok_or_else(|| CloudError::new(format!("{} sign-in is not available.", provider.label())))?;
            let callback = authenticator
                .authenticate(url, inner.config.oauth_redirect_url.scheme().to_owned())
                .await?;
            let nonce = match oauth::read_callback(&callback) {
                Callback::Failed { message } => return Err(CloudError::new(message)),
                Callback::Continue {
                    rotating_token_nonce,
                } => rotating_token_nonce,
            };
            let reloaded = inner.clerk.reload_sign_in(&sign_in.id, nonce.as_deref()).await?;
            let transferable = reloaded
                .response
                .first_factor_verification
                .as_ref()
                .and_then(|v| v.status.as_deref())
                == Some("transferable");
            if transferable {
                return Err(CloudError::new(format!(
                    "No T3 account uses this {} login. Create one at accounts.t3.codes, then sign in here.",
                    provider.label()
                )));
            }
            Inner::finish_sign_in(&inner, reloaded).await
        })
        .await
    }

    /// Signs out: ends the Clerk session (best effort), forgets the client, and clears the
    /// relay token cache. The caller removes relay environments from its catalog
    /// ([`remove_relay_environments`]) and drops their [`Environment`](crate::Environment)s.
    pub async fn sign_out(&self) -> Result<(), CloudError> {
        let inner = self.inner.clone();
        crate::runtime::spawn(async move {
            if inner.clerk.has_client()
                && let Err(error) = inner.clerk.remove_sessions().await
            {
                tracing::warn!(%error, "ending the T3 Connect session failed; forgetting it locally");
            }
            inner.clerk.forget()?;
            inner.relay.reset().await;
            inner.signed_out();
            Ok(())
        })
        .await
    }

    /// Reloads the linked-environment list and each one's relay status into
    /// [`state`](Self::state). Signed out, it settles to an empty list.
    pub fn refresh(&self) {
        let inner = self.inner.clone();
        crate::runtime::runtime().spawn(async move { Inner::refresh(&inner).await });
    }

    /// [`refresh`](Self::refresh), resolving when every status check finished.
    pub async fn refresh_now(&self) {
        let inner = self.inner.clone();
        crate::runtime::spawn(async move { Inner::refresh(&inner).await }).await
    }

    /// The endpoint for a T3 Connect environment: relay credential, DPoP token, ticket.
    pub fn endpoint(&self, environment_id: EnvironmentId) -> Arc<dyn Endpoint> {
        Arc::new(DpopEndpoint::new(
            environment_id,
            self.inner.key.clone(),
            Arc::new(RelaySource(self.inner.clone())),
            self.inner.client.clone(),
        ))
    }

    /// The catalog entry for connecting `environment` (the "Connect" button). Fails when
    /// signed out.
    pub fn saved_environment(
        &self,
        environment: &RelayEnvironment,
    ) -> Result<SavedEnvironment, CloudError> {
        let account = self
            .account()
            .ok_or_else(|| CloudError::new("Sign in to T3 Connect to connect this environment."))?;
        Ok(SavedEnvironment::new(
            environment.environment_id.clone(),
            environment.label.clone(),
            SavedTarget::Known(KnownTarget::Relay {
                account_id: account.user_id,
            }),
        ))
    }

    /// The endpoint for a saved relay environment that belongs to the signed-in account; `None`
    /// for other targets, other accounts, or when signed out.
    pub fn saved_endpoint(&self, saved: &SavedEnvironment) -> Option<Arc<dyn Endpoint>> {
        let SavedTarget::Known(KnownTarget::Relay { account_id }) = &saved.target else {
            return None;
        };
        let account = self.account()?;
        (account.user_id == *account_id).then(|| self.endpoint(saved.environment_id.clone()))
    }
}

/// Removes every T3 Connect entry from `catalog` (sign-out, account change; connections.md
/// 1.5). Returns the removed ids so the caller can drop their environments.
pub fn remove_relay_environments(catalog: &mut EnvironmentCatalog) -> Vec<EnvironmentId> {
    let (relay, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut catalog.environments)
        .into_iter()
        .partition(|e| matches!(e.target, SavedTarget::Known(KnownTarget::Relay { .. })));
    catalog.environments = kept;
    relay.into_iter().map(|e| e.environment_id).collect()
}

/// An email-code sign-in waiting for its code. Keep it in the dialog state.
#[derive(Clone)]
pub struct EmailSignIn {
    connect: T3Connect,
    sign_in_id: String,
    email_address_id: String,
    email: String,
    masked_email: String,
}

impl std::fmt::Debug for EmailSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailSignIn")
            .field("sign_in_id", &self.sign_in_id)
            .field("masked_email", &self.masked_email)
            .finish_non_exhaustive()
    }
}

impl EmailSignIn {
    /// The address as typed.
    pub fn email(&self) -> &str {
        &self.email
    }

    /// The address as Clerk masks it (`j***@example.com`).
    pub fn masked_email(&self) -> &str {
        &self.masked_email
    }

    /// Submits the emailed code. Wrong or expired codes fail with Clerk's message and can be
    /// retried.
    pub async fn verify(&self, code: &str) -> Result<Account, CloudError> {
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        if code.is_empty() {
            return Err(CloudError::new("Enter the code from the email."));
        }
        let inner = self.connect.inner.clone();
        let sign_in_id = self.sign_in_id.clone();
        crate::runtime::spawn(async move {
            let attempted = inner
                .clerk
                .attempt_first_factor(
                    &sign_in_id,
                    &[("strategy", "email_code"), ("code", code.as_str())],
                )
                .await?;
            Inner::finish_sign_in(&inner, attempted).await
        })
        .await
    }

    /// Emails a new code.
    pub async fn resend(&self) -> Result<(), CloudError> {
        let inner = self.connect.inner.clone();
        let (sign_in_id, email_address_id) =
            (self.sign_in_id.clone(), self.email_address_id.clone());
        crate::runtime::spawn(async move {
            inner
                .clerk
                .prepare_first_factor(
                    &sign_in_id,
                    &[
                        ("strategy", "email_code"),
                        ("email_address_id", email_address_id.as_str()),
                    ],
                )
                .await?;
            Ok(())
        })
        .await
    }
}

/// Sign-in states this client cannot finish (connections.md 4.3).
fn unsupported_check(sign_in: &SignInResource) -> Result<(), CloudError> {
    if sign_in.protect_check.as_ref().is_some_and(|v| !v.is_null()) {
        return Err(CloudError::new(
            "T3's sign-in service asked for a browser check this app cannot run. Try again later.",
        ));
    }
    Ok(())
}

/// The account for `session_id` from a Clerk client, if that session has a user.
fn account_from(session_id: &str, client: Option<&ClientResource>) -> Option<Account> {
    let session = client?.session(session_id)?;
    let user = session.user.as_ref()?;
    Some(Account {
        user_id: user.id.clone(),
        session_id: session.id.clone(),
        email: user.primary_email().map(str::to_owned),
        name: user.full_name().or_else(|| user.username.clone()),
        image_url: user.image_url.clone(),
    })
}

impl Inner {
    /// Completes a sign-in whose attempt or reload returned `reply`.
    async fn finish_sign_in(
        inner: &Arc<Inner>,
        reply: Envelope<SignInResource>,
    ) -> Result<Account, CloudError> {
        let sign_in = reply.response;
        unsupported_check(&sign_in)?;
        match sign_in.status.as_str() {
            "complete" => {}
            "needs_second_factor" | "needs_client_trust" => {
                return Err(CloudError::new(
                    "This account needs an extra verification step this app does not support yet. Try another sign-in method.",
                ));
            }
            "needs_new_password" => {
                return Err(CloudError::new(
                    "This account needs a new password. Reset it at accounts.t3.codes, then sign in here.",
                ));
            }
            status => {
                return Err(CloudError::new(format!(
                    "Sign-in did not complete ({status})."
                )));
            }
        }
        let session_id = sign_in
            .created_session_id
            .ok_or_else(|| CloudError::new("Sign-in completed without a session."))?;
        let mut account = account_from(&session_id, reply.client.as_ref());
        if account.is_none() {
            account = account_from(&session_id, inner.clerk.client().await?.as_ref());
        }
        let account =
            account.ok_or_else(|| CloudError::new("Sign-in completed without an account."))?;
        // A different account than before: its relay entries and tokens are not ours.
        inner.relay.reset().await;
        inner.set_account(Some(account.clone()))?;
        tokio::spawn({
            let inner = inner.clone();
            async move { Inner::refresh(&inner).await }
        });
        Ok(account)
    }

    fn set_account(&self, account: Option<Account>) -> Result<(), StoreError> {
        match &account {
            Some(account) => self.secrets.set(
                ACCOUNT_KEY,
                &serde_json::to_string(account).expect("accounts serialize"),
            )?,
            None => self.secrets.delete(ACCOUNT_KEY)?,
        }
        let changed = {
            let mut current = self.account.lock();
            let changed =
                current.as_ref().map(|a| &a.user_id) != account.as_ref().map(|a| &a.user_id);
            *current = account.clone();
            changed
        };
        if changed {
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
        self.state.send_modify(|state| {
            state.account = account;
            if changed {
                state.discovery = Discovery::default();
            }
        });
        Ok(())
    }

    /// Clerk no longer has our session (or the user signed out): forget the account.
    fn signed_out(&self) {
        if let Err(error) = self.set_account(None) {
            tracing::warn!(%error, "could not clear the stored T3 Connect account");
        }
    }

    /// A fresh `t3-relay` session JWT. Signs out locally when Clerk dropped the session.
    async fn session_jwt(&self) -> Result<String, ConnectionFailure> {
        let account = self.account.lock().clone().ok_or_else(sign_in_required)?;
        match self
            .clerk
            .session_token(&account.session_id, &self.config.jwt_template)
            .await
        {
            Ok(jwt) => Ok(jwt),
            Err(error) if error.is_signed_out() => {
                self.signed_out();
                Err(sign_in_required())
            }
            Err(error @ (ClerkError::Timeout | ClerkError::Network(_))) => {
                Err(ConnectionFailure::transient(
                    TransientReason::RelayUnavailable,
                    format!("{error} {NETWORK_BLOCKING_HINT}"),
                ))
            }
            Err(error) => Err(ConnectionFailure::blocked(
                BlockedReason::Authentication,
                error.to_string(),
            )
            .with_trace_id(error.trace_id().map(str::to_owned))),
        }
    }

    /// One discovery pass (upstream `discovery.ts:205-311`).
    async fn refresh(inner: &Arc<Inner>) {
        let generation = inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = |inner: &Inner| inner.generation.load(Ordering::SeqCst) == generation;
        if inner.account.lock().is_none() {
            inner
                .state
                .send_modify(|s| s.discovery = Discovery::default());
            return;
        }
        inner.state.send_modify(|s| {
            s.discovery = Discovery {
                refreshing: true,
                ..Discovery::default()
            }
        });
        let listed = async {
            let jwt = inner.session_jwt().await?;
            let environments = inner
                .relay
                .list_environments(&jwt)
                .await
                .map_err(|e| e.connection_failure())?;
            Ok::<_, ConnectionFailure>((jwt, environments))
        }
        .await;
        if !current(inner) {
            return;
        }
        let (jwt, environments) = match listed {
            Ok(listed) => listed,
            Err(failure) => {
                inner.state.send_modify(|s| {
                    s.discovery.refreshing = false;
                    // Signed out while listing: an empty list, not an error.
                    if s.account.is_some() {
                        s.discovery.error = Some(failure);
                    }
                });
                return;
            }
        };
        inner.state.send_modify(|s| {
            s.discovery.environments = environments
                .iter()
                .map(|environment| DiscoveredEnvironment {
                    environment: environment.clone(),
                    availability: Availability::Checking,
                })
                .collect();
        });
        let checks = environments.iter().map(|environment| {
            let jwt = &jwt;
            async move {
                let availability = match inner.relay.environment_status(jwt, environment).await {
                    Ok(status) if status.status == "online" => Availability::Online,
                    Ok(status) => Availability::Offline {
                        reason: status.error,
                    },
                    Err(failure) => Availability::Error(failure),
                };
                if current(inner) {
                    inner.state.send_modify(|s| {
                        if let Some(entry) =
                            s.discovery.environments.iter_mut().find(|e| {
                                e.environment.environment_id == environment.environment_id
                            })
                        {
                            entry.availability = availability;
                        }
                    });
                }
            }
        });
        futures::future::join_all(checks).await;
        if current(inner) {
            inner.state.send_modify(|s| s.discovery.refreshing = false);
        }
    }
}

fn sign_in_required() -> ConnectionFailure {
    ConnectionFailure::blocked(
        BlockedReason::Authentication,
        "Sign in to T3 Connect to connect this environment.",
    )
}

/// Bootstrap credentials from the relay for the signed-in account.
struct RelaySource(Arc<Inner>);

impl BootstrapSource for RelaySource {
    fn identity(&self) -> Option<String> {
        self.0.account.lock().as_ref().map(|a| a.user_id.clone())
    }

    fn bootstrap(
        &self,
        environment_id: EnvironmentId,
    ) -> BoxFuture<Result<Bootstrap, ConnectionFailure>> {
        let inner = self.0.clone();
        Box::pin(async move {
            let jwt = inner.session_jwt().await?;
            let connected = inner
                .relay
                .connect_environment(&jwt, &environment_id)
                .await
                .map_err(|e| e.connection_failure())?;
            if connected.environment_id != environment_id {
                return Err(ConnectionFailure::blocked(
                    BlockedReason::Configuration,
                    format!(
                        "Connected environment {} does not match {environment_id}.",
                        connected.environment_id
                    ),
                ));
            }
            let parse = |url: &str| {
                Url::parse(url).map_err(|_| {
                    ConnectionFailure::blocked(
                        BlockedReason::Configuration,
                        "Relay returned an invalid environment endpoint.",
                    )
                })
            };
            Ok(Bootstrap {
                http_base: parse(&connected.endpoint.http_base_url)?,
                ws_base: parse(&connected.endpoint.ws_base_url)?,
                credential: connected.credential,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FileSecretStore;

    fn secrets(name: &str) -> Arc<dyn SecretStore> {
        let path = std::env::temp_dir().join(format!(
            "t3ui-connect-{name}-{}-{}.json",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        Arc::new(FileSecretStore::at(path))
    }

    #[test]
    fn key_is_created_once_and_reused() {
        let secrets = secrets("key");
        let first = T3Connect::new(CloudConfig::production(), secrets.clone()).unwrap();
        let second = T3Connect::new(CloudConfig::production(), secrets.clone()).unwrap();
        assert_eq!(first.key_thumbprint(), second.key_thumbprint());
        assert!(first.account().is_none());
        assert!(first.state().borrow().account.is_none());
    }

    #[test]
    fn stored_account_shows_signed_in_and_owns_relay_entries() {
        let secrets = secrets("account");
        let account = Account {
            user_id: "user_1".into(),
            session_id: "sess_1".into(),
            email: Some("ada@example.com".into()),
            name: None,
            image_url: None,
        };
        secrets
            .set(ACCOUNT_KEY, &serde_json::to_string(&account).unwrap())
            .unwrap();
        let connect = T3Connect::new(CloudConfig::production(), secrets).unwrap();
        assert_eq!(connect.state().borrow().account.as_ref(), Some(&account));
        assert_eq!(account.display_name(), "ada@example.com");

        let record: RelayEnvironment = serde_json::from_str(
            r#"{"environmentId":"env-2","label":"devbox","linkedAt":"x",
                "endpoint":{"httpBaseUrl":"https://a.example/","wsBaseUrl":"wss://a.example/ws","providerKind":"cloudflare_tunnel"}}"#,
        )
        .unwrap();
        let saved = connect.saved_environment(&record).unwrap();
        assert_eq!(
            saved.target,
            SavedTarget::Known(KnownTarget::Relay {
                account_id: "user_1".into()
            })
        );
        assert!(connect.saved_endpoint(&saved).is_some());
        let mut other = saved.clone();
        other.target = SavedTarget::Known(KnownTarget::Relay {
            account_id: "user_2".into(),
        });
        assert!(connect.saved_endpoint(&other).is_none());

        let mut catalog = EnvironmentCatalog::default();
        catalog.upsert(saved);
        catalog.upsert(SavedEnvironment::new(
            "env-1".into(),
            "studio".into(),
            SavedTarget::Known(KnownTarget::Bearer {
                connection_id: "bearer:env-1".into(),
                http_base_url: Url::parse("https://studio.example/").unwrap(),
                ws_base_url: Url::parse("wss://studio.example/").unwrap(),
            }),
        ));
        assert_eq!(
            remove_relay_environments(&mut catalog),
            vec![EnvironmentId::from("env-2")]
        );
        assert_eq!(catalog.environments.len(), 1);
    }
}
