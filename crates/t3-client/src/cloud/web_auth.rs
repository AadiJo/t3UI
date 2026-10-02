//! The system browser sheet for provider sign-in: [`system_authenticator`] returns the
//! platform's [`WebAuthenticator`] (macOS: `ASWebAuthenticationSession`), or `None` where
//! there is none and only email codes work. Must be called from a GUI app: the sheet anchors to
//! the key window and runs on the main thread (the future itself can be awaited anywhere).

use std::sync::Arc;

use super::oauth::WebAuthenticator;

/// The authenticator for this platform, if it has one.
pub fn system_authenticator() -> Option<Arc<dyn WebAuthenticator>> {
    #[cfg(target_os = "macos")]
    return Some(Arc::new(macos::SystemBrowserSheet));
    #[cfg(not(target_os = "macos"))]
    None
}

#[cfg(target_os = "macos")]
mod macos {
    //! `ASWebAuthenticationSession` captures the callback scheme itself, so `t3code://` needs
    //! no `Info.plist` registration and an installed T3 Code app never sees the callback. The
    //! session shares Safari's cookies (`prefersEphemeralWebBrowserSession = NO`), so a user
    //! already signed in to GitHub there only confirms.

    use std::{cell::RefCell, rc::Rc};

    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::{
        AnyThread as _, DefinedClass as _, MainThreadMarker, MainThreadOnly, define_class,
        msg_send,
        rc::Retained,
        runtime::{NSObject, NSObjectProtocol, ProtocolObject},
    };
    use objc2_app_kit::{NSApplication, NSWindow};
    use objc2_authentication_services::{
        ASPresentationAnchor, ASWebAuthenticationPresentationContextProviding,
        ASWebAuthenticationSession, ASWebAuthenticationSessionErrorCode,
    };
    use objc2_foundation::{NSError, NSString, NSURL};
    use tokio::sync::oneshot;
    use url::Url;

    use super::super::oauth::{WebAuthError, WebAuthenticator};
    use crate::auth::BoxFuture;

    /// Opens provider sign-in in the system browser sheet over the key window.
    pub(super) struct SystemBrowserSheet;

    impl WebAuthenticator for SystemBrowserSheet {
        fn authenticate(
            &self,
            url: Url,
            callback_scheme: String,
        ) -> BoxFuture<Result<Url, WebAuthError>> {
            let (sender, receiver) = oneshot::channel();
            // AppKit objects live on the main thread; the session reports back on it too.
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("the main queue runs on the main thread");
                if let Err(error) = start(mtm, &url, &callback_scheme, sender) {
                    tracing::warn!("could not start provider sign-in: {error:?}");
                }
            });
            Box::pin(async move {
                receiver.await.unwrap_or_else(|_| {
                    Err(WebAuthError::Failed(
                        "The sign-in window closed unexpectedly.".into(),
                    ))
                })
            })
        }
    }

    type Reply = oneshot::Sender<Result<Url, WebAuthError>>;

    /// Starts one session. The session and its anchor provider are kept alive by the
    /// completion handler until it runs (the session holds the provider weakly).
    fn start(
        mtm: MainThreadMarker,
        url: &Url,
        callback_scheme: &str,
        reply: Reply,
    ) -> Result<(), &'static str> {
        let reply = Rc::new(RefCell::new(Some(reply)));
        let send = {
            let reply = reply.clone();
            move |result: Result<Url, WebAuthError>| {
                if let Some(reply) = reply.borrow_mut().take() {
                    let _ = reply.send(result);
                }
            }
        };
        let Some(window) = anchor_window(mtm) else {
            send(Err(WebAuthError::Failed(
                "Open the T3UI window, then try again.".into(),
            )));
            return Err("no window to anchor the sheet");
        };
        let Some(ns_url) = NSURL::URLWithString(&NSString::from_str(url.as_str())) else {
            send(Err(WebAuthError::Failed(
                "The sign-in URL is invalid.".into(),
            )));
            return Err("invalid URL");
        };
        let keep_alive: Rc<RefCell<Option<KeepAlive>>> = Rc::default();
        let completion = {
            let keep_alive = keep_alive.clone();
            let send = send.clone();
            RcBlock::new(move |callback: *mut NSURL, error: *mut NSError| {
                // SAFETY: AppKit passes either a valid URL or a valid error (or null).
                let callback = unsafe { callback.as_ref() };
                let error = unsafe { error.as_ref() };
                send(completion_result(callback, error));
                keep_alive.borrow_mut().take();
            })
        };
        let provider = PresentationProvider::new(mtm, window);
        // SAFETY: the block pointer is valid for the call (the session copies it).
        #[allow(deprecated)] // `initWithURL:callback:` needs macOS 14.4.
        let session = unsafe {
            ASWebAuthenticationSession::initWithURL_callbackURLScheme_completionHandler(
                ASWebAuthenticationSession::alloc(),
                &ns_url,
                Some(&NSString::from_str(callback_scheme)),
                &*completion as *const _ as *mut _,
            )
        };
        unsafe {
            session.setPresentationContextProvider(Some(ProtocolObject::from_ref(&*provider)));
            session.setPrefersEphemeralWebBrowserSession(false);
        }
        *keep_alive.borrow_mut() = Some(KeepAlive {
            _session: session.clone(),
            _provider: provider,
        });
        if !unsafe { session.start() } {
            keep_alive.borrow_mut().take();
            send(Err(WebAuthError::Failed(
                "The sign-in window could not open.".into(),
            )));
            return Err("start returned NO");
        }
        Ok(())
    }

    struct KeepAlive {
        _session: Retained<ASWebAuthenticationSession>,
        _provider: Retained<PresentationProvider>,
    }

    fn completion_result(
        callback: Option<&NSURL>,
        error: Option<&NSError>,
    ) -> Result<Url, WebAuthError> {
        if let Some(callback) = callback {
            return callback
                .absoluteString()
                .and_then(|s| Url::parse(&s.to_string()).ok())
                .ok_or_else(|| WebAuthError::Failed("The sign-in callback was invalid.".into()));
        }
        match error {
            Some(error) if error.code() == ASWebAuthenticationSessionErrorCode::CanceledLogin.0 => {
                Err(WebAuthError::Cancelled)
            }
            Some(error) => Err(WebAuthError::Failed(
                error.localizedDescription().to_string(),
            )),
            None => Err(WebAuthError::Cancelled),
        }
    }

    /// The key window, else the main window, else any window.
    fn anchor_window(mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
        let app = NSApplication::sharedApplication(mtm);
        app.keyWindow()
            .or_else(|| app.mainWindow())
            .or_else(|| app.windows().firstObject())
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements; no Drop impl.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "T3UIWebAuthenticationAnchor"]
        #[ivars = Retained<NSWindow>]
        struct PresentationProvider;

        unsafe impl NSObjectProtocol for PresentationProvider {}

        unsafe impl ASWebAuthenticationPresentationContextProviding for PresentationProvider {
            #[unsafe(method_id(presentationAnchorForWebAuthenticationSession:))]
            fn presentation_anchor(
                &self,
                _session: &ASWebAuthenticationSession,
            ) -> Retained<ASPresentationAnchor> {
                Retained::into_super(Retained::into_super(self.ivars().clone()))
            }
        }
    );

    impl PresentationProvider {
        fn new(mtm: MainThreadMarker, window: Retained<NSWindow>) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(window);
            // SAFETY: NSObject's designated initializer.
            unsafe { msg_send![super(this), init] }
        }
    }
}
