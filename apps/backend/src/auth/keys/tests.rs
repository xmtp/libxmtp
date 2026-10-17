use super::*;
use crate::test_support::auth::{TestKey, mint, valid_claims};
use jsonwebtoken::decode;
use serde_json::json;

#[xmtp_common::test(unwrap_try = true)]
fn absent_inline_signing_keys_load_an_empty_set() {
    let (_, config) = crate::test_support::auth::api_key("operator");
    assert!(config.keys.is_none());
    assert!(inline(&config)?.is_empty());
}

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

// verifies: AUTH-016
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

// verifies: AUTH-027
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

/// Each configured algorithm maps to the `jsonwebtoken` algorithm of the same
/// name and back, for every name the schema allows.
#[xmtp_common::test(unwrap_try = true)]
fn configured_algorithms_map_to_the_jwt_algorithms_of_the_same_name() {
    use std::str::FromStr;
    let schema = serde_json::to_value(crate::config::Config::schema())?;
    let names = schema["$defs"]["AuthKeyConfig"]["properties"]["alg"]["enum"].as_array()?;
    assert_eq!(names.len(), 6);
    for name in names {
        let name = name.as_str()?;
        let configured = SigningAlgorithm::from_name(name)?;
        assert_eq!(jwt_algorithm(configured), Algorithm::from_str(name)?);
        assert_eq!(algorithm(name), Some(Algorithm::from_str(name)?));
        assert_eq!(
            signing_algorithm(jwt_algorithm(configured)),
            Some(configured)
        );
    }
    for name in ["none", "HS256", "PS256"] {
        assert!(algorithm(name).is_none(), "{name}");
    }
    assert_eq!(signing_algorithm(Algorithm::HS256), None);
}

/// Each inline key parses as SubjectPublicKeyInfo for its exact algorithm. A
/// key of another family, or text that is not a key, is refused by its entry
/// without its id or value.
#[xmtp_common::test(unwrap_try = true)]
fn inline_keys_are_parsed_as_spki_for_the_exact_algorithm() {
    for key in [
        TestKey::es256(),
        TestKey::es384(),
        TestKey::eddsa(),
        TestKey::rsa(),
    ] {
        inline(&key.auth_config())?;
    }
    let key = TestKey::es256();
    for alg in ["ES384", "EdDSA", "RS256"] {
        let mut config = key.auth_config();
        config.keys.as_mut()?[0].alg = alg.into();
        let error = inline(&config).err()?.to_string();
        assert!(error.contains("auth.keys[0]"), "{error}");
        assert!(
            error.contains("public_key must be SubjectPublicKeyInfo for alg"),
            "{error}"
        );
        assert!(!error.contains(&key.kid));
        assert!(!error.contains(&key.public_key));
    }
    let mut config = key.auth_config();
    config.keys.as_mut()?[0].public_key = "private-key-sentinel".into();
    let error = inline(&config).err()?;
    for message in [error.to_string(), format!("{error:?}")] {
        assert!(message.contains("auth.keys[0]"), "{message}");
        assert!(!message.contains(&key.kid));
        assert!(!message.contains("private-key-sentinel"));
    }
}

/// Of three inline keys, the second is an RSA key under ES256. Startup refuses
/// that entry by position while it builds authentication, before storage.
// verifies: AUTH-017
#[xmtp_common::test(unwrap_try = true)]
async fn startup_refuses_the_first_inline_key_that_does_not_parse() {
    let rsa = TestKey::rsa();
    let mut mismatched = rsa.config();
    mismatched.alg = "ES256".into();
    let auth = AuthConfig {
        enabled: Some(true),
        keys: Some(vec![
            TestKey::es256().config(),
            mismatched,
            TestKey::eddsa().config(),
        ]),
        ..AuthConfig::default()
    };
    let error = inline(&auth).err()?;
    assert!(
        matches!(&error, ConfigError::Auth { field, reason }
            if field == "auth.keys[1]" && *reason == "public_key must be SubjectPublicKeyInfo for alg"),
        "{error}"
    );
    for message in [error.to_string(), format!("{error:?}")] {
        assert!(!message.contains(&rsa.kid), "{message}");
        assert!(!message.contains(&rsa.public_key), "{message}");
    }
    // Nothing listens on the database port. A refusal before storage returns
    // at once; one after it would wait on the connection.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let mut config: crate::config::Config = toml::from_str(&format!(
        "[database]\nurl = 'postgres://xmtp:xmtp@{closed}/xmtp'\n[server]\nidentifier = 'org.xmtp.test'"
    ))?;
    config.auth = Some(auth);
    let error = xmtp_common::time::timeout(
        std::time::Duration::from_secs(10),
        crate::server::initialize(config.validate()?),
    )
    .await
    .expect("initialize refuses before storage")
    .err()?;
    assert!(
        matches!(error.downcast_ref::<ConfigError>(), Some(ConfigError::Auth { field, .. }) if field == "auth.keys[1]"),
        "{error}"
    );
}
