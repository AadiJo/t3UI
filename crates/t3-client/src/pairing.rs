//! Parsing what users paste to add an environment (`packages/shared/src/remote.ts`,
//! connections.md 2.2).
//!
//! Accepted inputs:
//! - a pairing URL: `http(s)|ws(s)://host[:port]/pair#token=TOKEN` (or `?token=`),
//! - a hosted pairing URL: `https://app.t3.codes/pair?host=<backend>&label=..#token=TOKEN`,
//! - a host plus a pairing code (two form fields),
//! - the text `t3 serve`, `t3 pair`, or `t3 auth pairing create` print, including `--json`.

use t3_protocol::environment::IssuedPairingCredential;
use url::Url;

/// Where to pair and with which one-time credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingTarget {
    pub credential: String,
    /// `http(s)://host[:port]/`.
    pub http_base: Url,
    /// `ws(s)://host[:port]/`.
    pub ws_base: Url,
}

/// Why pairing input was rejected. `Display` is the exact upstream copy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairingInputError {
    #[error("Enter a backend URL.")]
    BackendUrlMissing,
    #[error("Backend URL is invalid.")]
    BackendUrlInvalid,
    #[error("Pairing URL is invalid.")]
    PairingUrlInvalid,
    #[error("Pairing URL is missing its token.")]
    PairingTokenMissing,
    #[error("Enter a pairing code.")]
    PairingCodeMissing,
}

const SUPPORTED_SCHEMES: [&str; 4] = ["http", "https", "ws", "wss"];

/// Parses a pairing URL (plain or hosted).
pub fn parse_pairing_url(input: &str) -> Result<PairingTarget, PairingInputError> {
    let url = Url::parse(input.trim()).map_err(|_| PairingInputError::PairingUrlInvalid)?;
    if !SUPPORTED_SCHEMES.contains(&url.scheme()) {
        return Err(PairingInputError::PairingUrlInvalid);
    }
    let token = token_from_url(&url);
    // Hosted link: the backend is in `?host=`, not the URL's own origin.
    if let Some(host) = query_param(&url, "host")
        && let Some(token) = &token
    {
        let backend = normalize_base_url(&host)?;
        return Ok(target(token.clone(), backend));
    }
    let token = token.ok_or(PairingInputError::PairingTokenMissing)?;
    let mut base = url;
    base.set_path("/");
    base.set_query(None);
    base.set_fragment(None);
    Ok(target(token, base))
}

/// Resolves the "host" and "pairing code" form fields.
pub fn resolve_host_and_code(host: &str, code: &str) -> Result<PairingTarget, PairingInputError> {
    let base = normalize_base_url(host)?;
    let code = code.trim();
    if code.is_empty() {
        return Err(PairingInputError::PairingCodeMissing);
    }
    Ok(target(code.to_owned(), base))
}

/// Parses anything a user might paste: a URL, `t3 serve` / `t3 pair` output, or
/// `t3 auth pairing create [--json]` output.
pub fn parse_pairing_text(input: &str) -> Result<PairingTarget, PairingInputError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(PairingInputError::BackendUrlMissing);
    }
    if text.starts_with('{') {
        let issued: IssuedPairingCredential =
            serde_json::from_str(text).map_err(|_| PairingInputError::PairingUrlInvalid)?;
        let url = issued
            .pair_url
            .ok_or(PairingInputError::BackendUrlMissing)?;
        return parse_pairing_url(&url);
    }
    // `Pairing URL: <url>` (t3 serve, t3 pair) or `Pair URL: <url>` (t3 auth pairing create).
    if let Some(url) = labeled_value(text, &["Pairing URL:", "Pair URL:"]) {
        return parse_pairing_url(url);
    }
    // `Connection string: <base>` + `Token: <code>` (t3 serve without a pairing URL line).
    if let (Some(host), Some(code)) = (
        labeled_value(text, &["Connection string:"]),
        labeled_value(text, &["Token:"]),
    ) {
        return resolve_host_and_code(host, code);
    }
    // Otherwise the first word that parses as a pairing URL.
    text.split_whitespace()
        .find(|word| word.contains("://"))
        .map(parse_pairing_url)
        .unwrap_or(Err(PairingInputError::PairingUrlInvalid))
}

/// Upstream host normalization: trim, drop leading `/`, default to `https://`, require an
/// http/https/ws/wss scheme, and reduce to the origin with path `/`.
pub fn normalize_base_url(input: &str) -> Result<Url, PairingInputError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PairingInputError::BackendUrlMissing);
    }
    let without_slashes = trimmed.trim_start_matches('/');
    let with_scheme = if has_scheme(without_slashes) {
        without_slashes.to_owned()
    } else {
        format!("https://{without_slashes}")
    };
    let mut url = Url::parse(&with_scheme).map_err(|_| PairingInputError::BackendUrlInvalid)?;
    if !SUPPORTED_SCHEMES.contains(&url.scheme()) || url.host().is_none() {
        return Err(PairingInputError::BackendUrlInvalid);
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// `ws:` to `http:`, `wss:` to `https:`, path `/`.
pub fn http_base_url(url: &Url) -> Url {
    with_scheme_family(url, |scheme| match scheme {
        "ws" => "http",
        "wss" => "https",
        other => other,
    })
}

/// `http:` to `ws:`, `https:` to `wss:`, path `/`.
pub fn ws_base_url(url: &Url) -> Url {
    with_scheme_family(url, |scheme| match scheme {
        "http" => "ws",
        "https" => "wss",
        other => other,
    })
}

fn with_scheme_family(url: &Url, map: impl Fn(&str) -> &str) -> Url {
    let mut next = url.clone();
    let scheme = map(url.scheme()).to_owned();
    // All four schemes are "special" in the URL standard, so switching between them succeeds.
    let _ = next.set_scheme(&scheme);
    next.set_path("/");
    next.set_query(None);
    next.set_fragment(None);
    next
}

fn target(credential: String, base: Url) -> PairingTarget {
    PairingTarget {
        credential,
        http_base: http_base_url(&base),
        ws_base: ws_base_url(&base),
    }
}

/// `scheme://` prefix check matching upstream's `/^[a-zA-Z][a-zA-Z\d+-]*:\/\//`.
fn has_scheme(input: &str) -> bool {
    let Some((scheme, _)) = input.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-')
}

/// The token from the fragment (`#token=`) first, then the query (`?token=`).
fn token_from_url(url: &Url) -> Option<String> {
    let from_fragment = url.fragment().and_then(|fragment| {
        url::form_urlencoded::parse(fragment.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.trim().to_owned())
    });
    from_fragment
        .filter(|token| !token.is_empty())
        .or_else(|| query_param(url, "token"))
}

fn query_param(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The value after the first line starting with one of `labels`.
fn labeled_value<'a>(text: &'a str, labels: &[&str]) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let line = line.trim();
        labels
            .iter()
            .find_map(|label| line.strip_prefix(label))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

#[cfg(test)]
mod tests {
    //! Failure modes:
    //! 1. Token only in the query (`?token=`) is ignored, or the fragment is not preferred.
    //! 2. Hosted links (`app.t3.codes/pair?host=..`) pair with app.t3.codes instead of `host`.
    //! 3. A bare host without a scheme is not defaulted to `https://`, or a leading `/` breaks it.
    //! 4. The pairing path, query, or fragment leaks into the base URLs.
    //! 5. ws/wss and http/https are not swapped correctly (including the default port).
    //! 6. CLI output (`t3 serve`, `t3 auth pairing create`, `--json`) is not recognized.
    //! 7. Bad input gives the wrong user-facing message.
    use super::*;

    fn bases(target: &PairingTarget) -> (&str, &str) {
        (target.http_base.as_str(), target.ws_base.as_str())
    }

    #[test]
    fn pairing_url_reads_fragment_then_query() {
        let target = parse_pairing_url("http://localhost:4810/pair#token=ABC").unwrap();
        assert_eq!(target.credential, "ABC");
        assert_eq!(bases(&target), ("http://localhost:4810/", "ws://localhost:4810/"));

        let target = parse_pairing_url("https://box.ts.net/pair?token=Q1").unwrap();
        assert_eq!(target.credential, "Q1");
        assert_eq!(bases(&target), ("https://box.ts.net/", "wss://box.ts.net/"));

        let target = parse_pairing_url("https://box/pair?token=Q#token=F").unwrap();
        assert_eq!(target.credential, "F");
    }

    #[test]
    fn hosted_pairing_url_uses_host_param() {
        let target = parse_pairing_url(
            "https://app.t3.codes/pair?host=https%3A%2F%2Fstudio.tail1.ts.net%3A8443&label=studio#token=XYZ",
        )
        .unwrap();
        assert_eq!(target.credential, "XYZ");
        assert_eq!(
            bases(&target),
            ("https://studio.tail1.ts.net:8443/", "wss://studio.tail1.ts.net:8443/")
        );
    }

    #[test]
    fn websocket_scheme_urls_map_to_http() {
        let target = parse_pairing_url("wss://box.example/pair#token=T").unwrap();
        assert_eq!(bases(&target), ("https://box.example/", "wss://box.example/"));
    }

    #[test]
    fn host_and_code_normalize_host() {
        let target = resolve_host_and_code(" //studio.ts.net/some/path?x=1 ", " CODE ").unwrap();
        assert_eq!(target.credential, "CODE");
        assert_eq!(bases(&target), ("https://studio.ts.net/", "wss://studio.ts.net/"));

        let target = resolve_host_and_code("http://192.168.1.4:3773", "C").unwrap();
        assert_eq!(bases(&target), ("http://192.168.1.4:3773/", "ws://192.168.1.4:3773/"));
    }

    #[test]
    fn cli_output_is_recognized() {
        let serve = "T3 Code server is ready.\nConnection string: http://127.0.0.1:4810\nToken: 3K85F22BWWQA\nPairing URL: http://127.0.0.1:4810/pair#token=3K85F22BWWQA\n\n  █▀▀▀▀▀█ ▀█▀█";
        let target = parse_pairing_text(serve).unwrap();
        assert_eq!(target.credential, "3K85F22BWWQA");
        assert_eq!(target.http_base.as_str(), "http://127.0.0.1:4810/");

        let no_url = "Connection string: http://127.0.0.1:4810\nToken: ABCDEF";
        assert_eq!(parse_pairing_text(no_url).unwrap().credential, "ABCDEF");

        let create = "Issued client pairing token 1.\nToken: TOK\nPair URL: https://h.example/pair#token=TOK\nExpires at: 2026-01-01T00:00:00.000Z";
        let target = parse_pairing_text(create).unwrap();
        assert_eq!(target.http_base.as_str(), "https://h.example/");

        let json = r#"{"id":"1","credential":"TOK","scopes":["orchestration:read"],"expiresAt":"2026-01-01T00:00:00.000Z","pairUrl":"http://localhost:3773/pair#token=TOK"}"#;
        assert_eq!(parse_pairing_text(json).unwrap().credential, "TOK");

        let bare = "open http://localhost:3773/pair#token=ZZ to pair";
        assert_eq!(parse_pairing_text(bare).unwrap().credential, "ZZ");
    }

    #[test]
    fn errors_use_upstream_copy() {
        assert_eq!(
            parse_pairing_url("http://localhost:3773/pair").unwrap_err().to_string(),
            "Pairing URL is missing its token."
        );
        assert_eq!(
            parse_pairing_url("ftp://x/pair#token=a").unwrap_err().to_string(),
            "Pairing URL is invalid."
        );
        assert_eq!(
            resolve_host_and_code("", "x").unwrap_err().to_string(),
            "Enter a backend URL."
        );
        assert_eq!(
            resolve_host_and_code("box", " ").unwrap_err().to_string(),
            "Enter a pairing code."
        );
        assert_eq!(
            resolve_host_and_code("ftp://box", "x").unwrap_err().to_string(),
            "Backend URL is invalid."
        );
        let json_without_url = r#"{"id":"1","credential":"T","scopes":[],"expiresAt":"x"}"#;
        assert_eq!(
            parse_pairing_text(json_without_url).unwrap_err(),
            PairingInputError::BackendUrlMissing
        );
    }
}
