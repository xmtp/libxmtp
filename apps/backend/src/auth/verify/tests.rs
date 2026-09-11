use super::*;
use crate::{
    auth::keys,
    test_support::auth::{TestKey, mint, mint_with_header, valid_claims},
};
use jsonwebtoken::{Algorithm, Header};
use serde_json::json;

fn verifier(config: AuthConfig) -> Verifier {
    Verifier::new(
        Arc::new(KeySet::new(keys::inline(&config).unwrap())),
        config,
    )
}
fn headers(token: &str) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    headers
}

#[xmtp_common::test(unwrap_try = true)]
fn bearer_parsing_rejects_missing_malformed_and_oversized_tokens() {
    let key = TestKey::es256();
    let verifier = verifier(key.auth_config());
    assert_eq!(
        verifier.verify(&http::HeaderMap::new()),
        Err(Rejection::Missing)
    );
    for value in ["Basic secret", "Bearer", "Bearer ", "Bearer    ", "Basic "] {
        let mut headers = http::HeaderMap::new();
        headers.insert("authorization", value.parse()?);
        assert_eq!(verifier.verify(&headers), Err(Rejection::Bearer));
    }
    for token in ["not.a.jwt".to_owned(), "a".repeat(MAX_TOKEN_BYTES + 1)] {
        assert_eq!(verifier.verify(&headers(&token)), Err(Rejection::Malformed));
    }
    let token = mint(&valid_claims(), &key);
    for scheme in ["Bearer", "bearer", "BEARER", "bEaReR"] {
        let mut headers = headers(&token);
        headers.insert("authorization", format!("{scheme} {token}").parse()?);
        assert!(verifier.verify(&headers).is_ok());
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn all_supported_key_families_verify_with_independent_validations() {
    let mut rsa384 = TestKey::rsa();
    rsa384.alg = Algorithm::RS384;
    let mut rsa512 = TestKey::rsa();
    rsa512.alg = Algorithm::RS512;
    let keys = [
        TestKey::es256(),
        TestKey::es384(),
        TestKey::eddsa(),
        TestKey::rsa(),
        rsa384,
        rsa512,
    ];
    for key in &keys {
        let verifier = verifier(key.auth_config());
        assert!(
            verifier
                .verify(&headers(&mint(&valid_claims(), key)))
                .is_ok()
        );
    }
    let config = AuthConfig {
        keys: Some(keys[..3].iter().map(TestKey::config).collect()),
        ..AuthConfig::default()
    };
    let verifier = verifier(config);
    for key in &keys[..3] {
        assert!(
            verifier
                .verify(&headers(&mint(&valid_claims(), key)))
                .is_ok()
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn symmetric_unsigned_and_unconfigured_algorithms_are_never_trusted() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let key = TestKey::es256();
    let verifier = verifier(key.auth_config());
    for alg in [Algorithm::HS256, Algorithm::HS384, Algorithm::HS512] {
        let token = jsonwebtoken::encode(
            &Header::new(alg),
            &valid_claims(),
            &jsonwebtoken::EncodingKey::from_secret(b"sentinel"),
        )?;
        assert_eq!(
            verifier.verify(&headers(&token)),
            Err(Rejection::UnsupportedAlg)
        );
    }
    let token = format!(
        "{}.{}.",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&valid_claims())?)
    );
    assert_eq!(
        verifier.verify(&headers(&token)),
        Err(Rejection::UnsupportedAlg)
    );
    let ed = TestKey::eddsa();
    assert_eq!(
        verifier.verify(&headers(&mint(&valid_claims(), &ed))),
        Err(Rejection::UnsupportedAlg)
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn key_selection_requires_a_unique_matching_id_or_algorithm() {
    let key = TestKey::es256();
    let other = TestKey::es256();
    let mut config = key.auth_config();
    let single = verifier(config.clone());
    let token = mint_with_header(&valid_claims(), &key, Header::new(key.alg));
    assert!(single.verify(&headers(&token)).is_ok());
    config.keys.as_mut().unwrap().push(other.config());
    let multiple = verifier(config);
    assert_eq!(multiple.verify(&headers(&token)), Err(Rejection::Untrusted));
    assert!(
        multiple
            .verify(&headers(&mint(&valid_claims(), &key)))
            .is_ok()
    );
    assert_eq!(
        single.verify(&headers(&mint(&valid_claims(), &other))),
        Err(Rejection::Untrusted)
    );
    let mut header = Header::new(key.alg);
    header.kid = Some(key.kid.clone());
    let token = mint_with_header(&json!({"exp": 1}), &other, header);
    assert_eq!(single.verify(&headers(&token)), Err(Rejection::Untrusted));
    let mut header = Header::new(key.alg);
    header.kid = Some("a".repeat(MAX_KID_BYTES + 1));
    let token = mint_with_header(&valid_claims(), &key, header);
    assert_eq!(single.verify(&headers(&token)), Err(Rejection::Malformed));
    let ed = TestKey::eddsa();
    let token = mint_with_header(&valid_claims(), &ed, Header::new(ed.alg));
    assert_eq!(single.verify(&headers(&token)), Err(Rejection::Untrusted));
}

#[xmtp_common::test(unwrap_try = true)]
fn time_audience_and_issuer_errors_follow_the_documented_order() {
    let key = TestKey::es256();
    let mut config = key.auth_config();
    config.audiences = Some(vec!["backend".into()]);
    config.issuers = Some(vec!["issuer".into()]);
    let verifier = verifier(config);
    let now = xmtp_common::time::now_secs();
    let cases = [
        (json!({}), Rejection::Expired),
        (json!({"exp": "sentinel"}), Rejection::Expired),
        (json!({"exp": null}), Rejection::Expired),
        // Stay well clear of the 60 s leeway. A one-second margin flips when
        // minting and verification land in different seconds.
        (json!({"exp": now - 3600}), Rejection::Expired),
        (
            json!({"exp": now + 7200, "nbf": now + 3600}),
            Rejection::NotYetValid,
        ),
        (json!({"exp": now + 3600}), Rejection::Audience),
        (json!({"exp": now + 3600, "aud": 1}), Rejection::Audience),
        (
            json!({"exp": now + 3600, "aud": ["backend", 1]}),
            Rejection::Audience,
        ),
        (
            json!({"exp": now + 3600, "aud": "other"}),
            Rejection::Audience,
        ),
        (
            json!({"exp": now + 3600, "aud": "backend"}),
            Rejection::Issuer,
        ),
        (
            json!({"exp": now + 3600, "aud": "backend", "iss": 1}),
            Rejection::Issuer,
        ),
        (
            json!({"exp": now + 3600, "aud": "backend", "iss": ["issuer"]}),
            Rejection::Issuer,
        ),
        (
            json!({"exp": now + 3600, "aud": "backend", "iss": "other"}),
            Rejection::Issuer,
        ),
    ];
    for (claims, expected) in cases {
        assert_eq!(
            verifier.verify(&headers(&mint(&claims, &key))),
            Err(expected),
            "{claims}"
        );
    }
    let fractional = json!({"exp": now as f64 + 3600.5, "nbf": now as f64 + 30.25, "aud": "backend", "iss": "issuer"});
    assert!(verifier.verify(&headers(&mint(&fractional, &key))).is_ok());
    for audience in [json!("backend"), json!(["other", "backend"])] {
        let claims = json!({"exp": now - 30, "nbf": now + 30, "aud": audience, "iss": "issuer"});
        assert!(verifier.verify(&headers(&mint(&claims, &key))).is_ok());
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn scopes_accept_strings_or_string_arrays_and_require_every_configured_scope() {
    let key = TestKey::es256();
    let mut config = key.auth_config();
    config.required_scopes = vec!["xmtp".into(), "publish".into()];
    let verifier = verifier(config);
    for scope in [
        json!("  xmtp publish xmtp  "),
        json!(["xmtp", "publish", "xmtp"]),
    ] {
        let mut claims = valid_claims();
        claims["scope"] = scope;
        claims["sub"] = json!("caller");
        let context = verifier.verify(&headers(&mint(&claims, &key)))?;
        assert_eq!(context.sub.as_deref(), Some("caller"));
        assert_eq!(
            context.scopes,
            BTreeSet::from(["xmtp".into(), "publish".into()])
        );
    }
    // A token holding only one of the two required scopes is rejected.
    let mut claims = valid_claims();
    claims["scope"] = json!("xmtp");
    let token = headers(&mint(&claims, &key));
    assert_eq!(verifier.verify(&token), Err(Rejection::Scope));
    assert_eq!(
        verifier.verify(&headers(&mint(&valid_claims(), &key))),
        Err(Rejection::Scope)
    );
    for scope in [
        json!(null),
        json!(123),
        json!(true),
        json!({"xmtp": true}),
        json!(["xmtp", 1]),
    ] {
        let mut claims = valid_claims();
        claims["scope"] = scope;
        assert_eq!(
            verifier.verify(&headers(&mint(&claims, &key))),
            Err(Rejection::Malformed)
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn key_selection_constructs_at_most_one_signature_verifier() {
    use jsonwebtoken::crypto::{CryptoProvider, JwtVerifier, rust_crypto::DEFAULT_PROVIDER};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static VERIFICATIONS: AtomicUsize = AtomicUsize::new(0);
    const CHILD: &str = "XMTP_AUTH_SIGNATURE_COUNT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "auth::verify::tests::key_selection_constructs_at_most_one_signature_verifier",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    fn counted(
        algorithm: &Algorithm,
        key: &jsonwebtoken::DecodingKey,
    ) -> jsonwebtoken::errors::Result<Box<dyn JwtVerifier>> {
        VERIFICATIONS.fetch_add(1, Ordering::Relaxed);
        (DEFAULT_PROVIDER.verifier_factory)(algorithm, key)
    }
    let provider = CryptoProvider {
        verifier_factory: counted,
        ..DEFAULT_PROVIDER.clone()
    };
    Box::leak(Box::new(provider)).install_default().unwrap();
    let first = TestKey::es256();
    let second = TestKey::es256();
    let ed = TestKey::eddsa();
    let verifier = verifier(AuthConfig {
        keys: Some(vec![first.config(), second.config()]),
        ..AuthConfig::default()
    });
    for (key, kid, expected, signatures) in [
        (&first, Some(first.kid.clone()), Ok(()), 1),
        (
            &second,
            Some(first.kid.clone()),
            Err(Rejection::Untrusted),
            1,
        ),
        (&first, None, Err(Rejection::Untrusted), 0),
        (&ed, None, Err(Rejection::Untrusted), 0),
        (&first, Some("unknown".into()), Err(Rejection::Untrusted), 0),
    ] {
        let mut header = Header::new(key.alg);
        header.kid = kid;
        let token = mint_with_header(&valid_claims(), key, header);
        VERIFICATIONS.store(0, Ordering::Relaxed);
        assert_eq!(verifier.verify(&headers(&token)).map(|_| ()), expected);
        assert_eq!(VERIFICATIONS.load(Ordering::Relaxed), signatures);
    }
}
