//! T3 Connect: sign in to a T3 account and reach the environments linked to it through the
//! relay (connections.md 3, 4, 7.7). GPUI-free.
//!
//! - [`T3Connect`]: the handle the app keeps. Sign-in ([`T3Connect::start_email_sign_in`],
//!   [`T3Connect::sign_in_with`]), sign-out, the linked-environment list
//!   ([`T3Connect::refresh`] into [`T3Connect::state`]), and endpoints for connecting
//!   ([`T3Connect::endpoint`], [`T3Connect::saved_endpoint`]).
//! - [`clerk`]: Clerk Frontend API in native mode (client token in `Authorization`).
//! - [`relay`]: `relay.t3.codes` (listing, status, DPoP access tokens, one-time credentials).
//! - [`endpoint`]: [`DpopEndpoint`], the [`Endpoint`](crate::Endpoint) for DPoP-authorized
//!   environments, with a [`BootstrapSource`] seam for where credentials come from.
//! - [`dpop`]: the install's ES256 key and RFC 9449 proofs.
//! - [`oauth`]: providers and the [`WebAuthenticator`] seam the app implements with
//!   `ASWebAuthenticationSession`.
//!
//! Secrets (the DPoP key, the Clerk client token, the account) live in the
//! [`SecretStore`](crate::store::SecretStore) under `t3-connect:*` keys. Connected T3 Connect
//! environments are ordinary catalog entries with a [`KnownTarget::Relay`]
//! (crate::store::KnownTarget::Relay) target owned by the account that added them.

pub mod clerk;
mod connect;
pub mod dpop;
pub mod endpoint;
mod jwt;
mod net;
pub mod oauth;
pub mod relay;

pub use connect::{
    ACCOUNT_KEY, Account, Availability, CloudError, CloudState, DiscoveredEnvironment, Discovery,
    EmailSignIn, T3Connect, remove_relay_environments,
};
pub use endpoint::{Bootstrap, BootstrapSource, DpopEndpoint};
pub use oauth::{OAuthProvider, WebAuthError, WebAuthenticator};
pub use relay::{RelayEnvironment, RelayManagedEndpoint};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use url::Url;

/// T3's production Clerk publishable key (public; `U:.env.example`).
pub const PUBLISHABLE_KEY: &str = "pk_live_Y2xlcmsudDMuY29kZXMk";

/// Public T3 Connect configuration. [`CloudConfig::production`] is what official builds use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudConfig {
    /// Clerk Frontend API origin (`https://clerk.t3.codes`).
    pub clerk_frontend_api: Url,
    /// Clerk JWT template whose tokens the relay accepts (`t3-relay`).
    pub jwt_template: String,
    /// Relay origin and OAuth issuer (`https://relay.t3.codes`).
    pub relay_url: Url,
    /// Relay public client id (`t3-web`, allowed `environment:connect environment:status`).
    pub relay_client_id: &'static str,
    /// Where Clerk sends the browser after a provider sign-in. `t3code://app/` is on the
    /// instance's native redirect allowlist (the desktop app's).
    pub oauth_redirect_url: Url,
    /// Account portal, for signing up (`https://accounts.t3.codes`).
    pub accounts_url: Url,
}

impl CloudConfig {
    pub fn production() -> Self {
        CloudConfig {
            clerk_frontend_api: frontend_api_from_publishable_key(PUBLISHABLE_KEY)
                .expect("the production publishable key is valid"),
            jwt_template: "t3-relay".into(),
            relay_url: Url::parse("https://relay.t3.codes").expect("valid URL"),
            relay_client_id: "t3-web",
            oauth_redirect_url: Url::parse("t3code://app/").expect("valid URL"),
            accounts_url: Url::parse("https://accounts.t3.codes").expect("valid URL"),
        }
    }

    /// `https://accounts.t3.codes/sign-up`.
    pub fn sign_up_url(&self) -> Url {
        self.accounts_url.join("/sign-up").expect("valid path")
    }
}

/// The Frontend API origin a Clerk publishable key encodes: `pk_<live|test>_` + base64 of
/// `<host>$` (upstream `packages/shared/src/relayAuth.ts:31-78`).
pub fn frontend_api_from_publishable_key(key: &str) -> Option<Url> {
    let encoded = key.trim().splitn(3, '_').nth(2)?;
    let decoded = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
    let host = decoded.strip_suffix('$').unwrap_or(&decoded);
    if host.is_empty() || host.contains('/') {
        return None;
    }
    Url::parse(&format!("https://{host}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_frontend_api() {
        assert_eq!(
            frontend_api_from_publishable_key(PUBLISHABLE_KEY)
                .unwrap()
                .as_str(),
            "https://clerk.t3.codes/"
        );
        assert!(frontend_api_from_publishable_key("pk_live_").is_none());
        assert!(frontend_api_from_publishable_key("pk_live_!!").is_none());
        // "a/b$" contains a path.
        assert!(
            frontend_api_from_publishable_key(&format!("pk_test_{}", STANDARD.encode("a/b$")))
                .is_none()
        );
        assert_eq!(
            CloudConfig::production().sign_up_url().as_str(),
            "https://accounts.t3.codes/sign-up"
        );
    }
}
