//! The browser sheet for provider sign-in ([`WebAuthenticator`]). macOS uses
//! `ASWebAuthenticationSession`; elsewhere provider buttons are hidden and email codes are the
//! only way in.

use std::sync::Arc;

use t3_client::{
    auth::BoxFuture,
    cloud::{WebAuthError, WebAuthenticator},
};
use url::Url;

/// The authenticator for this platform, if it has one.
pub(super) fn platform_authenticator() -> Option<Arc<dyn WebAuthenticator>> {
    #[cfg(target_os = "macos")]
    return Some(Arc::new(macos::SystemBrowserSheet));
    #[cfg(not(target_os = "macos"))]
    None
}

/// Stands in for the platform authenticator in snapshot scenes (shows the buttons; fails if
/// pressed).
pub(super) struct Unavailable;

impl WebAuthenticator for Unavailable {
    fn authenticate(&self, _: Url, _: String) -> BoxFuture<Result<Url, WebAuthError>> {
        Box::pin(async {
            Err(WebAuthError::Failed(
                "Provider sign-in is unavailable.".into(),
            ))
        })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    /// `ASWebAuthenticationSession` anchored to the key window.
    pub(super) struct SystemBrowserSheet;

    impl WebAuthenticator for SystemBrowserSheet {
        fn authenticate(
            &self,
            _url: Url,
            _callback_scheme: String,
        ) -> BoxFuture<Result<Url, WebAuthError>> {
            Box::pin(async {
                Err(WebAuthError::Failed(
                    "Provider sign-in is not available yet.".into(),
                ))
            })
        }
    }
}
