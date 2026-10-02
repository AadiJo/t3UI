//! Reading claims from JWTs we were handed by Clerk, without verifying them. Only for routing
//! decisions (which account a token belongs to); servers do the verification.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct Claims {
    sub: Option<String>,
}

/// The `sub` claim (`user_...` for Clerk session tokens), if the token decodes and has one.
pub fn subject(jwt: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Claims = serde_json::from_slice(&bytes).ok()?;
    claims.sub.filter(|sub| !sub.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_subject_and_rejects_garbage() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"user_1","aud":"t3-code-relay"}"#);
        assert_eq!(
            subject(&format!("h.{payload}.s")).as_deref(),
            Some("user_1")
        );
        let empty = URL_SAFE_NO_PAD.encode(br#"{"sub":""}"#);
        assert_eq!(subject(&format!("h.{empty}.s")), None);
        assert_eq!(subject("not-a-jwt"), None);
        assert_eq!(subject("a.!!!.c"), None);
    }
}
