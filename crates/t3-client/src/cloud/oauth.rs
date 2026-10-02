//! Social sign-in providers and the browser seam they need.
//!
//! Clerk's OAuth sign-in hands back a provider URL; the user finishes there and the browser
//! is redirected to `t3code://app/?rotating_token_nonce=..`. A [`WebAuthenticator`] opens the URL
//! and resolves with that callback. On macOS the app implements it with
//! `ASWebAuthenticationSession`, which captures the callback scheme itself (no URL scheme
//! registration, no clash with an installed T3 Code). Email-code sign-in needs none of this.

use url::Url;

use crate::auth::BoxFuture;

/// The social providers T3's Clerk instance enables (`GET /v1/environment`, verified live).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OAuthProvider {
    GitHub,
    Google,
    Apple,
    Microsoft,
}

impl OAuthProvider {
    /// In the order Clerk's sign-in lists them.
    pub const ALL: [OAuthProvider; 4] = [
        OAuthProvider::Apple,
        OAuthProvider::GitHub,
        OAuthProvider::Google,
        OAuthProvider::Microsoft,
    ];

    /// The Clerk strategy (`oauth_github`).
    pub fn strategy(self) -> &'static str {
        match self {
            OAuthProvider::GitHub => "oauth_github",
            OAuthProvider::Google => "oauth_google",
            OAuthProvider::Apple => "oauth_apple",
            OAuthProvider::Microsoft => "oauth_microsoft",
        }
    }

    /// Display name ("GitHub").
    pub fn label(self) -> &'static str {
        match self {
            OAuthProvider::GitHub => "GitHub",
            OAuthProvider::Google => "Google",
            OAuthProvider::Apple => "Apple",
            OAuthProvider::Microsoft => "Microsoft",
        }
    }
}

/// Why a browser session ended without a callback.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebAuthError {
    /// The user closed the browser sheet.
    #[error("Sign-in was cancelled.")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

/// Runs one browser-based sign-in: opens `url` and resolves with the first URL the browser is
/// sent to whose scheme is `callback_scheme`.
pub trait WebAuthenticator: Send + Sync {
    fn authenticate(
        &self,
        url: Url,
        callback_scheme: String,
    ) -> BoxFuture<Result<Url, WebAuthError>>;
}

/// What an OAuth callback says (clerk-js `oauthTransport` handling).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Callback {
    /// Reload the sign-in with this nonce (may be absent; then reload without one).
    Continue {
        rotating_token_nonce: Option<String>,
    },
    /// Clerk reported a failure.
    Failed { message: String },
}

/// Codes clerk-js treats as "continue to sign-up" rather than failure.
const TRANSFER_CODES: [&str; 2] = ["external_account_not_found", "external_account_exists"];

pub(crate) fn read_callback(url: &Url) -> Callback {
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    if param("__clerk_status").as_deref() == Some("failed") {
        let code = param("__clerk_error_code").unwrap_or_else(|| "oauth_callback_failed".into());
        if !TRANSFER_CODES.contains(&code.as_str()) {
            let message = match code.as_str() {
                "oauth_access_denied" => "You did not grant access to your account.".to_owned(),
                _ => format!("The provider sign-in failed ({code})."),
            };
            return Callback::Failed { message };
        }
    }
    Callback::Continue {
        rotating_token_nonce: param("rotating_token_nonce"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callbacks() {
        let ok = Url::parse("t3code://app/?rotating_token_nonce=abc").unwrap();
        assert_eq!(
            read_callback(&ok),
            Callback::Continue {
                rotating_token_nonce: Some("abc".into())
            }
        );
        let denied = Url::parse(
            "t3code://app/?__clerk_status=failed&__clerk_error_code=oauth_access_denied",
        )
        .unwrap();
        assert_eq!(
            read_callback(&denied),
            Callback::Failed {
                message: "You did not grant access to your account.".into()
            }
        );
        let transfer = Url::parse(
            "t3code://app/?__clerk_status=failed&__clerk_error_code=external_account_not_found",
        )
        .unwrap();
        assert!(matches!(
            read_callback(&transfer),
            Callback::Continue { .. }
        ));
    }
}
