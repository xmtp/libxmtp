//! Immutable key snapshots. Each key owns validation for one algorithm.
use crate::config::auth::{AuthConfig, MAX_KID_BYTES, algorithm};
use arc_swap::ArcSwap;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::Jwk};
use p256::pkcs8::DecodePublicKey;
use std::sync::Arc;

pub(crate) struct VerifyingKey {
    pub kid: Option<String>,
    pub alg: Algorithm,
    pub key: DecodingKey,
    pub validation: Validation,
}

pub(crate) struct KeySet(pub ArcSwap<Vec<VerifyingKey>>);
impl KeySet {
    pub fn new(keys: Vec<VerifyingKey>) -> Self {
        crate::telemetry::auth_keys(keys.len());
        Self(ArcSwap::from_pointee(keys))
    }
    pub fn replace(&self, keys: Vec<VerifyingKey>) {
        crate::telemetry::auth_keys(keys.len());
        self.0.store(Arc::new(keys));
    }
}

/// Parse SPKI and check the exact curve or key family before accepting the key.
/// All parser errors are discarded so callers cannot disclose PEM contents.
pub(crate) fn parse_public_key(pem: &str, alg: Algorithm) -> Result<DecodingKey, &'static str> {
    let failure = "invalid public key";
    if !pem.trim_start().starts_with("-----BEGIN PUBLIC KEY-----") {
        return Err(failure);
    }
    let valid = match alg {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            rsa::RsaPublicKey::from_public_key_pem(pem).is_ok()
        }
        Algorithm::ES256 => p256::PublicKey::from_public_key_pem(pem).is_ok(),
        Algorithm::ES384 => p384::PublicKey::from_public_key_pem(pem).is_ok(),
        Algorithm::EdDSA => ed25519_dalek::VerifyingKey::from_public_key_pem(pem).is_ok(),
        _ => false,
    };
    if !valid {
        return Err(failure);
    }
    match alg {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            DecodingKey::from_rsa_pem(pem.as_bytes())
        }
        Algorithm::ES256 | Algorithm::ES384 => DecodingKey::from_ec_pem(pem.as_bytes()),
        Algorithm::EdDSA => DecodingKey::from_ed_pem(pem.as_bytes()),
        _ => return Err(failure),
    }
    .map_err(|_| failure)
}

/// Build an independent validation object. Missing configured claims must fail.
fn validation(alg: Algorithm, config: &AuthConfig) -> Validation {
    let mut value = Validation::new(alg);
    value.validate_exp = true;
    value.validate_nbf = true;
    value.leeway = config.leeway_seconds;
    value.set_required_spec_claims(&["exp"]);
    value.validate_aud = config.audiences.is_some();
    if let Some(audiences) = &config.audiences {
        value.set_audience(audiences);
        value.required_spec_claims.insert("aud".into());
    }
    if let Some(issuers) = &config.issuers {
        value.set_issuer(issuers);
        value.required_spec_claims.insert("iss".into());
    }
    value
}

pub(crate) fn inline(config: &AuthConfig) -> Result<Vec<VerifyingKey>, &'static str> {
    config
        .keys
        .iter()
        .flatten()
        .map(|entry| {
            let alg = algorithm(&entry.alg).ok_or("invalid algorithm")?;
            Ok(VerifyingKey {
                kid: Some(entry.kid.clone()),
                alg,
                key: parse_public_key(&entry.public_key, alg)?,
                validation: validation(alg, config),
            })
        })
        .collect()
}

/// Select one key without attempting a signature. Ambiguous IDs also fail closed.
pub(crate) fn select<'a>(
    keys: &'a [VerifyingKey],
    kid: Option<&str>,
    alg: Algorithm,
) -> Option<&'a VerifyingKey> {
    let mut matches = keys.iter().filter(|key| match kid {
        Some(kid) => key.kid.as_deref() == Some(kid),
        None => key.alg == alg,
    });
    let key = matches.next()?;
    if matches.next().is_some() || key.alg != alg {
        return None;
    }
    Some(key)
}

/// Filter one JWK before decoding it. Only supported signing keys are accepted.
/// The warning includes at most 32 UTF-8 bytes of its ID and no other JWK values.
pub(crate) fn from_jwk(value: &serde_json::Value, config: &AuthConfig) -> Option<VerifyingKey> {
    let kid = value.get("kid").and_then(serde_json::Value::as_str);
    let parse = || {
        if value.get("kid").is_some() && kid.is_none()
            || kid.is_some_and(|kid| kid.len() > MAX_KID_BYTES)
        {
            return None;
        }
        if value
            .get("use")
            .is_some_and(|value| value.as_str() != Some("sig"))
        {
            return None;
        }
        let alg = algorithm(value.get("alg")?.as_str()?)?;
        let kty = value.get("kty")?.as_str()?;
        let curve = value.get("crv").and_then(serde_json::Value::as_str);
        let supported = match alg {
            Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => kty == "RSA",
            Algorithm::ES256 => kty == "EC" && curve == Some("P-256"),
            Algorithm::ES384 => kty == "EC" && curve == Some("P-384"),
            Algorithm::EdDSA => kty == "OKP" && curve == Some("Ed25519"),
            _ => false,
        };
        if !supported {
            return None;
        }
        let jwk: Jwk = serde_json::from_value(value.clone()).ok()?;
        let key = DecodingKey::from_jwk(&jwk).ok()?;
        (jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.verifier_factory)(&alg, &key).ok()?;
        Some(VerifyingKey {
            kid: kid.map(str::to_owned),
            alg,
            key,
            validation: validation(alg, config),
        })
    };
    let result = parse();
    if result.is_none() {
        let kid = kid.unwrap_or("");
        let end = (0..=kid.len().min(32))
            .rev()
            .find(|index| kid.is_char_boundary(*index))
            .unwrap_or(0);
        tracing::warn!(kid = &kid[..end], "skipped unusable JWKS key");
    }
    result
}

#[cfg(test)]
mod tests;
