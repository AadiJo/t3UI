//! DPoP proof-of-possession (RFC 9449) with one ES256 key per install (connections.md 3.4).
//!
//! The same [`DpopKey`] signs proofs for the relay and for every T3 Connect environment, and its
//! [thumbprint](DpopKey::thumbprint) is what relay-minted environment credentials are bound to.
//! It is persisted in the secret store under [`SECRET_KEY`] so tokens survive restarts.
//!
//! Proofs match upstream's `apps/web/src/cloud/dpop.ts` (jose `SignJWT`) member for member:
//! header `{"typ","alg","jwk":{"kty","crv","x","y"}}`, payload `{"htm","htu","jti","ath"?,"iat"}`.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer as _};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use url::Url;

/// Secret store key of the base64url private scalar.
pub const SECRET_KEY: &str = "t3-connect:dpop-key";

/// The install's DPoP signing key with its public JWK and thumbprint precomputed.
#[derive(Clone)]
pub struct DpopKey {
    signing: SigningKey,
    jwk: PublicJwk,
    thumbprint: String,
}

impl std::fmt::Debug for DpopKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DpopKey")
            .field("thumbprint", &self.thumbprint)
            .finish_non_exhaustive()
    }
}

/// The public half as a JWK, members in the order jose emits them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicJwk {
    pub kty: &'static str,
    pub crv: &'static str,
    pub x: String,
    pub y: String,
}

#[derive(Debug, thiserror::Error)]
#[error("the stored T3 Connect key is invalid")]
pub struct InvalidDpopKey;

impl DpopKey {
    /// A fresh random key.
    pub fn generate() -> Self {
        Self::from_signing_key(SigningKey::random(&mut rand_core::OsRng))
    }

    /// Restores a key saved with [`secret`](Self::secret).
    pub fn from_secret(secret: &str) -> Result<Self, InvalidDpopKey> {
        let bytes = URL_SAFE_NO_PAD
            .decode(secret.trim())
            .map_err(|_| InvalidDpopKey)?;
        let signing = SigningKey::from_slice(&bytes).map_err(|_| InvalidDpopKey)?;
        Ok(Self::from_signing_key(signing))
    }

    fn from_signing_key(signing: SigningKey) -> Self {
        let point = signing.verifying_key().to_encoded_point(false);
        let jwk = PublicJwk {
            kty: "EC",
            crv: "P-256",
            x: URL_SAFE_NO_PAD.encode(point.x().expect("uncompressed point has x")),
            y: URL_SAFE_NO_PAD.encode(point.y().expect("uncompressed point has y")),
        };
        let thumbprint = jwk_thumbprint(&jwk);
        DpopKey {
            signing,
            jwk,
            thumbprint,
        }
    }

    /// The private scalar, base64url without padding. Store it only in the secret store.
    pub fn secret(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.signing.to_bytes())
    }

    pub fn public_jwk(&self) -> &PublicJwk {
        &self.jwk
    }

    /// RFC 7638 SHA-256 thumbprint (`jkt`), sent as `clientKeyThumbprint` when connecting.
    pub fn thumbprint(&self) -> &str {
        &self.thumbprint
    }

    /// A single-use proof for one request. `access_token` adds `ath` (requests that present a
    /// DPoP access token); token exchanges send none. Never reuse a proof: servers reject
    /// replayed `jti`s.
    pub fn proof(&self, method: &str, url: &Url, access_token: Option<&str>) -> String {
        self.proof_at(
            method,
            url,
            access_token,
            chrono::Utc::now().timestamp(),
            &uuid::Uuid::new_v4().to_string(),
        )
    }

    /// [`proof`](Self::proof) with a fixed time and `jti` (tests).
    pub fn proof_at(
        &self,
        method: &str,
        url: &Url,
        access_token: Option<&str>,
        issued_at: i64,
        jti: &str,
    ) -> String {
        #[derive(Serialize)]
        struct Header<'a> {
            typ: &'static str,
            alg: &'static str,
            jwk: &'a PublicJwk,
        }
        #[derive(Serialize)]
        struct Payload<'a> {
            htm: String,
            htu: String,
            jti: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            ath: Option<String>,
            iat: i64,
        }
        let header = Header {
            typ: "dpop+jwt",
            alg: "ES256",
            jwk: &self.jwk,
        };
        let payload = Payload {
            htm: method.to_ascii_uppercase(),
            htu: htu(url),
            jti,
            ath: access_token.map(access_token_hash),
            iat: issued_at,
        };
        let signing_input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("header serializes")),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("payload serializes")),
        );
        // ES256: ECDSA P-256 over SHA-256 of the signing input, raw 64-byte r||s (RFC 7518 3.4).
        let signature: Signature = self.signing.sign(signing_input.as_bytes());
        format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }
}

/// `htu`: the request URL without query and fragment, WHATWG-serialized (what `Url` prints).
pub fn htu(url: &Url) -> String {
    let mut url = url.clone();
    url.set_query(None);
    url.set_fragment(None);
    url.into()
}

/// `ath`: base64url(SHA-256(access token)).
pub fn access_token_hash(access_token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(access_token.as_bytes()))
}

/// RFC 7638: SHA-256 over the required members in lexicographic order, no whitespace.
pub fn jwk_thumbprint(jwk: &PublicJwk) -> String {
    let canonical = format!(
        r#"{{"crv":"{}","kty":"{}","x":"{}","y":"{}"}}"#,
        jwk.crv, jwk.kty, jwk.x, jwk.y
    );
    URL_SAFE_NO_PAD.encode(Sha256::digest(canonical.as_bytes()))
}

// Failure modes these tests pin down (each breaks auth against the relay or an environment,
// whose verifier is upstream `packages/shared/src/dpop.ts`):
//  1. Header members other than typ/alg/jwk, or a `d` in the jwk -> malformed proof.
//  2. JWK coordinates not exactly 32 bytes base64url -> verifier cannot rebuild the key.
//  3. Thumbprint over the wrong member order or with whitespace -> key_mismatch against a
//     relay-minted credential bound to our `jkt`.
//  4. `htu` keeping the query/fragment, or not WHATWG-serialized -> request_mismatch.
//  5. `ath` hashed over the wrong bytes, or present/missing in the wrong requests -> token_mismatch.
//  6. DER instead of raw r||s, or a signature not over `b64(header).b64(payload)` -> invalid_proof.
//  7. `iat` in milliseconds -> time_window.
//  8. The same `jti` twice -> replay.
//  9. A key that does not survive a save/restore -> every bound token breaks after a restart.
// Cross-checked against upstream's verifier in Node by `e2e/dpop-crosscheck.mjs`.
#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{VerifyingKey, signature::Verifier as _};
    use serde_json::Value;

    fn decode(part: &str) -> Value {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
    }

    fn parts(proof: &str) -> Vec<&str> {
        proof.split('.').collect()
    }

    #[test]
    fn rfc9449_thumbprint_and_ath_vectors() {
        // RFC 9449 section 4.1 key with its section 6.1 `jkt`, and the section 7.1 `ath`.
        let jwk = PublicJwk {
            kty: "EC",
            crv: "P-256",
            x: "l8tFrhx-34tV3hRICRDY9zCkDlpBhF42UQUfWVAWBFs".into(),
            y: "9VE4jf_Ok_o64zbTTlcuNJajHmt6v9TDVrU0CdvGRDA".into(),
        };
        assert_eq!(
            jwk_thumbprint(&jwk),
            "0ZcOCORZNYy-DWpqq30jZyJGHTN0d2HglBV3uiguA4I"
        );
        assert_eq!(
            access_token_hash("Kz~8mXK1EalYznwH-LC-1fBAo.4Ljp~zsPE_NeO.gxU"),
            "fUHyO2r2Z3DZ53EsNrWBb0xWXoaNy59IiKCAqksmQEo"
        );
    }

    #[test]
    fn header_and_payload_members_match_jose() {
        let key = DpopKey::generate();
        let url = Url::parse("https://relay.t3.codes/v1/client/dpop-token").unwrap();
        let proof = key.proof_at("post", &url, None, 1_790_000_000, "jti-1");
        let parts = parts(&proof);
        assert_eq!(parts.len(), 3);
        // Exact bytes, including member order: a reordering would still verify, but this is
        // what upstream's jose output looks like.
        let header = String::from_utf8(URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(
            header,
            format!(
                r#"{{"typ":"dpop+jwt","alg":"ES256","jwk":{{"kty":"EC","crv":"P-256","x":"{}","y":"{}"}}}}"#,
                key.public_jwk().x,
                key.public_jwk().y
            )
        );
        let payload = String::from_utf8(URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(
            payload,
            r#"{"htm":"POST","htu":"https://relay.t3.codes/v1/client/dpop-token","jti":"jti-1","iat":1790000000}"#
        );
        assert!(decode(parts[0])["jwk"].get("d").is_none());
    }

    #[test]
    fn coordinates_are_32_bytes() {
        for _ in 0..64 {
            let key = DpopKey::generate();
            assert_eq!(
                URL_SAFE_NO_PAD.decode(&key.public_jwk().x).unwrap().len(),
                32
            );
            assert_eq!(
                URL_SAFE_NO_PAD.decode(&key.public_jwk().y).unwrap().len(),
                32
            );
        }
    }

    #[test]
    fn ath_only_with_an_access_token() {
        let key = DpopKey::generate();
        let url = Url::parse("https://env.example/api/auth/websocket-ticket").unwrap();
        let with = key.proof("POST", &url, Some("token-1"));
        assert_eq!(
            decode(parts(&with)[1])["ath"],
            Value::String(access_token_hash("token-1"))
        );
        let without = key.proof("POST", &url, None);
        assert!(decode(parts(&without)[1]).get("ath").is_none());
    }

    #[test]
    fn htu_drops_query_and_fragment_only() {
        let url = Url::parse("https://Host.Example:443/oauth/token?x=1#frag").unwrap();
        assert_eq!(htu(&url), "https://host.example/oauth/token");
        let root = Url::parse("http://127.0.0.1:4710").unwrap();
        assert_eq!(htu(&root), "http://127.0.0.1:4710/");
    }

    #[test]
    fn signature_is_raw_es256_over_the_signing_input() {
        let key = DpopKey::generate();
        let url = Url::parse("https://env.example/oauth/token").unwrap();
        let proof = key.proof("POST", &url, None);
        let parts = parts(&proof);
        let signature_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        assert_eq!(signature_bytes.len(), 64);
        let signature = Signature::from_slice(&signature_bytes).unwrap();
        let jwk = decode(parts[0])["jwk"].clone();
        let mut sec1 = vec![4u8];
        sec1.extend(URL_SAFE_NO_PAD.decode(jwk["x"].as_str().unwrap()).unwrap());
        sec1.extend(URL_SAFE_NO_PAD.decode(jwk["y"].as_str().unwrap()).unwrap());
        let verifying = VerifyingKey::from_sec1_bytes(&sec1).unwrap();
        let input = format!("{}.{}", parts[0], parts[1]);
        verifying.verify(input.as_bytes(), &signature).unwrap();
    }

    #[test]
    fn iat_is_seconds_and_jti_is_fresh() {
        let key = DpopKey::generate();
        let url = Url::parse("https://env.example/oauth/token").unwrap();
        let a = decode(parts(&key.proof("POST", &url, None))[1]);
        let b = decode(parts(&key.proof("POST", &url, None))[1]);
        let now = chrono::Utc::now().timestamp();
        assert!((a["iat"].as_i64().unwrap() - now).abs() <= 2);
        assert_ne!(a["jti"], b["jti"]);
    }

    #[test]
    fn key_round_trips_through_its_secret() {
        let key = DpopKey::generate();
        let restored = DpopKey::from_secret(&key.secret()).unwrap();
        assert_eq!(restored.thumbprint(), key.thumbprint());
        assert_eq!(restored.public_jwk(), key.public_jwk());
        assert!(DpopKey::from_secret("not a key").is_err());
        assert!(DpopKey::from_secret(&URL_SAFE_NO_PAD.encode([0u8; 32])).is_err());
    }
}
