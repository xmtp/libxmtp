use super::*;
use crate::{
    config::Config,
    test_support::auth::{TestKey, api_key, api_key_value},
};

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
fn api_keys_load_alone_or_alongside_one_jwt_source() {
    let (value, config) = api_key("Team.Bot");
    assert_eq!(value.len(), 43);
    for mut auth in [
        config,
        TestKey::es256().auth_config(),
        AuthConfig {
            enabled: Some(true),
            jwks_url: Some("https://issuer.example/keys".into()),
            ..AuthConfig::default()
        },
    ] {
        auth.api_keys.insert("Team.Bot".into(), value.clone());
        let config = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?)?;
        assert_eq!(config.auth.unwrap().api_keys["Team.Bot"], value);
    }
    for source in [
        "[auth]\nenabled = true",
        "[auth]\nenabled = true\n[auth.api_keys]",
    ] {
        let error = loaded(source).unwrap_err().to_string();
        assert!(error.contains("auth"), "{error}");
        assert!(error.contains("set api_keys, jwks_url, or keys"), "{error}");
    }
    let mut auth = TestKey::es256().auth_config();
    auth.jwks_url = Some("https://issuer.example/keys".into());
    auth.api_keys.insert("bot".into(), value.clone());
    let error = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?)
        .unwrap_err()
        .to_string();
    assert!(error.contains("auth.jwks_url/auth.keys"), "{error}");
    assert!(error.contains("set exactly one key source"), "{error}");
    auth.jwks_url = None;
    auth.keys = Some(Vec::new());
    let error = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?)
        .unwrap_err()
        .to_string();
    assert!(error.contains("auth.keys"), "{error}");
    assert!(error.contains("must not be empty"), "{error}");
}

#[xmtp_common::test(unwrap_try = true)]
fn api_key_names_and_values_are_validated_without_disclosing_values() {
    let value = api_key_value();
    for name in [
        "",
        "-bot",
        ".bot",
        &"a".repeat(65),
        "team bot",
        "team/bot",
        "ébot",
    ] {
        let auth = AuthConfig {
            enabled: Some(true),
            api_keys: [(name.to_owned(), value.clone())].into(),
            ..AuthConfig::default()
        };
        let error = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?).unwrap_err();
        assert!(
            matches!(&error, ConfigError::Auth { field, .. } if field == &format!("auth.api_keys.{name}"))
        );
        for message in [error.to_string(), format!("{error:?}")] {
            assert!(message.contains(name), "{message}");
            assert!(!message.contains(&value), "{message}");
        }
    }
    for value in [
        "a".repeat(MIN_API_KEY_BYTES - 1),
        "a".repeat(MAX_API_KEY_BYTES + 1),
        format!("{value} "),
        format!("{value}\t"),
        format!("{value}\n"),
        format!("{value}\x7f"),
        format!("{value}é"),
    ] {
        let auth = AuthConfig {
            enabled: Some(true),
            api_keys: [("test-bot".into(), value.clone())].into(),
            ..AuthConfig::default()
        };
        let error = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?).unwrap_err();
        assert!(
            matches!(&error, ConfigError::Auth { field, .. } if field == "auth.api_keys.test-bot")
        );
        for message in [error.to_string(), format!("{error:?}")] {
            assert!(message.contains("test-bot"), "{message}");
            assert!(!message.contains(&value), "{message}");
        }
    }
    for value in ["!".repeat(MIN_API_KEY_BYTES), "~".repeat(MAX_API_KEY_BYTES)] {
        let auth = AuthConfig {
            enabled: Some(true),
            api_keys: [(format!("9._-{}", "a".repeat(60)), value)].into(),
            ..AuthConfig::default()
        };
        loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?)?;
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn api_key_values_are_unique_and_key_count_is_bounded() {
    let (value, mut auth) = api_key("first");
    auth.api_keys.insert("second".into(), value.clone());
    let error = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?).unwrap_err();
    assert!(
        matches!(&error, ConfigError::Auth { field, .. } if field == "auth.api_keys.first/second")
    );
    for message in [error.to_string(), format!("{error:?}")] {
        assert!(message.contains("first"), "{message}");
        assert!(message.contains("second"), "{message}");
        assert!(!message.contains(&value), "{message}");
    }
    auth.api_keys = (0..MAX_API_KEYS)
        .map(|index| (format!("bot-{index}"), format!("{value}-{index}")))
        .collect();
    loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?)?;
    auth.api_keys.insert("overflow".into(), value);
    let error = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?).unwrap_err();
    assert!(
        matches!(&error, ConfigError::Auth { field, reason } if field == "auth.api_keys" && reason.contains("256"))
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn api_key_debug_reports_only_the_count() {
    let (value, auth) = api_key("debug-bot");
    let config = loaded(&toml::to_string(&BTreeMap::from([("auth", &auth)]))?)?;
    let debug = format!("{config:?}");
    assert!(debug.contains("api_key_count: 1"));
    assert!(!debug.contains("debug-bot"));
    assert!(!debug.contains(&value));
}

#[xmtp_common::test(unwrap_try = true)]
fn api_key_environment_values_resolve_and_missing_variables_fail() {
    const CHILD: &str = "XMTP_API_KEY_CONFIG_CHILD";
    const VARIABLE: &str = "XMTP_API_KEY_CONFIG_VALUE";
    const MISSING: &str = "XMTP_API_KEY_CONFIG_MISSING";
    if std::env::var_os(CHILD).is_none() {
        let output = crate::config::tests::child_test(
            module_path!(),
            "api_key_environment_values_resolve_and_missing_variables_fail",
        )
        .env(CHILD, "1")
        .env(VARIABLE, api_key_value())
        .env_remove(MISSING)
        .output()?;
        crate::config::tests::assert_one_test_passed(&output);
        return;
    }
    let value = std::env::var(VARIABLE)?;
    let config = loaded(&format!(
        "[auth]\nenabled = true\n[auth.api_keys]\nbot = 'env:{VARIABLE}'"
    ))?;
    assert_eq!(config.auth.unwrap().api_keys["bot"], value);
    let error = loaded(&format!(
        "[auth]\nenabled = true\n[auth.api_keys]\nbot = 'env:{VARIABLE}'\nmissing = 'env:{MISSING}'"
    ))
    .unwrap_err();
    for message in [error.to_string(), format!("{error:?}")] {
        assert!(message.contains(MISSING), "{message}");
        assert!(!message.contains(&value), "{message}");
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
        let output = crate::config::tests::child_test(
            module_path!(),
            "environment_values_resolve_in_both_auth_key_sources",
        )
        .env(CHILD, "1")
        .env("XMTP_AUTH_TEST_KEY", key.public_key)
        .env("XMTP_AUTH_TEST_URL", "https://issuer.example/keys")
        .output()?;
        crate::config::tests::assert_one_test_passed(&output);
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
        "api_keys",
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
        fields["api_keys"]["additionalProperties"]["anyOf"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value["pattern"]
                .as_str()
                .is_some_and(|pattern| pattern.starts_with("^env:")))
    );
    assert!(
        schema["$defs"]["AuthKeyConfig"]["properties"]["public_key"]
            .to_string()
            .contains("env:")
    );
}

// verifies: CONF-085
#[xmtp_common::test(unwrap_try = true)]
fn resolved_key_values_are_absent_from_validation_errors() {
    const CHILD: &str = "XMTP_AUTH_REDACTION_CHILD";
    const SENTINEL: &str = "SENTINEL-SECRET-abc123";
    if std::env::var_os(CHILD).is_none() {
        let output = crate::config::tests::child_test(
            module_path!(),
            "resolved_key_values_are_absent_from_validation_errors",
        )
        .env(CHILD, "1")
        .env("XMTP_KID_SECRET", SENTINEL)
        .output()?;
        crate::config::tests::assert_one_test_passed(&output);
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

    // Every secret key, from an environment reference, in a refused file.
    const REQUIRED: &str =
        "[server]\nidentifier = 'org.xmtp.test'\n[database]\nurl = 'postgres://localhost/xmtp'\n";
    let reference = "'env:XMTP_KID_SECRET'";
    let refused = [
        (
            "database.url",
            format!("[server]\nidentifier = 'org.xmtp.test'\n[database]\nurl = {reference}\n"),
        ),
        (
            "database.replica_url",
            format!("{REQUIRED}replica_url = {reference}\n"),
        ),
        (
            "telemetry.otlp_endpoint",
            format!("{REQUIRED}[telemetry]\notlp_endpoint = {reference}\n"),
        ),
        (
            "chains",
            format!("{REQUIRED}[chains]\n\"eip155:1\" = {reference}\n"),
        ),
        (
            "attachments.target.S3.endpoint",
            format!(
                "{REQUIRED}[attachments]\nbase_url = 'https://files.example.org'\n[attachments.target.S3]\nendpoint = {reference}\nregion = 'us-east-1'\nbucket = 'attachments'\n[attachments.target.S3.credentials]\nkind = 'web_identity'\n"
            ),
        ),
    ];
    for (field, document) in &refused {
        let error = Config::load_str(document).unwrap_err();
        assert!(
            matches!(&error, ConfigError::Invalid { field: named, .. } if named == field),
            "{field}: {error}"
        );
        for message in [error.to_string(), format!("{error:?}")] {
            assert!(!message.contains(SENTINEL), "{message}");
        }
    }
    // Every secret key, written in the file, in a configuration that loads.
    let config = Config::load_str(&format!(
        "[server]\nidentifier = 'org.xmtp.test'\n\
         [telemetry]\notlp_endpoint = 'https://otel.example.org/{SENTINEL}'\n\
         [database]\nurl = 'postgres://xmtp:{SENTINEL}@localhost/xmtp'\nreplica_url = 'postgres://xmtp:{SENTINEL}@localhost/xmtp'\n\
         [chains]\n\"eip155:1\" = 'https://rpc.example.org/{SENTINEL}'\n\
         [push.apns]\nkey = '{SENTINEL}'\nkey_id = 'ABC123DEFG'\nteam_id = 'DEF123GHIJ'\nbundle_id = 'org.example.app'\nenvironment = 'production'\n\
         [push.fcm]\nservice_account = '{SENTINEL}'\n\
         [attachments]\nbase_url = 'https://files.example.org'\n\
         [attachments.target.S3]\nendpoint = 'https://s3.example.org/{SENTINEL}'\nregion = 'us-east-1'\nbucket = 'attachments'\n\
         [attachments.target.S3.credentials]\nkind = 'static'\naccess_key_id = '{SENTINEL}'\nsecret_access_key = '{SENTINEL}'\nsession_token = '{SENTINEL}'\n"
    ))?;
    let debug = format!("{config:?}");
    assert!(!debug.contains(SENTINEL), "{debug}");
}
