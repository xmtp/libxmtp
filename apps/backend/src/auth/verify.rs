//! One signature check per request. Verification never performs network IO.
use super::keys::{KeySet, select};
use crate::config::auth::{AuthConfig, MAX_KID_BYTES, algorithm};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{decode, decode_header, errors::ErrorKind};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

pub(crate) const MAX_TOKEN_BYTES: usize = 8 * 1024;
pub(crate) const MISSING: &str = "missing bearer token";
pub(crate) const MALFORMED_BEARER: &str = "authorization header is not a bearer token";
pub(crate) const UNSUPPORTED: &str = "token is not a supported JWT";
pub(crate) const UNTRUSTED: &str = "token signature is not trusted";
pub(crate) const EXPIRED: &str = "token has expired";
pub(crate) const NOT_YET_VALID: &str = "token is not yet valid";
pub(crate) const AUDIENCE: &str = "token audience is not allowed";
pub(crate) const ISSUER: &str = "token issuer is not allowed";
pub(crate) const SCOPE: &str = "token is missing a required scope";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthContext {
    pub sub: Option<String>,
    pub scopes: BTreeSet<String>,
}
#[derive(Deserialize)]
pub(crate) struct Claims {
    pub sub: Option<String>,
    #[serde(default, deserialize_with = "scope_claim")]
    pub scope: Option<ScopeClaim>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ScopeClaim {
    String(String),
    Array(Vec<String>),
}
fn scope_claim<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<ScopeClaim>, D::Error> {
    ScopeClaim::deserialize(deserializer).map(Some)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rejection {
    Missing,
    Bearer,
    Malformed,
    UnsupportedAlg,
    Untrusted,
    Expired,
    NotYetValid,
    Audience,
    Issuer,
    Scope,
}
impl Rejection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Bearer | Self::Malformed => "malformed",
            Self::UnsupportedAlg => "unsupported_alg",
            Self::Untrusted => "untrusted",
            Self::Expired => "expired",
            Self::NotYetValid => "not_yet_valid",
            Self::Audience => "audience",
            Self::Issuer => "issuer",
            Self::Scope => "scope",
        }
    }
    pub(crate) fn status(self) -> tonic::Status {
        let message = match self {
            Self::Missing => MISSING,
            Self::Bearer => MALFORMED_BEARER,
            Self::Malformed | Self::UnsupportedAlg => UNSUPPORTED,
            Self::Untrusted => UNTRUSTED,
            Self::Expired => EXPIRED,
            Self::NotYetValid => NOT_YET_VALID,
            Self::Audience => AUDIENCE,
            Self::Issuer => ISSUER,
            Self::Scope => SCOPE,
        };
        if self == Self::Scope {
            tonic::Status::permission_denied(message)
        } else {
            tonic::Status::unauthenticated(message)
        }
    }
}

pub(crate) struct Verifier {
    pub keys: Arc<KeySet>,
    config: AuthConfig,
}
impl Verifier {
    pub fn new(keys: Arc<KeySet>, config: AuthConfig) -> Self {
        Self { keys, config }
    }

    /// Check the bearer token and select exactly one trusted signing key.
    /// Claims are checked only after the signature succeeds. No token data is logged.
    pub fn verify(&self, headers: &http::HeaderMap) -> Result<AuthContext, Rejection> {
        let header = headers
            .get(http::header::AUTHORIZATION)
            .ok_or(Rejection::Missing)?;
        let header = header.to_str().map_err(|_| Rejection::Bearer)?;
        let (scheme, token) = header.split_once(' ').ok_or(Rejection::Bearer)?;
        let token = token.trim();
        if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
            return Err(Rejection::Bearer);
        }
        if token.len() > MAX_TOKEN_BYTES {
            return Err(Rejection::Malformed);
        }
        let encoded_header = token.split('.').next().ok_or(Rejection::Malformed)?;
        let header_bytes = URL_SAFE_NO_PAD
            .decode(encoded_header)
            .map_err(|_| Rejection::Malformed)?;
        let raw_header: Value =
            serde_json::from_slice(&header_bytes).map_err(|_| Rejection::Malformed)?;
        let alg = raw_header
            .get("alg")
            .and_then(Value::as_str)
            .ok_or(Rejection::Malformed)?;
        if algorithm(alg).is_none() {
            return Err(Rejection::UnsupportedAlg);
        }
        let header = decode_header(token).map_err(|_| Rejection::Malformed)?;
        if header
            .kid
            .as_ref()
            .is_some_and(|kid| kid.len() > MAX_KID_BYTES)
        {
            return Err(Rejection::Malformed);
        }
        // Decode syntax before key lookup. This value is untrusted until decode verifies it.
        let claims = jsonwebtoken::dangerous::insecure_decode::<Value>(token)
            .map_err(|_| Rejection::Malformed)?
            .claims;
        if !claims.is_object() {
            return Err(Rejection::Malformed);
        }
        let keys = self.keys.0.load();
        if header.kid.is_some() && !keys.iter().any(|key| key.alg == header.alg) {
            return Err(Rejection::UnsupportedAlg);
        }
        let key = select(&keys, header.kid.as_deref(), header.alg).ok_or(Rejection::Untrusted)?;
        match decode::<Value>(token, &key.key, &key.validation) {
            Ok(_) => {}
            // These errors are produced only after signature verification. Check them
            // below in a stable order; the library uses an unordered required-claim set.
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::MissingRequiredClaim(_)
                        | ErrorKind::InvalidClaimFormat(_)
                        | ErrorKind::ExpiredSignature
                        | ErrorKind::ImmatureSignature
                        | ErrorKind::InvalidAudience
                        | ErrorKind::InvalidIssuer
                ) => {}
            Err(error) => {
                return Err(match error.kind() {
                    ErrorKind::Base64(_)
                    | ErrorKind::Json(_)
                    | ErrorKind::InvalidToken
                    | ErrorKind::Utf8(_) => Rejection::Malformed,
                    _ => Rejection::Untrusted,
                });
            }
        }
        self.check_claims(claims)
    }

    /// Enforce strict JSON claim types and the documented error order.
    /// This must only receive claims whose signature has been verified.
    fn check_claims(&self, value: Value) -> Result<AuthContext, Rejection> {
        let now = xmtp_common::time::now_secs() as u64;
        let exp = value
            .get("exp")
            .and_then(numeric_date)
            .ok_or(Rejection::Expired)?;
        if exp < now.saturating_sub(self.config.leeway_seconds) {
            return Err(Rejection::Expired);
        }
        if let Some(nbf) = value.get("nbf") {
            let nbf = numeric_date(nbf).ok_or(Rejection::Malformed)?;
            if nbf > now.saturating_add(self.config.leeway_seconds) {
                return Err(Rejection::NotYetValid);
            }
        }
        if let Some(allowed) = &self.config.audiences {
            let audience = match value.get("aud") {
                Some(Value::String(aud)) => vec![aud.as_str()],
                Some(Value::Array(aud)) => aud
                    .iter()
                    .map(Value::as_str)
                    .collect::<Option<Vec<_>>>()
                    .ok_or(Rejection::Audience)?,
                _ => return Err(Rejection::Audience),
            };
            if !audience
                .iter()
                .any(|aud| allowed.iter().any(|allowed| allowed == aud))
            {
                return Err(Rejection::Audience);
            }
        }
        if let Some(allowed) = &self.config.issuers {
            let issuer = value
                .get("iss")
                .and_then(Value::as_str)
                .ok_or(Rejection::Issuer)?;
            if !allowed.iter().any(|allowed| allowed == issuer) {
                return Err(Rejection::Issuer);
            }
        }
        let claims: Claims = serde_json::from_value(value).map_err(|_| Rejection::Malformed)?;
        let scopes: BTreeSet<String> = match claims.scope {
            Some(ScopeClaim::String(scopes)) => scopes
                .split(' ')
                .filter(|scope| !scope.is_empty())
                .map(str::to_owned)
                .collect(),
            Some(ScopeClaim::Array(scopes)) => scopes.into_iter().collect(),
            None => BTreeSet::new(),
        };
        if self
            .config
            .required_scopes
            .iter()
            .any(|scope| !scopes.contains(scope))
        {
            return Err(Rejection::Scope);
        }
        Ok(AuthContext {
            sub: claims.sub,
            scopes,
        })
    }
}

/// Match the JWT library's NumericDate representation, including fractional seconds.
/// Strings and numbers outside the unsigned timestamp range are malformed.
fn numeric_date(value: &Value) -> Option<u64> {
    if let Some(seconds) = value.as_u64() {
        return Some(seconds);
    }
    let seconds = value.as_f64()?;
    (seconds.is_finite() && seconds >= 0.0 && seconds < u64::MAX as f64)
        .then(|| seconds.round() as u64)
}

#[cfg(test)]
mod tests;
