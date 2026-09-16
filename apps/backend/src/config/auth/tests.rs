use super::*;
use crate::{config::Config, test_support::auth::TestKey};

fn loaded(auth: &str) -> Result<Config, ConfigError> {
    let path = std::env::temp_dir().join(format!("auth-config-{}.toml", uuid::Uuid::new_v4()));
    std::fs::write(
        &path,
        format!("[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n{auth}"),
    )
    .unwrap();
    let result = Config::load(&path);
    std::fs::remove_file(path).unwrap();
    result
}

#[xmtp_common::test(unwrap_try = true)]
fn absent_auth_and_timing_defaults_preserve_opt_in_behavior() {
    assert!(loaded("")?.auth.is_none());
    let auth = loaded("[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'")?
        .auth
        .unwrap();
    assert_eq!(
        (
            auth.leeway_seconds,
            auth.jwks_refresh_seconds,
            auth.jwks_max_stale_seconds
        ),
        (60, 300, 3600)
    );
    for (field, value) in [
        ("leeway_seconds", 301),
        ("jwks_refresh_seconds", 0),
        ("jwks_max_stale_seconds", 310),
    ] {
        let error = loaded(&format!(
            "[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'\n{field} = {value}"
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains(field), "{error}");
    }
    loaded(
        "[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'\nleeway_seconds = 300\njwks_refresh_seconds = 1\njwks_max_stale_seconds = 12",
    )?;
}

#[xmtp_common::test(unwrap_try = true)]
fn key_sources_and_claim_lists_are_validated_at_load() {
    for auth in [
        "[auth]\nenabled = true",
        "[auth]\nenabled = true\nkeys = []",
        "[auth]\nenabled = true\nkeys = []\njwks_url = 'https://issuer.example/keys'",
    ] {
        assert!(loaded(auth).unwrap_err().to_string().contains("keys"));
    }
    // An empty list would make its claim required with nothing able to match,
    // so every token would fail against a config that looks valid.
    for (field, auth) in [
        (
            "auth.audiences",
            "[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'\naudiences = []",
        ),
        (
            "auth.issuers",
            "[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'\nissuers = []",
        ),
    ] {
        let error = loaded(auth).unwrap_err().to_string();
        assert!(error.contains(field), "{error}");
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn keys_are_parsed_as_spki_for_the_exact_algorithm_at_load() {
    for key in [
        TestKey::es256(),
        TestKey::es384(),
        TestKey::eddsa(),
        TestKey::rsa(),
    ] {
        key.auth_config().validate()?;
    }
    let key = TestKey::es256();
    for alg in [
        "none", "HS256", "HS384", "HS512", "PS256", "ES384", "EdDSA", "RS256",
    ] {
        let mut config = key.auth_config();
        config.keys.as_mut().unwrap()[0].alg = alg.into();
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("auth.keys[0]"));
        assert!(!error.contains(&key.kid));
        assert!(!error.contains(&key.public_key));
    }
    let mut key = key.config();
    key.public_key = "private-key-sentinel".into();
    let source = format!(
        "[auth]\nenabled = true\nkeys = [{}]",
        toml::Value::try_from(&key)?
    );
    let error = loaded(&source).unwrap_err().to_string();
    assert!(error.contains("auth.keys[0]"));
    assert!(!error.contains(&key.kid));
    assert!(!error.contains("private-key-sentinel"));
}

#[xmtp_common::test(unwrap_try = true)]
fn missing_duplicate_and_overlong_key_ids_fail_with_entry_context() {
    let key = TestKey::es256();
    for kid in [String::new(), "a".repeat(MAX_KID_BYTES + 1)] {
        let mut config = key.auth_config();
        config.keys.as_mut().unwrap()[0].kid = kid;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("auth.keys[0]")
        );
    }
    let mut config = key.auth_config();
    config.keys.as_mut().unwrap().push(key.config());
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("auth.keys[1]")
    );
    let error =
        loaded("[auth]\nenabled = true\nkeys = [{ alg = 'ES256', public_key = 'sentinel' }]")
            .unwrap_err()
            .to_string();
    assert!(error.contains("auth.keys[0]"));
    assert!(!error.contains("sentinel"));
}

#[xmtp_common::test(unwrap_try = true)]
fn jwks_transport_and_debug_do_not_disclose_the_url_or_key() {
    for url in [
        "http://issuer.example/secret",
        "ftp://localhost/secret",
        "file:///secret",
        "http://127.0.0.1.example/secret",
    ] {
        let error = loaded(&format!("[auth]\nenabled = true\njwks_url = '{url}'"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("auth.jwks_url"));
        assert!(!error.contains(url));
    }
    for url in [
        "https://issuer.example/secret",
        "http://127.0.0.1:1234/secret",
        "http://localhost:1234/secret",
        "http://[::1]:1234/secret",
    ] {
        let config = loaded(&format!("[auth]\nenabled = true\njwks_url = '{url}'"))?;
        assert!(!format!("{config:?}").contains("secret"));
    }
    let config = TestKey::es256().auth_config();
    let debug = format!("{config:?}");
    assert!(!debug.contains("PUBLIC KEY"));
    assert!(!debug.contains(&config.keys.as_ref().unwrap()[0].kid));
    assert!(debug.contains("key_count: 1"));
}

#[xmtp_common::test(unwrap_try = true)]
fn environment_values_resolve_in_both_auth_key_sources() {
    const CHILD: &str = "XMTP_AUTH_CONFIG_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let key = TestKey::es256();
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "config::auth::tests::environment_values_resolve_in_both_auth_key_sources",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("XMTP_AUTH_TEST_KEY", key.public_key)
            .env("XMTP_AUTH_TEST_URL", "https://issuer.example/keys")
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    let config = loaded(
        "[auth]\nenabled = true\nkeys = [{ kid = 'test', alg = 'ES256', public_key = 'env:XMTP_AUTH_TEST_KEY' }]",
    )?;
    assert_eq!(
        config.auth.unwrap().keys.unwrap()[0].public_key,
        std::env::var("XMTP_AUTH_TEST_KEY")?
    );
    let config = loaded("[auth]\nenabled = true\njwks_url = 'env:XMTP_AUTH_TEST_URL'")?;
    assert_eq!(
        config.auth.unwrap().jwks_url.unwrap(),
        std::env::var("XMTP_AUTH_TEST_URL")?
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn schema_describes_all_auth_fields_and_environment_references() {
    let schema = serde_json::to_value(Config::schema())?;
    let fields = schema["$defs"]["AuthConfig"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "jwks_url",
        "keys",
        "audiences",
        "issuers",
        "required_scopes",
        "leeway_seconds",
        "jwks_refresh_seconds",
        "jwks_max_stale_seconds",
    ] {
        assert!(fields.contains_key(field), "{field}");
    }
    assert!(fields["jwks_url"].to_string().contains("env:"));
    assert!(
        schema["$defs"]["AuthKeyConfig"]["properties"]["public_key"]
            .to_string()
            .contains("env:")
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn resolved_key_values_are_absent_from_validation_errors() {
    const CHILD: &str = "XMTP_AUTH_REDACTION_CHILD";
    const SENTINEL: &str = "SENTINEL-SECRET-abc123";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "config::auth::tests::resolved_key_values_are_absent_from_validation_errors",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("XMTP_KID_SECRET", SENTINEL)
            .output()?;
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        return;
    }
    for (alg, reason) in [
        ("BOGUS", "alg must be a supported asymmetric algorithm"),
        ("ES256", "public_key must be SubjectPublicKeyInfo for alg"),
    ] {
        let error = Config::load_str(&format!(
            "[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n[auth]\nenabled = true\nkeys = [{{ kid = 'env:XMTP_KID_SECRET', alg = '{alg}', public_key = 'env:XMTP_KID_SECRET' }}]"
        )).unwrap_err();
        for message in [error.to_string(), format!("{error:?}")] {
            assert!(!message.contains(SENTINEL), "{message}");
            assert!(message.contains("auth.keys[0]"), "{message}");
            assert!(message.contains(reason), "{message}");
        }
    }
}
