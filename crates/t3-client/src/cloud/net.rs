//! One HTTP exchange on the networking runtime, for the Clerk and relay clients. Unlike
//! [`EnvironmentHttp`](crate::EnvironmentHttp) it hands back the status, the `Authorization`
//! response header (Clerk rotates its client token through it), and the raw body.

use std::time::Duration;

use reqwest::RequestBuilder;

/// A completed exchange, any status.
pub(crate) struct Reply {
    pub status: u16,
    /// The response `Authorization` header, if any.
    pub authorization: Option<String>,
    pub body: Vec<u8>,
}

/// The request never completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NetError {
    Timeout,
    /// Transport failure (DNS, TLS, refused), with its root cause.
    Network(String),
}

/// The shared HTTP client (same connection pool as environment requests).
pub(crate) fn client() -> &'static reqwest::Client {
    crate::http::client()
}

/// Sends `request` on the networking runtime with `timeout`.
pub(crate) async fn send(request: RequestBuilder, timeout: Duration) -> Result<Reply, NetError> {
    crate::runtime::spawn(async move {
        let response = request.timeout(timeout).send().await.map_err(map_error)?;
        let status = response.status().as_u16();
        let authorization = response
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.bytes().await.map_err(map_error)?.to_vec();
        Ok(Reply {
            status,
            authorization,
            body,
        })
    })
    .await
}

fn map_error(error: reqwest::Error) -> NetError {
    if error.is_timeout() {
        return NetError::Timeout;
    }
    let mut current: &dyn std::error::Error = &error;
    while let Some(source) = current.source() {
        current = source;
    }
    NetError::Network(current.to_string())
}
