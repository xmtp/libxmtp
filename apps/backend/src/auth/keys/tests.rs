use super::*;
use crate::test_support::auth::{TestKey, mint, valid_claims};
use jsonwebtoken::decode;
use serde_json::json;

#[xmtp_common::test(unwrap_try = true)]
fn every_key_requires_exp_and_each_configured_audience_and_issuer() {
    let es = TestKey::es256();
    let ed = TestKey::eddsa();
    let config = AuthConfig {
        keys: Some(vec![es.config(), ed.config()]),
        audiences: Some(vec!["backend".into()]),
        issuers: Some(vec!["issuer".into()]),
        ..AuthConfig::default()
    };
    let keys = inline(&config)?;
    for (key, signer) in keys.iter().zip([es, ed]) {
        assert_eq!(key.validation.algorithms, vec![signer.alg]);
        for claim in ["exp", "aud", "iss"] {
            assert!(key.validation.required_spec_claims.contains(claim));
        }
        let mut claims = valid_claims();
        claims["aud"] = json!("backend");
        claims["iss"] = json!("issuer");
        assert!(
            decode::<serde_json::Value>(mint(&claims, &signer), &key.key, &key.validation).is_ok()
        );
        for missing in ["exp", "aud", "iss"] {
            let mut claims = claims.clone();
            claims.as_object_mut().unwrap().remove(missing);
            assert!(
                decode::<serde_json::Value>(mint(&claims, &signer), &key.key, &key.validation)
                    .is_err()
            );
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn jwk_filter_rejects_bad_usage_algorithm_curve_and_key_material() {
    let config = AuthConfig::default();
    let key = TestKey::es256();
    assert!(from_jwk(&key.jwk, &config).is_some());
    for (field, value) in [
        ("alg", json!(null)),
        ("alg", json!("HS256")),
        ("kty", json!("oct")),
        ("crv", json!("P-384")),
        ("use", json!("enc")),
        ("kid", json!("a".repeat(MAX_KID_BYTES + 1))),
        ("x", json!("invalid")),
    ] {
        let mut jwk = key.jwk.clone();
        jwk[field] = value;
        assert!(from_jwk(&jwk, &config).is_none(), "{field}");
    }
    let mut jwk = key.jwk.clone();
    jwk.as_object_mut().unwrap().remove("alg");
    assert!(from_jwk(&jwk, &config).is_none());
    for key in [TestKey::es384(), TestKey::eddsa(), TestKey::rsa()] {
        assert!(from_jwk(&key.jwk, &config).is_some());
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn skipped_key_logs_only_a_utf8_safe_bounded_id() {
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Warn);
    let key = json!({"kid": "é".repeat(200), "alg": "private-alg-sentinel", "key": "private-key-sentinel"});
    tracing::dispatcher::with_default(&capture.dispatch(), || {
        assert!(from_jwk(&key, &AuthConfig::default()).is_none());
    });
    let logs = capture.output();
    assert!(!logs.contains("private-"));
    let event: serde_json::Value = serde_json::from_str(logs.trim())?;
    assert_eq!(event["kid"].as_str().unwrap().len(), 32);
}

#[xmtp_common::test(unwrap_try = true)]
fn key_count_tracks_loaded_and_replaced_snapshots() {
    let recorder = crate::telemetry::recorder_builder()?.build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        let key = TestKey::es256();
        let set = KeySet::new(inline(&key.auth_config()).unwrap());
        assert!(handle.render().contains("xmtp_auth_keys 1"));
        let config = AuthConfig {
            keys: Some(vec![key.config(), TestKey::eddsa().config()]),
            ..AuthConfig::default()
        };
        set.replace(inline(&config).unwrap());
        assert!(handle.render().contains("xmtp_auth_keys 2"));
    });
}
