use super::*;
use std::collections::BTreeSet;

mod example;

/// A command that runs one test of this test binary in a child process. The
/// test is named by `module_path!()` where it is defined and its function name.
pub(crate) fn child_test(module: &str, function: &str) -> std::process::Command {
    let (_, module) = module
        .split_once("::")
        .expect("a test module below the crate root");
    let mut command =
        std::process::Command::new(std::env::current_exe().expect("the test executable"));
    command.args(["--exact", &format!("{module}::{function}"), "--nocapture"]);
    command
}

/// Require that a child ran exactly one test and that the test passed. libtest
/// exits successfully when its filter matches no test.
pub(crate) fn assert_one_test_passed(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed;"),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn logging_defaults_and_overrides_are_typed() {
    let config: Config = toml::from_str(MINIMAL)?;
    assert!(matches!(config.server.log_level, LogLevel::Info));
    assert!(config.server.request_logger);
    for level in ["off", "error", "warn", "info", "debug", "trace"] {
        let config: Config = toml::from_str(&format!(
            "{MINIMAL}log_level = '{level}'\nrequest_logger = false"
        ))?;
        assert_eq!(serde_json::to_value(config.server.log_level)?, level);
        assert!(!config.server.request_logger);
    }
    assert!(toml::from_str::<Config>(&format!("{MINIMAL}log_level = 'verbose'")).is_err());
}
/// A configuration that starts. The `[server]` section comes last and is left
/// open, so a test appends its own server keys directly and any other section
/// with its own header.
const MINIMAL: &str =
    "[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n";
/// A fixed P-256 SubjectPublicKeyInfo. Loading does not parse keys; the
/// backend parses them when it builds authentication.
pub(crate) const ES256_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEt5TVksIMVkHivH+8X8T6Rt3CNAR4
Y4jKNSug4VqB9WEBD0qCol4jSgmE2PfkHlcIoahWI5HWPIS9+hyCDkm8Rg==
-----END PUBLIC KEY-----
";
/// The PKCS#8 private key of `ES256_PUBLIC_KEY`.
pub(crate) const ES256_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgL38+Kb0CcfB3yZi4
ckz/j8b8rFvzsU9Qg0esse3Y2CmhRANCAAS3lNWSwgxWQeK8f7xfxPpG3cI0BHhj
iMo1K6DhWoH1YQEPSoKiXiNKCYTY9+QeVwihqFYjkdY8hL36HIIOSbxG
-----END PRIVATE KEY-----
";

/// One inline key with the fixed ES256 public key.
pub(crate) fn es256_key(kid: &str) -> auth::AuthKeyConfig {
    auth::AuthKeyConfig {
        kid: kid.to_owned(),
        alg: "ES256".into(),
        public_key: ES256_PUBLIC_KEY.into(),
    }
}

/// A fresh printable API key value of at least 32 bytes.
pub(crate) fn api_key_value() -> String {
    xmtp_common::rand_hexstring()
}
const RESPONSE_TEST_ENVELOPE_BYTES: usize = 1_000_000;
const RESPONSE_TEST_REQUEST_BYTES: usize = 2_000_000;

// verifies: ATCH-073, CONF-065
#[xmtp_common::test(unwrap_try = true)]
fn invalid_attachment_credential_sources_name_the_key() {
    let source = format!(
        "{MINIMAL}\n[attachments]\nbase_url = 'https://example.com/attachments'\n\
         [attachments.target.S3]\nendpoint = 'https://s3.example.com'\n\
         region = 'us-east-1'\nbucket = 'attachments'\n\
         [attachments.target.S3.credentials]\n"
    );
    for (fields, expected_key) in [
        ("kind = 'unknown'", "attachments.target.S3.credentials.kind"),
        ("kind = 'profile'", "attachments.target.S3.credentials.name"),
        (
            "kind = 'profile'\nname = ''",
            "attachments.target.S3.credentials.name",
        ),
    ] {
        let error = Config::load_str(&format!("{source}{fields}\n")).unwrap_err();
        assert!(
            matches!(&error, ConfigError::Invalid { field, .. } if *field == expected_key),
            "{error}"
        );
        assert!(error.to_string().contains(expected_key));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn push_defaults_provider_fields_and_redaction() {
    let config = Config::load_str(MINIMAL)?;
    assert_eq!(config.push.recipient_ttl_seconds, 2_592_000);
    assert_eq!(config.push.max_attempts, 3);
    assert_eq!(config.limits.max_push_topics, 100_000);
    assert!(config.push.apns.is_none() && config.push.fcm.is_none() && config.push.http.is_none());
    let source = format!(
        "{MINIMAL}\n[push.apns]\nkey = 'private-apns-credential'\nkey_id = 'key'\nteam_id = 'team'\nbundle_id = 'bundle'\nenvironment = 'sandbox'\n[push.fcm]\nservice_account = 'private-fcm-credential'\n[push.http]\n"
    );
    let config = Config::load_str(&source)?;
    assert!(config.push.http.is_some());
    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("private-apns-credential"));
    assert!(!debug.contains("private-fcm-credential"));
    let source = source
        .replace("private-apns-credential", "env:CARGO_MANIFEST_DIR")
        .replace("private-fcm-credential", "env:CARGO_MANIFEST_DIR");
    let config = Config::load_str(&source)?;
    let expected = std::env::var("CARGO_MANIFEST_DIR")?;
    assert_eq!(
        config.push.apns.unwrap().key.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(
        config.push.fcm.unwrap().service_account.as_deref(),
        Some(expected.as_str())
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn push_invalid_values_name_the_configuration_key() {
    for (extra, field) in [
        (
            "[push]\nrecipient_ttl_seconds = -1",
            "push.recipient_ttl_seconds",
        ),
        ("[push]\nmax_attempts = -1", "push.max_attempts"),
        ("[push]\nmax_attempts = 4294967296", "push.max_attempts"),
        (
            "[push]\nrecipient_ttl_seconds = 86399",
            "push.recipient_ttl_seconds",
        ),
        ("[push]\nmax_attempts = 0", "push.max_attempts"),
        ("[push]\nmax_attempts = 11", "push.max_attempts"),
        ("[push.apns]", "push.apns.key"),
        ("[push.apns]\nkey = 'key'", "push.apns.key_id"),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'",
            "push.apns.team_id",
        ),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'\nteam_id = 'team'",
            "push.apns.bundle_id",
        ),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'\nteam_id = 'team'\nbundle_id = 'bundle'",
            "push.apns.environment",
        ),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'\nteam_id = 'team'\nbundle_id = 'bundle'\nenvironment = 'invalid'",
            "push.apns.environment",
        ),
        ("[push.fcm]", "push.fcm.service_account"),
        (
            "[push.fcm]\nservice_account = ''",
            "push.fcm.service_account",
        ),
        ("[limits]\nmax_push_topics = 0", "limits.max_push_topics"),
    ] {
        let error = Config::load_str(&format!("{MINIMAL}\n{extra}")).unwrap_err();
        assert!(
            matches!(error, ConfigError::Invalid { field: actual, .. } if actual == field),
            "{error}"
        );
    }
    for domain in [
        "*",
        "a.*.org",
        "a*.org",
        "example..org",
        "",
        ".example.org",
        "example.org.",
        "https://example.org",
        "example.org:443",
        "example.org/path",
        "-example.org",
        "example-.org",
    ] {
        let error = Config::load_str(&format!(
            "{MINIMAL}\n[push.http]\nallowed_domains = ['{domain}']"
        ))
        .unwrap_err();
        assert!(matches!(
            error,
            ConfigError::Invalid {
                field: "push.http.allowed_domains",
                ..
            }
        ));
    }
    Config::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = 86400\nmax_attempts = 10\n[push.http]\nallowed_domains = ['hooks.example.com', '*.EXAMPLE.org']"
    ))?;
}

#[xmtp_common::test(unwrap_try = true)]
fn push_expiry_is_checked_at_both_duration_and_clock_boundaries() {
    let config = Config::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = {MAX_RETENTION_SECONDS}"
    ))?;
    assert_eq!(
        config.push.expires_at(0)?,
        MAX_RETENTION_SECONDS as i64 * NS_IN_SEC
    );
    assert_eq!(config.push.expires_at(NS_IN_SEC), None);
    assert_eq!(config.push.expires_at(-1), None);
    let error = Config::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = {}",
        MAX_RETENTION_SECONDS + 1
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ConfigError::Invalid {
            field: "push.recipient_ttl_seconds",
            ..
        }
    ));
}

/// A key that the field table does not list is refused in every table with
/// fixed keys, including an `[auth]` table whose `enabled` is `false`.
// verifies: CONF-083
#[xmtp_common::test(unwrap_try = true)]
fn unknown_keys_are_refused_in_every_table() {
    let rows = example::field_table();
    let base = full_document(AuthSource::InlineKeys);
    Config::load_str(&toml::to_string(&base)?)?;
    let mut tables = vec![String::new()];
    for row in &rows {
        let element = match row.kind.as_str() {
            "table" => false,
            "array of tables" => true,
            _ => continue,
        };
        // A table whose keys the operator chooses lists every key name.
        if rows
            .iter()
            .any(|other| other.key.starts_with(&format!("{}.<", row.key)))
        {
            continue;
        }
        tables.push(if element {
            format!("{}[]", row.key)
        } else {
            row.key.clone()
        });
    }
    for (table, enabled) in tables
        .iter()
        .map(|table| (table.as_str(), true))
        .chain([("auth", false)])
    {
        let mut document = base.clone();
        table_at(&mut document, "auth").insert("enabled".into(), enabled.into());
        table_at(&mut document, table).insert("unlisted_key".into(), true.into());
        assert!(
            Config::load_str(&toml::to_string(&document)?).is_err(),
            "an unlisted key in [{table}] with auth enabled = {enabled} must be refused"
        );
    }
}

/// Where `full_document` takes its JWT keys from.
#[derive(Clone, Copy)]
enum AuthSource {
    JwksUrl,
    InlineKeys,
}

/// A configuration that loads and holds every table of the field table, with
/// an enabled `[auth]` table.
fn full_document(auth: AuthSource) -> toml::Table {
    let mut document: toml::Table = toml::from_str(
        r#"
[server]
identifier = "org.xmtp.test"
[telemetry.resource_attributes]
"deployment.environment" = "test"
[database]
url = "postgres://localhost/xmtp"
[publishing]
[streams]
[retention]
[validation]
[limits]
[mls]
[handles]
[chains]
"eip155:1" = "https://rpc.example.org"
[auth]
enabled = true
jwks_url = "https://auth.example.org/keys"
audiences = ["xmtp.example.org"]
issuers = ["https://auth.example.org/"]
[auth.api_keys]
bot = "0123456789abcdef0123456789abcdef"
[push.apns]
key = "apns-key"
key_id = "ABC123DEFG"
team_id = "DEF123GHIJ"
bundle_id = "org.example.app"
environment = "production"
[push.fcm]
service_account = "{}"
[push.http]
allowed_domains = ["push.example.org"]
[attachments]
base_url = "https://files.example.org/attachments"
[attachments.target.S3]
endpoint = "https://s3.example.org"
region = "us-east-1"
bucket = "attachments"
[attachments.target.S3.credentials]
kind = "static"
access_key_id = "AKIAIOSFODNN7EXAMPLE"
secret_access_key = "secret"
[[application_components]]
component_id = 0xC000
name = "USER_STATUS"
component_type = "string"
insert_policy = "allow"
update_policy = "allow"
delete_policy = "allow"
in_groups = true
in_dms = false
"#,
    )
    .expect("the full document is TOML");
    if let AuthSource::InlineKeys = auth {
        let auth = table_at(&mut document, "auth");
        auth.remove("jwks_url");
        let mut key = toml::Table::new();
        key.insert("kid".into(), "test".into());
        key.insert("alg".into(), "ES256".into());
        key.insert("public_key".into(), ES256_PUBLIC_KEY.into());
        auth.insert("keys".into(), vec![toml::Value::Table(key)].into());
    }
    document
}

/// The table at a dotted path. `name[]` is the first element of an array of
/// tables, and the empty path is the document.
fn table_at<'a>(document: &'a mut toml::Table, path: &str) -> &'a mut toml::Table {
    let mut table = document;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        let (name, element) = match segment.strip_suffix("[]") {
            Some(name) => (name, true),
            None => (segment, false),
        };
        let value = table
            .get_mut(name)
            .unwrap_or_else(|| panic!("the document has no {name} on {path}"));
        table = if element {
            value
                .as_array_mut()
                .and_then(|items| items.first_mut())
                .and_then(toml::Value::as_table_mut)
        } else {
            value.as_table_mut()
        }
        .unwrap_or_else(|| panic!("{name} on {path} is not a table"));
    }
    table
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-021, CONF-089
fn every_cross_field_relationship_is_rejected() {
    let mut config: Config = toml::from_str(MINIMAL)?;

    config.publishing.max_publish_duration_ms = config.database.max_statement_timeout_ms;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "publishing.max_publish_duration_ms",
            ..
        })
    ));
    // Each relation holds at its boundary.
    config.publishing.max_publish_duration_ms = config.database.max_statement_timeout_ms + 1;
    config.validate()?;
    config.limits.max_query_limit = config.limits.default_query_limit;
    config.limits.max_update_adds = config.limits.max_stream_topics;
    config.validate()?;
    config = toml::from_str(MINIMAL)?;
    config.streams.max_pong_wait_ms = config.streams.keepalive_interval_ms;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "streams.max_pong_wait_ms",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_query_limit = config.limits.default_query_limit - 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_query_limit",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = config.limits.max_request_bytes + 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_envelope_bytes",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
    config.limits.max_request_bytes = RESPONSE_TEST_REQUEST_BYTES;
    config.limits.max_response_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_envelope_bytes",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_update_adds = config.limits.max_stream_topics + 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_update_adds",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES - ENVELOPE_METADATA_AND_FRAMING_BYTES;
    config.limits.max_request_bytes = DELIVERY_FRAME_BYTES;
    config.limits.max_response_bytes = DELIVERY_FRAME_BYTES;
    config.validate()?;
    config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_envelope_bytes",
            ..
        })
    ));
}

// verifies: CONF-084
#[xmtp_common::test(unwrap_try = true)]
fn missing_and_empty_environment_references_fail_without_values() {
    let missing = write_config(
        "missing",
        "[database]\nurl = 'env:XMTP_BACKEND_CONFIG_MISSING_9F31'\n",
    );
    let missing_error = Config::load(&missing.0).expect_err("missing env must fail");
    assert!(
        missing_error
            .to_string()
            .contains("XMTP_BACKEND_CONFIG_MISSING_9F31")
    );

    let empty = write_config("empty", "[database]\nurl = 'env:'\n");
    let empty_error = Config::load(&empty.0).expect_err("empty env must fail");
    assert!(!empty_error.to_string().contains("env:"));
}

#[xmtp_common::test(unwrap_try = true)]
fn debug_output_redacts_urls() {
    let config: Config =
        toml::from_str("[database]\nurl = 'postgres://user:password@localhost/xmtp'\n")?;
    let debug = format!("{config:?}");
    assert!(!debug.contains("password"));
    assert!(debug.contains("<redacted>"));
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_matches_generated_artifact() {
    let generated = serde_json::to_value(Config::schema())?;
    let published: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/schemas/backend-v1.json"))?;
    assert_eq!(generated, published);
    assert_eq!(generated["$id"], SCHEMA_ID);
}

#[xmtp_common::test(unwrap_try = true)]
fn environment_reference_returns_variable_value() {
    let path = std::env::var("PATH")?;
    assert_eq!(resolve_env("env:PATH")?, path);
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-020
fn finite_retention_must_fit_the_authoritative_database_clock() {
    const DATABASE_NS: i64 = 1_700_000_000_000_000_000;
    let maximum_seconds = ((i64::MAX - DATABASE_NS) / NS_IN_SEC) as u64;
    for field in [
        "retention.group_message_seconds",
        "retention.welcome_seconds",
        "retention.key_package_seconds",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        let duration = match field {
            "retention.group_message_seconds" => &mut config.retention.group_message_seconds,
            "retention.welcome_seconds" => &mut config.retention.welcome_seconds,
            _ => &mut config.retention.key_package_seconds,
        };
        *duration = maximum_seconds;
        config.retention.validate_at(DATABASE_NS)?;
        let mut invalid = config.retention.clone();
        match field {
            "retention.group_message_seconds" => invalid.group_message_seconds += 1,
            "retention.welcome_seconds" => invalid.welcome_seconds += 1,
            _ => invalid.key_package_seconds += 1,
        }
        assert!(
            matches!(invalid.validate_at(DATABASE_NS), Err(ConfigError::Invalid { field: actual, .. }) if actual == field)
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn configurable_deadlines_follow_the_host_monotonic_clock_range() {
    for milliseconds in [1, 60_000, u64::MAX] {
        let representable = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(milliseconds))
            .is_some();
        assert_eq!(
            validate_deadline(milliseconds, "timer").is_ok(),
            representable
        );
    }
    for field in [
        "server.max_drain_duration_ms",
        "streams.poll_interval_ms",
        "streams.max_pong_wait_ms",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        match field {
            "server.max_drain_duration_ms" => config.server.max_drain_duration_ms = u64::MAX,
            "streams.poll_interval_ms" => config.streams.poll_interval_ms = u64::MAX,
            _ => config.streams.max_pong_wait_ms = u64::MAX,
        }
        if validate_deadline(u64::MAX, field).is_err() {
            assert!(
                matches!(config.validate(), Err(ConfigError::Invalid { field: actual, .. }) if actual == field)
            );
        } else {
            config.validate()?;
        }
    }
}

struct ConfigFile(std::path::PathBuf);
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_config(name: &str, source: &str) -> ConfigFile {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "xmtp-backend-config-{}-{name}.toml",
        xmtp_common::rand_hexstring()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("create test config");
    let guard = ConfigFile(path);
    file.write_all(source.as_bytes())
        .expect("write test config");
    guard
}

#[xmtp_common::test(unwrap_try = true)]
fn telemetry_defaults_and_endpoint_precedence() {
    let config: Config = toml::from_str(MINIMAL)?;
    assert_eq!(config.telemetry.metrics_listen, "0.0.0.0:9464");
    assert_eq!(config.server.log_format, LogFormat::Text);
    assert_eq!(config.telemetry.service_name, "xmtp-backend");
    assert!(!config.telemetry.otlp_logs);
    assert!(config.telemetry.otlp_endpoint(None)?.is_none());
    assert_eq!(
        config
            .telemetry
            .otlp_endpoint(Some("http://tempo:4317".into()))?
            .as_deref(),
        Some("http://tempo:4317")
    );
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}log_format = 'json'\n[telemetry]\nmetrics_listen = ''\notlp_endpoint = 'http://collector:4317'\notlp_logs = true\nservice_name = 'custom'\nsample_ratio = 0.25\nresource_attributes = {{ 'deployment.environment' = 'test' }}"
    ))?;
    config.validate()?;
    assert_eq!(config.server.log_format, LogFormat::Json);
    // The configured endpoint wins over the environment fallback.
    assert_eq!(
        config
            .telemetry
            .otlp_endpoint(Some("malformed-fallback-secret".into()))?
            .as_deref(),
        Some("http://collector:4317")
    );
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-022
fn telemetry_rejects_unknown_reserved_and_invalid_values_without_endpoint_contents() {
    assert!(toml::from_str::<Config>(&format!("{MINIMAL}\n[telemetry]\nunknown = true")).is_err());
    for key in ["service.name", "service.version", "xmtp.backend.identifier"] {
        let config: Config = toml::from_str(&format!(
            "{MINIMAL}\n[telemetry.resource_attributes]\n'{key}' = 'override'"
        ))?;
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("telemetry.resource_attributes"));
        assert!(error.contains(key));
    }
    for endpoint in [
        "secret-invalid",
        "ftp://secret",
        "http://[secret",
        "http://host/secret value",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.telemetry.otlp_endpoint = Some(endpoint.into());
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("telemetry.otlp_endpoint"));
        assert!(!error.contains(endpoint));
        config.telemetry.otlp_endpoint = None;
        let error = config
            .telemetry
            .otlp_endpoint(Some(endpoint.into()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("OTEL_EXPORTER_OTLP_ENDPOINT"));
        assert!(!error.contains(endpoint));
    }
    for ratio in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.telemetry.sample_ratio = ratio;
        assert!(config.validate().is_err());
    }
    let bad = write_config(
        "telemetry-endpoint",
        &format!("{MINIMAL}\n[telemetry]\notlp_endpoint = 'env:PATH'"),
    );
    let error = Config::load(&bad.0).unwrap_err().to_string();
    assert!(error.contains("telemetry.otlp_endpoint"));
    assert!(!error.contains(&std::env::var("PATH")?));
}

// verifies: CONF-084
#[xmtp_common::test(unwrap_try = true)]
fn telemetry_string_values_resolve_environment_references_once() {
    let file = write_config(
        "telemetry-env",
        &format!(
            "{MINIMAL}\n[telemetry]\nservice_name = 'env:PATH'\nresource_attributes = {{ custom = 'env:PATH' }}"
        ),
    );
    let config = Config::load(&file.0)?;
    assert_eq!(config.telemetry.service_name, std::env::var("PATH")?);
    assert_eq!(
        config.telemetry.resource_attributes["custom"],
        std::env::var("PATH")?
    );
}

/// A resolved value that itself begins with `env:` is kept as written, a
/// reference in an array resolves, and a variable that is not Unicode is
/// refused by name.
// verifies: CONF-084
#[xmtp_common::test(unwrap_try = true)]
fn environment_references_resolve_once_in_every_string_position() {
    const CHILD: &str = "XMTP_CONFIG_REFERENCE_CHILD";
    const NESTED: &str = "XMTP_CONFIG_REFERENCE_NESTED";
    const TARGET: &str = "XMTP_CONFIG_REFERENCE_TARGET";
    const ELEMENT: &str = "XMTP_CONFIG_REFERENCE_ELEMENT";
    const NOT_UNICODE: &str = "XMTP_CONFIG_REFERENCE_NOT_UNICODE";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::ffi::OsStrExt;
        let output = child_test(
            module_path!(),
            "environment_references_resolve_once_in_every_string_position",
        )
        .env(CHILD, "1")
        .env(NESTED, format!("env:{TARGET}"))
        .env(TARGET, "must-not-be-read")
        .env(ELEMENT, "xmtp.example.org")
        .env(NOT_UNICODE, std::ffi::OsStr::from_bytes(b"\xff"))
        .output()?;
        assert_one_test_passed(&output);
        return;
    }
    let config = Config::load_str(&format!(
        "{MINIMAL}[telemetry]\nservice_name = 'env:{NESTED}'\n[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'\naudiences = ['env:{ELEMENT}', 'literal']\n"
    ))?;
    assert_eq!(config.telemetry.service_name, format!("env:{TARGET}"));
    assert_eq!(
        config.auth?.audiences?,
        vec!["xmtp.example.org".to_owned(), "literal".to_owned()]
    );
    let error = Config::load_str(&format!(
        "{MINIMAL}[telemetry]\nservice_name = 'env:{NOT_UNICODE}'\n"
    ))
    .unwrap_err();
    assert!(
        matches!(&error, ConfigError::Environment { name } if name == NOT_UNICODE),
        "{error}"
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn inline_configuration_resolves_environment_references() {
    let config = Config::load_str(&format!(
        "{MINIMAL}\n[telemetry]\nresource_attributes = {{ path = 'env:PATH' }}\n"
    ))?;
    assert_eq!(
        config.telemetry.resource_attributes["path"],
        std::env::var("PATH")?
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn malformed_inline_configuration_omits_document_contents() {
    // Guard the unit variant: retaining a TOML parser error could expose input text.
    let error = Config::load_str("secret-inline-sentinel = [").unwrap_err();
    assert!(matches!(error, ConfigError::Parse));
    assert!(!format!("{error} {error:?}").contains("secret-inline-sentinel"));
}

/// A deployment that does not name itself cannot be bound to by a client
/// database, so an unnamed or malformed identifier stops the process.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-002
fn the_identifier_is_required_and_shaped() {
    let missing: Config = toml::from_str("[database]\nurl = 'postgres://localhost/xmtp'\n")?;
    let error = missing.validate().unwrap_err();
    assert!(matches!(
        error,
        ConfigError::Invalid {
            field: "server.identifier",
            ..
        }
    ));
    for identifier in [
        String::new(),
        "a".repeat(MAX_SERVER_IDENTIFIER_BYTES + 1),
        // A single multi-byte character can also push a short name over the
        // byte bound, which is what the rule counts.
        "é".repeat(MAX_SERVER_IDENTIFIER_BYTES / 2 + 1),
        "org.xmtp has a space".into(),
        "org.xmtp\u{0}dev".into(),
        "org.xmtp\ndev".into(),
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.server.identifier = Some(identifier.clone());
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::Invalid {
                    field: "server.identifier",
                    ..
                })
            ),
            "{identifier:?} must be rejected"
        );
    }
    // The documented convention is accepted at the byte bound.
    let mut config: Config = toml::from_str(MINIMAL)?;
    config.server.identifier = Some("a".repeat(MAX_SERVER_IDENTIFIER_BYTES));
    config.validate()?;
}

/// An existing file that gained `[auth]` before this rule must state its
/// intent, so auth can never switch off by accident. A section that says it is
/// off is not checked any further and loads no key material.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-005, CONF-066
fn an_auth_section_must_state_whether_it_is_enabled() {
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}\n[auth]\njwks_url = 'https://issuer.example/keys'\n"
    ))?;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::Invalid {
            field: "auth.enabled",
            ..
        }
    ));
    // Disabled auth ignores every other field, including ones that would fail
    // their own checks. The backend publishes an empty summary for it.
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\naudiences = []\nissuers = []\n"
    ))?;
    config.validate()?;
    assert!(!config.auth.as_ref()?.is_enabled());
    // Ignoring a field is not the same as never reading it: environment
    // references resolve for the whole document before any section is
    // validated, so a missing variable in a disabled section still fails.
    let error = Config::load_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\njwks_url = 'env:XMTP_AUTH_DISABLED_MISSING'\n"
    ))
    .unwrap_err();
    assert!(
        matches!(&error, ConfigError::Environment { name } if name == "XMTP_AUTH_DISABLED_MISSING")
    );
}

/// An operator states a minimum client version as a semantic version, or
/// states none and admits every client version.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-067
fn the_minimum_client_version_is_optional_and_semantic() {
    for version in ["1", "1.2", "v1.2.3", "latest", "1.2.3.4", ""] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.server.min_libxmtp_version = Some(version.into());
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::Invalid {
                    field: "server.min_libxmtp_version",
                    ..
                })
            ),
            "{version:?} must be rejected"
        );
    }
    let config: Config = toml::from_str(MINIMAL)?;
    config.validate()?;
    assert!(config.server.min_libxmtp_version.is_none());
    let mut config: Config = toml::from_str(MINIMAL)?;
    config.server.min_libxmtp_version = Some("1.2.3-beta.1".into());
    config.validate()?;
}

/// The transport ceiling is fixed, so a budget above it would promise a client
/// something the transport cannot carry.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-008
fn request_and_response_budgets_stop_at_the_transport_ceiling() {
    for field in ["limits.max_request_bytes", "limits.max_response_bytes"] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        match field {
            "limits.max_request_bytes" => config.limits.max_request_bytes = MAX_TRANSPORT_BYTES,
            _ => config.limits.max_response_bytes = MAX_TRANSPORT_BYTES,
        }
        config.validate()?;
        match field {
            "limits.max_request_bytes" => config.limits.max_request_bytes += 1,
            _ => config.limits.max_response_bytes += 1,
        }
        let error = config.validate().unwrap_err();
        assert!(
            matches!(&error, ConfigError::Invalid { field: named, .. } if *named == field),
            "{field} must be named, got {error}"
        );
    }
}

/// An absent minimum name length takes the default of 3. The two ends of the
/// range start, and 0 and 28 refuse to start and name the key.
// verifies: CONF-065, CONF-080
#[xmtp_common::test(unwrap_try = true)]
fn handles_min_name_length_defaults_and_range() {
    let config = Config::load_str(MINIMAL)?;
    assert_eq!(config.handles.min_name_length, 3);
    for length in [1, 27] {
        let config =
            Config::load_str(&format!("{MINIMAL}\n[handles]\nmin_name_length = {length}"))?;
        assert_eq!(config.handles.min_name_length, length);
    }
    for length in [0, 28] {
        let error = Config::load_str(&format!("{MINIMAL}\n[handles]\nmin_name_length = {length}"))
            .unwrap_err();
        assert!(
            matches!(
                error,
                ConfigError::Invalid {
                    field: "handles.min_name_length",
                    ..
                }
            ),
            "{length}: {error}"
        );
    }
}

/// One TOML value, written as it would be in a file.
fn literal(text: &str) -> toml::Value {
    toml::from_str::<toml::Table>(&format!("value = {text}"))
        .unwrap_or_else(|error| panic!("{text} is not a TOML value: {error}"))
        .remove("value")
        .expect("the value was just written")
}

/// The standard checks a Checks cell names before its first semicolon, each
/// as the text of its code span, with `key ` before a check on the key.
fn standard_checks(cell: &str) -> Vec<String> {
    let standard = cell.split(';').next().unwrap_or_default();
    let mut checks = Vec::new();
    let mut parts = standard.split('`');
    while let (Some(before), Some(check)) = (parts.next(), parts.next()) {
        checks.push(if before.trim_end().ends_with("key") {
            format!("key {check}")
        } else {
            check.to_owned()
        });
    }
    checks
}

/// One value that fails one standard check of one row of the field table.
struct FailingValue {
    key: &'static str,
    check: &'static str,
    auth: AuthSource,
    table: &'static str,
    name: String,
    value: toml::Value,
}

fn failing(key: &'static str, check: &'static str, text: &str) -> FailingValue {
    let (table, name) = key.rsplit_once('.').expect("a key below the root");
    FailingValue {
        key,
        check,
        auth: AuthSource::JwksUrl,
        table,
        name: name.to_owned(),
        value: literal(text),
    }
}

/// A value for a key the operator chooses, set in `table`.
fn failing_name(
    key: &'static str,
    check: &'static str,
    table: &'static str,
    name: &str,
    text: &str,
) -> FailingValue {
    FailingValue {
        table,
        name: name.to_owned(),
        ..failing(key, check, text)
    }
}

fn with_inline_keys(case: FailingValue) -> FailingValue {
    FailingValue {
        auth: AuthSource::InlineKeys,
        ..case
    }
}

const COUNT: &str = "range 1..4294967295";
const MAX_INT: &str = "range 1..2147483647";

/// Rows whose standard check has no failing value that today's code refuses.
/// The only value outside `range 0..4294967295` is above it, and the backend
/// checks this key against the host clock instead until
/// https://github.com/xmtp/libxmtp/issues/4590 is fixed.
const REFUSED_LATER: &[(&str, &str)] = &[("auth.jwks_max_stale_seconds", "range 0..4294967295")];

fn failing_values() -> Vec<FailingValue> {
    let mut cases = vec![
        failing("server.listen", "socket address", "'localhost:5050'"),
        failing(
            "server.log_level",
            "one of: off, error, warn, info, debug, trace",
            "'verbose'",
        ),
        failing("server.log_format", "one of: text, json", "'xml'"),
        failing(
            "telemetry.metrics_listen",
            "socket address or empty",
            "'localhost:9464'",
        ),
        failing("telemetry.service_name", "non-blank", "\" \\t \""),
        failing("telemetry.sample_ratio", "range 0.0..1.0", "-0.1"),
        failing("telemetry.sample_ratio", "range 0.0..1.0", "1.1"),
        failing("telemetry.sample_ratio", "range 0.0..1.0", "nan"),
        failing(
            "database.url",
            "url postgres, postgresql",
            "'mysql://localhost/xmtp'",
        ),
        failing(
            "database.replica_url",
            "url postgres, postgresql",
            "'mysql://localhost/xmtp'",
        ),
        failing("database.max_connections", COUNT, "4294967296"),
        failing("database.max_statement_timeout_ms", MAX_INT, "0"),
        failing("database.max_statement_timeout_ms", MAX_INT, "2147483648"),
        failing("publishing.max_publish_duration_ms", MAX_INT, "0"),
        failing("publishing.max_publish_duration_ms", MAX_INT, "2147483648"),
        failing("publishing.max_barrier_wait_ms", MAX_INT, "0"),
        failing("publishing.max_barrier_wait_ms", MAX_INT, "2147483648"),
        failing("streams.max_gap_ranges", "range 1..", "0"),
        failing("streams.keepalive_interval_ms", COUNT, "4294967296"),
        failing("validation.max_scw_cache_entries", "range 1..", "0"),
        failing("limits.max_push_topics", MAX_INT, "0"),
        failing("limits.max_push_topics", MAX_INT, "2147483648"),
        failing("mls.max_group_members", "range 1..65535", "0"),
        failing("mls.max_group_members", "range 1..65535", "65536"),
        failing("mls.max_installations_per_inbox", "range 1..65535", "0"),
        failing("mls.max_installations_per_inbox", "range 1..65535", "65536"),
        failing_name(
            "chains.<chain id>",
            "url http, https",
            "chains",
            "eip155:1",
            "'ftp://rpc.example.org'",
        ),
        failing(
            "auth.jwks_url",
            "secure url",
            "'http://auth.example.org/keys'",
        ),
        with_inline_keys(failing("auth.keys", "non-empty", "[]")),
        with_inline_keys(failing_name(
            "auth.keys[].kid",
            "bytes 1..256",
            "auth.keys[]",
            "kid",
            "''",
        )),
        with_inline_keys(failing_name(
            "auth.keys[].kid",
            "bytes 1..256",
            "auth.keys[]",
            "kid",
            &format!("'{}'", "k".repeat(257)),
        )),
        with_inline_keys(failing_name(
            "auth.keys[].alg",
            "one of",
            "auth.keys[]",
            "alg",
            "'HS256'",
        )),
        failing_name(
            "auth.api_keys.<name>",
            "key pattern ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$",
            "auth.api_keys",
            "-bot",
            "'fedcba9876543210fedcba9876543210'",
        ),
        failing("auth.audiences", "non-empty", "[]"),
        failing("auth.issuers", "non-empty", "[]"),
        failing("push.recipient_ttl_seconds", "range 86400..", "86399"),
        failing("push.max_attempts", "range 1..10", "0"),
        failing("push.max_attempts", "range 1..10", "11"),
        failing("push.apns.key", "non-empty", "''"),
        failing("push.apns.key_id", "non-empty", "''"),
        failing("push.apns.team_id", "non-empty", "''"),
        failing("push.apns.bundle_id", "non-empty", "''"),
        failing(
            "push.apns.environment",
            "one of: production, sandbox",
            "'development'",
        ),
        failing("push.fcm.service_account", "non-empty", "''"),
        failing(
            "push.http.allowed_domains",
            "host name",
            "['-bad.example.org']",
        ),
        failing(
            "attachments.target.S3.endpoint",
            "plain url",
            "'https://user:password@s3.example.org'",
        ),
    ];
    let api_keys: toml::Table = (0..=crate::auth::MAX_API_KEYS)
        .map(|index| (format!("bot-{index}"), format!("{index:0>32}").into()))
        .collect();
    cases.push(FailingValue {
        value: toml::Value::Table(api_keys),
        ..failing("auth.api_keys", "entries ..256", "{}")
    });
    for name in ["service.name", "service.version", "xmtp.backend.identifier"] {
        cases.push(failing_name(
            "telemetry.resource_attributes.<name>",
            "key not: service.name, service.version, xmtp.backend.identifier",
            "telemetry.resource_attributes",
            name,
            "'reserved'",
        ));
    }
    // The upper bound of these keys is refused only after
    // https://github.com/xmtp/libxmtp/issues/4590; their lower bound is here.
    for key in [
        "server.max_drain_duration_ms",
        "streams.poll_interval_ms",
        "streams.max_pong_wait_ms",
        "limits.max_query_topics",
        "limits.default_query_limit",
        "limits.max_query_limit",
        "limits.max_newest_metadata_topics",
        "limits.max_newest_full_topics",
        "limits.max_publish_topics",
        "limits.max_update_adds",
        "limits.max_update_removes",
        "limits.max_stream_topics",
        "limits.max_static_topics",
        "limits.max_lookup_identifiers",
        "limits.max_scw_signatures",
        "limits.max_identity_entries",
    ] {
        cases.push(failing(key, COUNT, "0"));
    }
    for key in [
        "database.max_connections",
        "streams.keepalive_interval_ms",
        "limits.max_http2_streams",
        "limits.max_update_frames_per_second",
        "limits.max_update_burst",
        "limits.max_ping_frames_per_second",
        "limits.max_ping_burst",
    ] {
        cases.push(failing(key, COUNT, "0"));
    }
    for key in [
        "limits.max_http2_streams",
        "limits.max_update_frames_per_second",
        "limits.max_update_burst",
        "limits.max_ping_frames_per_second",
        "limits.max_ping_burst",
    ] {
        cases.push(failing(key, COUNT, "4294967296"));
    }
    for key in [
        "retention.group_message_seconds",
        "retention.welcome_seconds",
        "retention.key_package_seconds",
        "limits.max_envelope_bytes",
        "limits.max_request_bytes",
        "limits.max_response_bytes",
    ] {
        cases.push(failing(key, "range 1..", "0"));
    }
    for key in [
        "application_components[].insert_policy",
        "application_components[].update_policy",
        "application_components[].delete_policy",
    ] {
        cases.push(failing_name(
            key,
            "one of: allow, deny, allow_if_admin, allow_if_super_admin, allow_if_self_or_non_member",
            "application_components[]",
            key.rsplit_once('.').expect("a key below the root").1,
            "'maybe'",
        ));
    }
    cases
}

/// Every standard check the field table names refuses a value that fails it.
/// Inside a disabled `[auth]` table, the same values start.
// verifies: CONF-087, CONF-066
#[xmtp_common::test(unwrap_try = true)]
fn every_standard_check_in_the_field_table_refuses_a_failing_value() {
    let cases = failing_values();
    let named: BTreeSet<(String, String)> = example::field_table()
        .iter()
        .flat_map(|row| {
            standard_checks(&row.checks)
                .into_iter()
                .map(|check| (row.key.clone(), check))
        })
        .collect();
    let covered: BTreeSet<(String, String)> = cases
        .iter()
        .map(|case| (case.key.to_owned(), case.check.to_owned()))
        .chain(
            REFUSED_LATER
                .iter()
                .map(|(key, check)| ((*key).to_owned(), (*check).to_owned())),
        )
        .collect();
    assert_eq!(
        named, covered,
        "the field table and the cases name different checks"
    );

    for auth in [AuthSource::JwksUrl, AuthSource::InlineKeys] {
        Config::load_str(&toml::to_string(&full_document(auth))?)?;
    }
    for case in &cases {
        let mut document = full_document(case.auth);
        table_at(&mut document, case.table).insert(case.name.clone(), case.value.clone());
        let text = toml::to_string(&document)?;
        assert!(
            Config::load_str(&text).is_err(),
            "{} = {} must fail {}",
            case.key,
            case.value,
            case.check
        );
        if case.key.starts_with("auth.") {
            table_at(&mut document, "auth").insert("enabled".into(), false.into());
            if let Err(error) = Config::load_str(&toml::to_string(&document)?) {
                panic!(
                    "{} = {} must start while auth is disabled: {error}",
                    case.key, case.value
                );
            }
        }
    }
}

/// An absent key takes the value its Default cell states, read from the spec:
/// a literal, or the compiled default of section 3. Keys of an optional table
/// are read from a file that adds only that table's required keys.
// verifies: CONF-088
#[xmtp_common::test(unwrap_try = true)]
fn minimal_configuration_uses_defaults() {
    const REQUIRED: &str =
        "[server]\nidentifier = 'org.xmtp.test'\n[database]\nurl = 'postgres://localhost/xmtp'\n";
    let minimal = Config::load_str(REQUIRED)?;
    let tables = Config::load_str(&format!(
        "{REQUIRED}[auth]\nenabled = false\n[push.http]\n[attachments]\nbase_url = 'https://files.example.org'\n[attachments.target.S3]\nendpoint = 'https://s3.example.org'\nregion = 'us-east-1'\nbucket = 'attachments'\n[attachments.target.S3.credentials]\nkind = 'web_identity'\n"
    ))?;
    let mut tables_loaded = serde_json::to_value(&tables)?;
    // The target applies its presign lifetime default when it signs.
    let Some(xmtp_attachments_server::TargetConfig::S3(s3)) = tables
        .attachments
        .as_ref()
        .map(|attachments| &attachments.target)
    else {
        panic!("the attachments table loaded without its target");
    };
    tables_loaded["attachments"]["target"]["S3"]["presign_ttl_seconds"] =
        s3.presign_ttl_seconds().into();
    let loaded = [serde_json::to_value(&minimal)?, tables_loaded];

    let compiled = example::compiled_defaults();
    let rows = example::field_table();
    for row in &rows {
        let text = match row.default.as_str() {
            // The key has no default, or the requirement that reads it gives
            // the effect of its absence.
            "none" | "absent" => continue,
            "section 3" => example::code(
                compiled
                    .get(&row.key)
                    .unwrap_or_else(|| panic!("section 3 has no default for {}", row.key)),
            ),
            default if default.len() > 2 && default.starts_with('`') && default.ends_with('`') => {
                example::code(default)
            }
            // Another requirement or term states the default and governs it.
            default if is_requirement_id(default) || is_terms_reference(default) => continue,
            default => panic!(
                "{}: the Default cell {default:?} is not a value, section 3, none, absent, or a reference",
                row.key
            ),
        };
        // A table's own keys have rows of their own, which carry their defaults.
        if text == "{}"
            && rows.iter().any(|other| {
                other
                    .key
                    .strip_prefix(&format!("{}.", row.key))
                    .is_some_and(|rest| !rest.starts_with('<'))
            })
        {
            continue;
        }
        let expected = serde_json::to_value(literal(text))?;
        let actual = loaded
            .iter()
            .find_map(|config| {
                row.key
                    .split('.')
                    .try_fold(config, |value, name| value.get(name))
                    .filter(|value| !value.is_null())
            })
            .unwrap_or_else(|| panic!("{} is not in either loaded file", row.key));
        assert_eq!(actual, &expected, "{}", row.key);
    }
}

/// Whether `text` has the form of a requirement identifier, such as
/// `PUSH-226`: three to five capital letters, `-`, and three digits.
fn is_requirement_id(text: &str) -> bool {
    text.split_once('-').is_some_and(|(prefix, number)| {
        is_spec_prefix(prefix)
            && number.len() == 3
            && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Whether `text` names a term of another spec, such as
/// `OPS Terms (Drain deadline)`.
fn is_terms_reference(text: &str) -> bool {
    text.split_once(" Terms (").is_some_and(|(prefix, term)| {
        is_spec_prefix(prefix) && term.len() > 1 && term.ends_with(')')
    })
}

fn is_spec_prefix(prefix: &str) -> bool {
    (3..=5).contains(&prefix.len()) && prefix.bytes().all(|byte| byte.is_ascii_uppercase())
}

/// A chain key is `eip155:` and a chain id in canonical decimal form that fits
/// in 64 bits.
// verifies: CONF-090
#[xmtp_common::test(unwrap_try = true)]
fn chain_keys_must_be_eip155_chain_ids() {
    let load = |chain: &str| {
        Config::load_str(&format!(
            "{MINIMAL}[chains]\n\"{chain}\" = 'https://rpc.example.org'\n"
        ))
    };
    for chain in [
        "eip155:01",
        "eip155:+1",
        "eip155:18446744073709551616",
        "cosmos:cosmoshub-4",
    ] {
        let error = load(chain).unwrap_err();
        assert!(
            matches!(
                error,
                ConfigError::Invalid {
                    field: "chains",
                    ..
                }
            ),
            "{chain}: {error}"
        );
    }
    for chain in ["eip155:0", "eip155:1", "eip155:18446744073709551615"] {
        let config = load(chain)?;
        assert!(config.chains.contains_key(chain), "{chain}");
    }
}
