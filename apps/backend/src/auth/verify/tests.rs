use super::*;
use crate::{
    auth::keys,
    test_support::auth::{TestKey, api_key, api_key_value, mint, mint_with_header, valid_claims},
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
fn configured_api_key_returns_its_name_for_every_bearer_case() {
    let (value, config) = api_key("operator");
    let verifier = verifier(config);
    for scheme in ["bearer", "BEARER", "Bearer"] {
        let mut headers = headers(&value);
        headers.insert("authorization", format!("{scheme} {value}").parse()?);
        assert_eq!(
            verifier.verify(&headers)?,
            AuthContext::ApiKey {
                name: "operator".into(),
            }
        );
    }
}

// verifies: AUTH-007
#[xmtp_common::test(unwrap_try = true)]
fn api_key_miss_without_jwt_source_is_untrusted() {
    let (_, config) = api_key("operator");
    let verifier = verifier(config);
    for token in [api_key_value(), mint(&valid_claims(), &TestKey::es256())] {
        assert_eq!(verifier.verify(&headers(&token)), Err(Rejection::Untrusted));
    }
}

// verifies: AUTH-007
#[xmtp_common::test(unwrap_try = true)]
fn api_key_miss_with_jwt_source_preserves_jwt_verification() {
    let (_, mut config) = api_key("operator");
    let key = TestKey::es256();
    config.keys = Some(vec![key.config()]);
    let verifier = verifier(config);
    assert!(matches!(
        verifier.verify(&headers(&mint(&valid_claims(), &key)))?,
        AuthContext::Jwt { .. }
    ));
    assert_eq!(
        verifier.verify(&headers("not.a.jwt")),
        Err(Rejection::Malformed)
    );
}

// verifies: AUTH-006
#[xmtp_common::test(unwrap_try = true)]
fn configured_jwt_shaped_value_is_admitted_as_an_api_key() {
    let key = TestKey::es256();
    let value = mint(&valid_claims(), &key);
    let mut config = key.auth_config();
    config.api_keys.insert("jwt-shaped".into(), value.clone());
    assert_eq!(
        verifier(config).verify(&headers(&value))?,
        AuthContext::ApiKey {
            name: "jwt-shaped".into(),
        }
    );
}

// verifies: AUTH-006
#[xmtp_common::test(unwrap_try = true)]
fn api_key_admission_does_not_apply_jwt_claim_checks() {
    let (value, mut config) = api_key("operator");
    config.required_scopes = vec!["required".into()];
    config.audiences = Some(vec!["backend".into()]);
    config.issuers = Some(vec!["issuer".into()]);
    assert_eq!(
        verifier(config).verify(&headers(&value))?,
        AuthContext::ApiKey {
            name: "operator".into(),
        }
    );
}

// verifies: AUTH-005
#[xmtp_common::test(unwrap_try = true)]
fn oversized_bearer_is_malformed_even_when_it_matches_a_configured_key() {
    let (_, mut config) = api_key("operator");
    let value = "a".repeat(MAX_TOKEN_BYTES + 1);
    config.api_keys.insert("operator".into(), value.clone());
    assert_eq!(
        verifier(config).verify(&headers(&value)),
        Err(Rejection::Malformed)
    );
}

// verifies: AUTH-006
#[xmtp_common::test(unwrap_try = true)]
fn each_configured_api_key_returns_only_its_own_name() {
    let (last_value, mut config) = api_key("zulu");
    let first_value = api_key_value();
    config.api_keys.insert("alpha".into(), first_value.clone());
    let verifier = verifier(config);
    for (name, value) in [("alpha", first_value), ("zulu", last_value)] {
        assert_eq!(
            verifier.verify(&headers(&value))?,
            AuthContext::ApiKey { name: name.into() }
        );
    }
}

// verifies: AUTH-005
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

// verifies: AUTH-008
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

// verifies: AUTH-008
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
        Err(Rejection::Untrusted)
    );
}

// verifies: AUTH-009, AUTH-010
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

// verifies: AUTH-011, AUTH-012
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
        (
            json!({"exp": "sentinel", "aud": "backend", "iss": "issuer"}),
            Rejection::Malformed,
        ),
        (
            json!({"exp": null, "aud": "backend", "iss": "issuer"}),
            Rejection::Malformed,
        ),
        // Stay well clear of the 60 s leeway. A one-second margin flips when
        // minting and verification land in different seconds.
        (json!({"exp": now - 3600}), Rejection::Expired),
        (
            json!({"exp": now + 7200, "nbf": now + 3600}),
            Rejection::NotYetValid,
        ),
        (json!({"exp": now + 3600}), Rejection::Audience),
        (
            json!({"exp": now + 3600, "aud": 1, "iss": "issuer"}),
            Rejection::Malformed,
        ),
        (
            json!({"exp": now + 3600, "aud": ["backend", 1], "iss": "issuer"}),
            Rejection::Malformed,
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
            Rejection::Malformed,
        ),
        (
            json!({"exp": now + 3600, "aud": "backend", "iss": ["issuer"]}),
            Rejection::Malformed,
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

// verifies: AUTH-013
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
        assert_eq!(
            context,
            AuthContext::Jwt {
                sub: Some("caller".into()),
                iss: None,
                scopes: BTreeSet::from(["xmtp".into(), "publish".into()]),
            }
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

/// Two issuers can issue the same subject, so the context keeps the verified
/// `iss` beside `sub` for the rate-limit caller key.
#[xmtp_common::test(unwrap_try = true)]
fn jwt_context_keeps_the_verified_issuer() {
    let key = TestKey::es256();
    let verifier = verifier(key.auth_config());
    for issuer in [Some("https://issuer.example"), None] {
        let mut claims = valid_claims();
        claims["sub"] = json!("caller");
        if let Some(issuer) = issuer {
            claims["iss"] = json!(issuer);
        }
        assert_eq!(
            verifier.verify(&headers(&mint(&claims, &key)))?,
            AuthContext::Jwt {
                sub: Some("caller".into()),
                iss: issuer.map(str::to_owned),
                scopes: BTreeSet::new(),
            }
        );
    }
}

// verifies: AUTH-009
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
        (&ed, Some(first.kid.clone()), Err(Rejection::Untrusted), 0),
        (&ed, Some("unknown".into()), Err(Rejection::Untrusted), 0),
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

// verifies: AUTH-009
#[xmtp_common::test(unwrap_try = true)]
fn selected_key_algorithm_mismatch_is_untrusted() {
    let ec = TestKey::es256();
    let ed = TestKey::eddsa();
    for loaded in [vec![ec.config()], vec![ec.config(), ed.config()]] {
        let verifier = verifier(AuthConfig {
            keys: Some(loaded),
            ..AuthConfig::default()
        });
        for kid in [ec.kid.clone(), "unknown".into()] {
            let mut header = Header::new(ed.alg);
            header.kid = Some(kid);
            let token = mint_with_header(&valid_claims(), &ed, header);
            assert_eq!(verifier.verify(&headers(&token)), Err(Rejection::Untrusted));
        }
    }
}

// verifies: AUTH-031
#[xmtp_common::test(unwrap_try = true)]
fn present_claim_types_are_checked_with_and_without_allowlists() {
    let key = TestKey::es256();
    for allowlists in [false, true] {
        let mut config = key.auth_config();
        if allowlists {
            config.audiences = Some(vec!["backend".into()]);
            config.issuers = Some(vec!["issuer".into()]);
        }
        let verifier = verifier(config);
        let base = json!({"exp": xmtp_common::time::now_secs() + 3600, "nbf": 0, "aud": "backend", "iss": "issuer", "sub": "caller", "scope": "xmtp"});
        for name in ["exp", "nbf", "aud", "iss", "sub", "scope"] {
            let mut invalid = vec![json!(null), json!(true), json!({}), json!(["valid", 1])];
            if matches!(name, "exp" | "nbf") {
                invalid.extend([json!("1"), json!(-1), json!(-0.1), json!([])]);
            } else {
                invalid.push(json!(123));
            }
            if matches!(name, "iss" | "sub") {
                invalid.push(json!(["caller"]));
            }
            for value in invalid {
                let mut claims = base.clone();
                claims[name] = value;
                assert_eq!(
                    verifier.verify(&headers(&mint(&claims, &key))),
                    Err(Rejection::Malformed),
                    "allowlists={allowlists}, {name}: {claims}"
                );
            }
        }
        for audience in [json!("backend"), json!(["other", "backend"])] {
            for scope in [json!("xmtp"), json!(["xmtp", "publish"])] {
                let mut claims = base.clone();
                claims["aud"] = audience.clone();
                claims["scope"] = scope;
                claims["exp"] = json!(xmtp_common::time::now_secs() as f64 + 3600.5);
                claims["nbf"] = json!(0.5);
                assert!(verifier.verify(&headers(&mint(&claims, &key))).is_ok());
            }
        }
        let mut huge = base.clone();
        huge["exp"] = json!(1e100);
        assert!(verifier.verify(&headers(&mint(&huge, &key))).is_ok());
        huge["nbf"] = json!(1e100);
        assert_eq!(
            verifier.verify(&headers(&mint(&huge, &key))),
            Err(Rejection::NotYetValid)
        );
    }
}

// verifies: AUTH-031
#[xmtp_common::test(unwrap_try = true)]
fn numeric_dates_round_fractional_seconds_and_accept_finite_nonnegative_numbers() {
    for (value, expected) in [
        (json!(0), 0),
        (json!(0.49), 0),
        (json!(0.5), 1),
        (json!(1.49), 1),
        (json!(1.5), 2),
        (json!(1e100), u64::MAX),
    ] {
        assert_eq!(numeric_date(&value), Some(expected));
    }
    for value in [json!(-1), json!(-0.1), json!(null), json!("1")] {
        assert_eq!(numeric_date(&value), None);
    }
}

// verifies: AUTH-014, AUTH-031
#[xmtp_common::test(unwrap_try = true)]
fn mixed_claim_errors_follow_table_order_with_and_without_allowlists() {
    let key = TestKey::es256();
    let now = xmtp_common::time::now_secs();
    let base = json!({"exp": now + 3600, "nbf": 0, "aud": "backend", "iss": "issuer", "sub": "caller", "scope": "required"});
    for audiences in [false, true] {
        for issuers in [true, false] {
            let mut config = key.auth_config();
            config.required_scopes = vec!["required".into()];
            if audiences {
                config.audiences = Some(vec!["backend".into()]);
            }
            if issuers {
                config.issuers = Some(vec!["issuer".into()]);
            }
            let verifier = verifier(config);
            let issuer_before_type = if issuers {
                Rejection::Issuer
            } else {
                Rejection::Malformed
            };
            let audience_before_type = if audiences {
                Rejection::Audience
            } else {
                Rejection::Malformed
            };
            let allowlist_before_type = if audiences {
                Rejection::Audience
            } else {
                issuer_before_type
            };
            let cases = [
                (json!({"aud": 1, "iss": "other"}), issuer_before_type),
                (
                    json!({"exp": now - 3600, "nbf": null, "aud": 1, "iss": false, "sub": null, "scope": null}),
                    Rejection::Expired,
                ),
                (
                    json!({"nbf": now + 3600, "aud": 1, "iss": false, "sub": null, "scope": null}),
                    Rejection::NotYetValid,
                ),
                (
                    json!({"exp": null, "nbf": now + 3600}),
                    Rejection::NotYetValid,
                ),
                (
                    json!({"exp": "invalid", "nbf": null, "aud": "other", "iss": "other", "sub": false}),
                    allowlist_before_type,
                ),
                (
                    json!({"nbf": -1, "aud": "other", "iss": "other"}),
                    allowlist_before_type,
                ),
                (json!({"aud": "other", "iss": false}), audience_before_type),
                (json!({"aud": 1, "iss": false}), Rejection::Malformed),
                (json!({"iss": "other", "scope": null}), issuer_before_type),
                (json!({"sub": null, "scope": "other"}), Rejection::Malformed),
                (json!({"scope": "other"}), Rejection::Scope),
            ];
            for (changes, expected) in cases {
                let mut claims = base.clone();
                for (name, value) in changes.as_object().unwrap() {
                    claims[name] = value.clone();
                }
                assert_eq!(
                    verifier.verify(&headers(&mint(&claims, &key))),
                    Err(expected),
                    "audiences={audiences}, issuers={issuers}, {claims}"
                );
            }
            let mut no_exp = base.clone();
            no_exp.as_object_mut().unwrap().remove("exp");
            no_exp["nbf"] = json!(now + 3600);
            no_exp["aud"] = json!(1);
            assert_eq!(
                verifier.verify(&headers(&mint(&no_exp, &key))),
                Err(Rejection::Expired)
            );
            for invalid_date in [
                json!(null),
                json!("1"),
                json!(-1),
                json!(-0.1),
                json!(true),
                json!([]),
            ] {
                let mut claims = base.clone();
                claims["exp"] = invalid_date.clone();
                assert_eq!(
                    verifier.verify(&headers(&mint(&claims, &key))),
                    Err(Rejection::Malformed),
                    "{claims}"
                );
                claims["exp"] = base["exp"].clone();
                claims["nbf"] = invalid_date;
                assert_eq!(
                    verifier.verify(&headers(&mint(&claims, &key))),
                    Err(Rejection::Malformed),
                    "{claims}"
                );
            }
            for missing in ["aud", "iss"] {
                let mut claims = base.clone();
                claims.as_object_mut().unwrap().remove(missing);
                claims["exp"] = json!(null);
                let expected = if missing == "aud" && audiences {
                    Rejection::Audience
                } else if missing == "iss" && issuers {
                    Rejection::Issuer
                } else {
                    Rejection::Malformed
                };
                assert_eq!(
                    verifier.verify(&headers(&mint(&claims, &key))),
                    Err(expected),
                    "{claims}"
                );
            }
        }
    }
}
