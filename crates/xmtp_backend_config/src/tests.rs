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

/// Require that `result` is an `Invalid` refusal naming `field`, and return
/// the refusal.
#[track_caller]
pub(crate) fn assert_invalid<T>(result: Result<T, ConfigError>, field: &str) -> ConfigError {
    let Err(error) = result else {
        panic!("{field} must be refused");
    };
    assert!(matches!(error, ConfigError::Invalid { .. }), "{error}");
    assert_eq!(error.field(), Some(field), "{error}");
    error
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
    for (fields, expected_key, expected_reason) in [
        (
            "kind = 'unknown'",
            "attachments.target.S3.credentials.kind",
            "is not one of the allowed values",
        ),
        (
            "kind = 'profile'",
            "attachments.target.S3.credentials.name",
            "is required",
        ),
        (
            "kind = 'profile'\nname = ''",
            "attachments.target.S3.credentials.name",
            "must not be empty",
        ),
        (
            "kind = 'profile'\nname = 3",
            "attachments.target.S3.credentials.name",
            "has the wrong type or value",
        ),
    ] {
        let error = assert_invalid(
            ValidatedConfig::load_str(&format!("{source}{fields}\n")),
            expected_key,
        );
        assert!(
            matches!(error, ConfigError::Invalid { reason, .. } if reason == expected_reason),
            "{fields}: {error}"
        );
        assert!(error.to_string().contains(expected_key));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn push_defaults_provider_fields_and_redaction() {
    let config = ValidatedConfig::load_str(MINIMAL)?;
    assert_eq!(config.push.recipient_ttl_seconds, 2_592_000);
    assert_eq!(config.push.max_attempts, 3);
    assert_eq!(config.limits.max_push_topics, 100_000);
    assert!(config.push.apns.is_none() && config.push.fcm.is_none() && config.push.http.is_none());
    let source = format!(
        "{MINIMAL}\n[push.apns]\nkey = 'private-apns-credential'\nkey_id = 'key'\nteam_id = 'team'\nbundle_id = 'bundle'\nenvironment = 'sandbox'\n[push.fcm]\nservice_account = 'private-fcm-credential'\n[push.http]\n"
    );
    let config = ValidatedConfig::load_str(&source)?;
    assert!(config.push.http.is_some());
    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("private-apns-credential"));
    assert!(!debug.contains("private-fcm-credential"));
    let source = source
        .replace("private-apns-credential", "env:CARGO_MANIFEST_DIR")
        .replace("private-fcm-credential", "env:CARGO_MANIFEST_DIR");
    let config = ValidatedConfig::load_str(&source)?;
    let expected = std::env::var("CARGO_MANIFEST_DIR")?;
    assert_eq!(
        config.push.apns.clone().unwrap().key.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(
        config.push.fcm.clone().unwrap().service_account.as_deref(),
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
        assert_invalid(
            ValidatedConfig::load_str(&format!("{MINIMAL}\n{extra}")),
            field,
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
        assert_invalid(
            ValidatedConfig::load_str(&format!(
                "{MINIMAL}\n[push.http]\nallowed_domains = ['{domain}']"
            )),
            "push.http.allowed_domains",
        );
    }
    ValidatedConfig::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = 86400\nmax_attempts = 10\n[push.http]\nallowed_domains = ['hooks.example.com', '*.EXAMPLE.org']"
    ))?;
}

#[xmtp_common::test(unwrap_try = true)]
fn push_expiry_is_checked_at_both_duration_and_clock_boundaries() {
    let config = ValidatedConfig::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = {MAX_RETENTION_SECONDS}"
    ))?;
    assert_eq!(
        config.push.expires_at(0)?,
        MAX_RETENTION_SECONDS as i64 * NS_IN_SEC
    );
    assert_eq!(config.push.expires_at(NS_IN_SEC), None);
    assert_eq!(config.push.expires_at(-1), None);
    assert_invalid(
        ValidatedConfig::load_str(&format!(
            "{MINIMAL}\n[push]\nrecipient_ttl_seconds = {}",
            MAX_RETENTION_SECONDS + 1
        )),
        "push.recipient_ttl_seconds",
    );
}

/// A key that the field table does not list is refused in every table with
/// fixed keys, including an `[auth]` table whose `enabled` is `false`.
// verifies: CONF-083
#[xmtp_common::test(unwrap_try = true)]
fn unknown_keys_are_refused_in_every_table() {
    let rows = example::field_table();
    let base = full_document(AuthSource::InlineKeys);
    ValidatedConfig::load_str(&toml::to_string(&base)?)?;
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
        let field = match table {
            "" => "unlisted_key".to_owned(),
            table => format!("{}.unlisted_key", table.replace("[]", "[0]")),
        };
        let error = assert_invalid(
            ValidatedConfig::load_str(&toml::to_string(&document)?),
            &field,
        );
        assert!(
            matches!(error, ConfigError::Invalid { reason, .. } if reason == "is not a configuration key"),
            "an unlisted key in [{table}] with auth enabled = {enabled}: {error}"
        );
    }
}

/// A credential kind refuses every key it does not take: any key next to a
/// kind that takes none, such as `access_key_id` next to `kind = "instance"`,
/// and a key of another kind, such as `role_arn` next to `kind = "static"`.
// verifies: CONF-083
#[xmtp_common::test(unwrap_try = true)]
fn credential_kinds_refuse_the_keys_they_do_not_take() {
    let schema = serde_json::to_value(Config::schema())?;
    let kinds: Vec<(String, BTreeSet<String>)> = schema["$defs"]["CredentialsConfig"]["oneOf"]
        .as_array()?
        .iter()
        .map(|variant| {
            let properties = variant["properties"]
                .as_object()
                .expect("each credential kind lists its keys");
            let kind = properties["kind"]["const"]
                .as_str()
                .expect("each credential kind names itself")
                .to_owned();
            let keys = properties
                .keys()
                .filter(|key| *key != "kind")
                .cloned()
                .collect();
            (kind, keys)
        })
        .collect();
    let every_key: BTreeSet<String> = kinds
        .iter()
        .flat_map(|(_, keys)| keys.iter().cloned())
        .collect();
    let load = |credentials: &toml::Table| {
        let mut document = full_document(AuthSource::JwksUrl);
        table_at(&mut document, "attachments.target.S3")
            .insert("credentials".into(), credentials.clone().into());
        ValidatedConfig::load_str(&toml::to_string(&document).expect("the document is TOML"))
    };
    let with_keys = |kind: &str, keys: &[&str]| {
        let mut credentials = toml::Table::new();
        credentials.insert("kind".into(), kind.into());
        for key in keys {
            credentials.insert((*key).into(), "value".into());
        }
        credentials
    };
    // The refusal names the extra key.
    let assert_refused = |result: Result<ValidatedConfig, ConfigError>, extra: &str| {
        let error = assert_invalid(
            result,
            &format!("attachments.target.S3.credentials.{extra}"),
        );
        assert!(
            matches!(error, ConfigError::Invalid { reason, .. } if reason == "is not a configuration key"),
            "{error}"
        );
    };
    for (kind, keys, extra) in [
        ("instance", vec!["access_key_id"], "access_key_id"),
        (
            "static",
            vec!["access_key_id", "secret_access_key", "role_arn"],
            "role_arn",
        ),
        (
            "assume_role",
            vec!["role_arn", "access_key_id"],
            "access_key_id",
        ),
    ] {
        assert_refused(load(&with_keys(kind, &keys)), extra);
    }
    let mut refused = 0;
    for (kind, keys) in &kinds {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        let credentials = with_keys(kind, &keys);
        if let Err(error) = load(&credentials) {
            panic!("kind = {kind} with its own keys must load: {error}");
        }
        for other in every_key.iter().filter(|key| !keys.contains(&key.as_str())) {
            let mut extra = credentials.clone();
            extra.insert(other.clone(), "value".into());
            assert_refused(load(&extra), other);
            refused += 1;
        }
    }
    assert!(refused > 100, "only {refused} keys were refused");
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
    assert_invalid(
        config.clone().validate(),
        "publishing.max_publish_duration_ms",
    );
    // Each relation holds at its boundary.
    config.publishing.max_publish_duration_ms = config.database.max_statement_timeout_ms + 1;
    config.clone().validate()?;
    config.limits.max_query_limit = config.limits.default_query_limit;
    config.limits.max_update_adds = config.limits.max_stream_topics;
    config.clone().validate()?;
    config = toml::from_str(MINIMAL)?;
    config.limits.max_query_limit = config.limits.default_query_limit - 1;
    assert_invalid(config.clone().validate(), "limits.max_query_limit");
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = config.limits.max_request_bytes + 1;
    assert_invalid(config.clone().validate(), "limits.max_envelope_bytes");
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
    config.limits.max_request_bytes = RESPONSE_TEST_REQUEST_BYTES;
    config.limits.max_response_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
    assert_invalid(config.clone().validate(), "limits.max_envelope_bytes");
    config = toml::from_str(MINIMAL)?;
    config.limits.max_update_adds = config.limits.max_stream_topics + 1;
    assert_invalid(config.clone().validate(), "limits.max_update_adds");
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES - ENVELOPE_METADATA_AND_FRAMING_BYTES;
    config.limits.max_request_bytes = DELIVERY_FRAME_BYTES;
    config.limits.max_response_bytes = DELIVERY_FRAME_BYTES;
    config.clone().validate()?;
    config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES;
    assert_invalid(config.clone().validate(), "limits.max_envelope_bytes");
}

// verifies: CONF-084
#[xmtp_common::test(unwrap_try = true)]
fn missing_and_empty_environment_references_fail_without_values() {
    let missing = write_config(
        "missing",
        "[database]\nurl = 'env:XMTP_BACKEND_CONFIG_MISSING_9F31'\n",
    );
    let missing_error = ValidatedConfig::load(&missing.0).expect_err("missing env must fail");
    assert!(
        missing_error
            .to_string()
            .contains("XMTP_BACKEND_CONFIG_MISSING_9F31")
    );

    let empty = write_config("empty", "[database]\nurl = 'env:'\n");
    let empty_error = ValidatedConfig::load(&empty.0).expect_err("empty env must fail");
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
    const PUBLISHED: &str = include_str!("../../../docs/schemas/backend-v1.json");
    let generated = serde_json::to_value(Config::schema())?;
    let published: serde_json::Value = serde_json::from_str(PUBLISHED)?;
    assert_eq!(generated, published);
    assert_eq!(generated["$id"], SCHEMA_ID);
    // Value equality ignores key order. The file is what `just backend schema`
    // writes, so its text must match too.
    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&Config::schema())?),
        PUBLISHED
    );
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
        assert_invalid(invalid.validate_at(DATABASE_NS), field);
    }
}

/// A timer without a fixed maximum must fit the host's monotonic clock.
#[xmtp_common::test(unwrap_try = true)]
fn uncapped_deadlines_follow_the_host_monotonic_clock_range() {
    for milliseconds in [1, 60_000, u64::MAX] {
        let representable = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(milliseconds))
            .is_some();
        assert_eq!(
            validate_deadline(milliseconds, "timer").is_ok(),
            representable
        );
    }
}

/// The Pong wait is bounded on its own. It may equal or fall below the
/// keepalive interval: no relation ties the two.
#[xmtp_common::test(unwrap_try = true)]
fn the_pong_wait_need_not_exceed_the_keepalive_interval() {
    for pong_wait_ms in [30_000, 1] {
        let config = ValidatedConfig::load_str(&format!(
            "{MINIMAL}[streams]\nkeepalive_interval_ms = 30000\nmax_pong_wait_ms = {pong_wait_ms}\n"
        ))?;
        assert_eq!(config.streams.max_pong_wait_ms, pong_wait_ms);
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
    config.clone().validate()?;
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
        assert_invalid(
            config.clone().validate(),
            &format!("telemetry.resource_attributes.\"{key}\""),
        );
    }
    for endpoint in [
        "secret-invalid",
        "ftp://secret",
        "http://[secret",
        "http://host/secret value",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.telemetry.otlp_endpoint = Some(endpoint.into());
        let error = config.clone().validate().unwrap_err().to_string();
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
        assert!(config.clone().validate().is_err());
    }
    let bad = write_config(
        "telemetry-endpoint",
        &format!("{MINIMAL}\n[telemetry]\notlp_endpoint = 'env:PATH'"),
    );
    let error = ValidatedConfig::load(&bad.0).unwrap_err().to_string();
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
    let config = ValidatedConfig::load(&file.0)?;
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
    let config = ValidatedConfig::load_str(&format!(
        "{MINIMAL}[telemetry]\nservice_name = 'env:{NESTED}'\n[auth]\nenabled = true\njwks_url = 'https://issuer.example/keys'\naudiences = ['env:{ELEMENT}', 'literal']\n"
    ))?;
    assert_eq!(config.telemetry.service_name, format!("env:{TARGET}"));
    assert_eq!(
        config.into_config().auth?.audiences?,
        vec!["xmtp.example.org".to_owned(), "literal".to_owned()]
    );
    let error = ValidatedConfig::load_str(&format!(
        "{MINIMAL}[telemetry]\nservice_name = 'env:{NOT_UNICODE}'\n"
    ))
    .unwrap_err();
    assert!(
        matches!(&error, ConfigError::Environment { name, .. } if name == NOT_UNICODE),
        "{error}"
    );
    assert_eq!(error.field(), Some("telemetry.service_name"), "{error}");
}

#[xmtp_common::test(unwrap_try = true)]
fn inline_configuration_resolves_environment_references() {
    let config = ValidatedConfig::load_str(&format!(
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
    let error = ValidatedConfig::load_str("secret-inline-sentinel = [").unwrap_err();
    assert!(matches!(error, ConfigError::Parse));
    assert!(!format!("{error} {error:?}").contains("secret-inline-sentinel"));
}

/// A deployment that does not name itself cannot be bound to by a client
/// database, so an unnamed or malformed identifier stops the process.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-002
fn the_identifier_is_required_and_shaped() {
    let missing: Config = toml::from_str("[database]\nurl = 'postgres://localhost/xmtp'\n")?;
    assert_invalid(missing.validate(), "server.identifier");
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
        assert_invalid(config.clone().validate(), "server.identifier");
    }
    // The documented convention is accepted at the byte bound.
    let mut config: Config = toml::from_str(MINIMAL)?;
    config.server.identifier = Some("a".repeat(MAX_SERVER_IDENTIFIER_BYTES));
    config.clone().validate()?;
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
    assert_invalid(config.clone().validate(), "auth.enabled");
    // Disabled auth ignores every other field, including ones that would fail
    // their own checks. The backend publishes an empty summary for it.
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\naudiences = []\nissuers = []\n"
    ))?;
    assert!(!config.validate()?.auth_enabled());
    // Ignoring a field is not the same as never reading it: environment
    // references resolve for the whole document before any section is
    // validated, so a missing variable in a disabled section still fails.
    let error = ValidatedConfig::load_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\njwks_url = 'env:XMTP_AUTH_DISABLED_MISSING'\n"
    ))
    .unwrap_err();
    assert!(
        matches!(&error, ConfigError::Environment { name, .. } if name == "XMTP_AUTH_DISABLED_MISSING"),
        "{error}"
    );
    assert_eq!(error.field(), Some("auth.jwks_url"), "{error}");
}

/// An operator states a minimum client version as a semantic version, or
/// states none and admits every client version.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-067
fn the_minimum_client_version_is_optional_and_semantic() {
    for version in ["1", "1.2", "v1.2.3", "latest", "1.2.3.4", ""] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.server.min_libxmtp_version = Some(version.into());
        assert_invalid(config.clone().validate(), "server.min_libxmtp_version");
    }
    let config: Config = toml::from_str(MINIMAL)?;
    config.clone().validate()?;
    assert!(config.server.min_libxmtp_version.is_none());
    let mut config: Config = toml::from_str(MINIMAL)?;
    config.server.min_libxmtp_version = Some("1.2.3-beta.1".into());
    config.clone().validate()?;
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
        config.clone().validate()?;
        match field {
            "limits.max_request_bytes" => config.limits.max_request_bytes += 1,
            _ => config.limits.max_response_bytes += 1,
        }
        assert_invalid(config.clone().validate(), field);
    }
}

/// An absent minimum name length takes the default of 3. The two ends of the
/// range start, and 0 and 28 refuse to start and name the key.
// verifies: CONF-065, CONF-080
#[xmtp_common::test(unwrap_try = true)]
fn handles_min_name_length_defaults_and_range() {
    let config = ValidatedConfig::load_str(MINIMAL)?;
    assert_eq!(config.handles.min_name_length, 3);
    for length in [1, 27] {
        let config = ValidatedConfig::load_str(&format!(
            "{MINIMAL}\n[handles]\nmin_name_length = {length}"
        ))?;
        assert_eq!(config.handles.min_name_length, length);
    }
    for length in [0, 28] {
        assert_invalid(
            ValidatedConfig::load_str(&format!("{MINIMAL}\n[handles]\nmin_name_length = {length}")),
            "handles.min_name_length",
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
    /// The key that the refusal names.
    field: String,
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
        field: key.to_owned(),
    }
}

/// A value for the key `name` in `table`, for a row whose key has a position
/// in it, such as `auth.keys[].kid`. The refusal names `field`.
fn failing_name(
    key: &'static str,
    check: &'static str,
    table: &'static str,
    name: &str,
    text: &str,
    field: &str,
) -> FailingValue {
    FailingValue {
        table,
        name: name.to_owned(),
        field: field.to_owned(),
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
            "chains.\"eip155:1\"",
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
            "auth.keys[0].kid",
        )),
        with_inline_keys(failing_name(
            "auth.keys[].kid",
            "bytes 1..256",
            "auth.keys[]",
            "kid",
            &format!("'{}'", "k".repeat(257)),
            "auth.keys[0].kid",
        )),
        with_inline_keys(failing_name(
            "auth.keys[].alg",
            "one of",
            "auth.keys[]",
            "alg",
            "'HS256'",
            "auth.keys[0].alg",
        )),
        failing_name(
            "auth.api_keys.<name>",
            "key pattern ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$",
            "auth.api_keys",
            "-bot",
            "'fedcba9876543210fedcba9876543210'",
            "auth.api_keys.-bot",
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
            &format!("telemetry.resource_attributes.\"{name}\""),
        ));
    }
    // Both bounds of the published count limits and the capped timers.
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
        cases.push(failing(key, COUNT, "4294967296"));
    }
    cases.push(failing(
        "auth.jwks_max_stale_seconds",
        "range 0..4294967295",
        "4294967296",
    ));
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
        let name = key.rsplit_once('.').expect("a key below the root").1;
        cases.push(failing_name(
            key,
            "one of: allow, deny, allow_if_admin, allow_if_super_admin, allow_if_self_or_non_member",
            "application_components[]",
            name,
            "'maybe'",
            &format!("application_components[0].{name}"),
        ));
    }
    cases
}

/// Every standard check the field table names refuses a value that fails it,
/// and the refusal names the key. Inside a disabled `[auth]` table, the same
/// values start.
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
        .collect();
    assert_eq!(
        named, covered,
        "the field table and the cases name different checks"
    );

    for auth in [AuthSource::JwksUrl, AuthSource::InlineKeys] {
        ValidatedConfig::load_str(&toml::to_string(&full_document(auth))?)?;
    }
    for case in &cases {
        let mut document = full_document(case.auth);
        table_at(&mut document, case.table).insert(case.name.clone(), case.value.clone());
        let text = toml::to_string(&document)?;
        match ValidatedConfig::load_str(&text) {
            Err(error) => assert_eq!(
                error.field(),
                Some(case.field.as_str()),
                "{} = {} must fail {} naming its key: {error}",
                case.key,
                case.value,
                case.check
            ),
            Ok(_) => panic!("{} = {} must fail {}", case.key, case.value, case.check),
        }
        if case.key.starts_with("auth.") {
            table_at(&mut document, "auth").insert("enabled".into(), false.into());
            if let Err(error) = ValidatedConfig::load_str(&toml::to_string(&document)?) {
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
    let minimal = ValidatedConfig::load_str(REQUIRED)?;
    let tables = ValidatedConfig::load_str(&format!(
        "{REQUIRED}[auth]\nenabled = false\n[push.http]\n[attachments]\nbase_url = 'https://files.example.org'\n[attachments.target.S3]\nendpoint = 'https://s3.example.org'\nregion = 'us-east-1'\nbucket = 'attachments'\n[attachments.target.S3.credentials]\nkind = 'web_identity'\n"
    ))?;
    let mut tables_loaded = serde_json::to_value(&*tables)?;
    // The target applies its presign lifetime default when it signs.
    let Some(attachments::TargetConfig::S3(s3)) = tables
        .attachments
        .as_ref()
        .map(|attachments| &attachments.target)
    else {
        panic!("the attachments table loaded without its target");
    };
    tables_loaded["attachments"]["target"]["S3"]["presign_ttl_seconds"] =
        s3.presign_ttl_seconds().into();
    let loaded = [serde_json::to_value(&*minimal)?, tables_loaded];

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
        ValidatedConfig::load_str(&format!(
            "{MINIMAL}[chains]\n\"{chain}\" = 'https://rpc.example.org'\n"
        ))
    };
    for chain in [
        "eip155:01",
        "eip155:+1",
        "eip155:18446744073709551616",
        "cosmos:cosmoshub-4",
    ] {
        assert_invalid(load(chain), &format!("chains.\"{chain}\""));
    }
    for chain in ["eip155:0", "eip155:1", "eip155:18446744073709551615"] {
        let config = load(chain)?;
        assert!(config.chains.contains_key(chain), "{chain}");
    }
}

/// A refusal caused by one key or value names that key as a TOML dotted key,
/// with a fixed reason, and never the value. A typed-decoding refusal has one
/// of four reasons, chosen by the decoder's message, so a decoder update that
/// changes those messages fails here.
// verifies: CONF-065
#[xmtp_common::test(unwrap_try = true)]
fn every_refusal_names_its_key() {
    const MISSING: &str = "XMTP_CONFIG_REFUSAL_MISSING_7C2D";
    const REQUIRED_SERVER: &str = "[server]\nidentifier = 'org.xmtp.test'\n";
    let s3 = |table: &str, credentials: &str| {
        format!(
            "{MINIMAL}[attachments]\nbase_url = 'https://files.example.org'\n\
             [attachments.target.S3]\nendpoint = 'https://s3.example.org'\n\
             region = 'us-east-1'\nbucket = 'attachments'\n{table}\
             [attachments.target.S3.credentials]\n{credentials}\n"
        )
    };
    let s3_line = |line: &str| {
        format!(
            "{MINIMAL}[attachments]\nbase_url = 'https://files.example.org'\n\
             [attachments.target.S3]\nendpoint = 'https://s3.example.org'\n\
             region = 'us-east-1'\nbucket = 'attachments'\n{line}\n"
        )
    };
    let component = |id: u32, name: &str, in_groups: bool| {
        format!(
            "[[application_components]]\ncomponent_id = {id}\nname = '{name}'\n\
             component_type = 'string'\ninsert_policy = 'allow'\nupdate_policy = 'allow'\n\
             delete_policy = 'allow'\nin_groups = {in_groups}\nin_dms = false\n"
        )
    };
    let auth = |keys: &str| format!("{MINIMAL}[auth]\nenabled = true\n{keys}\n");
    let key =
        |kid: &str, alg: &str| format!("{{ kid = '{kid}', alg = '{alg}', public_key = 'k' }}");
    let cases: Vec<(String, &str, String)> = vec![
        // Typed decoding, one case for each of the four reasons.
        (
            format!("{MINIMAL}[limits]\nmax_query_limit = 'many'\n"),
            "limits.max_query_limit",
            "has the wrong type or value".into(),
        ),
        (
            format!("{MINIMAL}unlisted = true\n"),
            "server.unlisted",
            "is not a configuration key".into(),
        ),
        (
            format!("{REQUIRED_SERVER}[database]\n"),
            "database.url",
            "is required".into(),
        ),
        (
            format!("{MINIMAL}log_level = 'verbose'\n"),
            "server.log_level",
            "is not one of the allowed values".into(),
        ),
        // Decoding paths through a quoted key, an array entry, and an enum variant.
        (
            format!("\"odd key\" = 1\n{MINIMAL}"),
            "\"odd key\"",
            "is not a configuration key".into(),
        ),
        (
            format!("{MINIMAL}[auth]\nenabled = false\naudiences = ['a', 3]\n"),
            "auth.audiences[1]",
            "has the wrong type or value".into(),
        ),
        (
            format!("{MINIMAL}{}", component(0x1_C000, "a", true)),
            "application_components[0].component_id",
            "has the wrong type or value".into(),
        ),
        (
            s3("unlisted = 1\n", "kind = 'web_identity'"),
            "attachments.target.S3.unlisted",
            "is not a configuration key".into(),
        ),
        (
            format!(
                "{MINIMAL}[attachments]\nbase_url = 'https://files.example.org'\n\
                 [attachments.target.GCS]\nbucket = 'attachments'\n"
            ),
            "attachments.target.GCS",
            "is not a configuration key".into(),
        ),
        // A table or a string written in another shape.
        (
            format!(
                "{MINIMAL}[attachments]\nbase_url = 'https://files.example.org'\n\
                 [[attachments.target]]\nendpoint = 'https://s3.example.org'\n\
                 region = 'us-east-1'\nbucket = 'attachments'\n\
                 credentials = {{ kind = 'instance' }}\n"
            ),
            "attachments.target",
            "has the wrong type or value".into(),
        ),
        (
            format!(
                "{MINIMAL}[attachments]\nbase_url = 'https://files.example.org'\n\
                 target = [{{ endpoint = 'https://s3.example.org', region = 'us-east-1', \
                 bucket = 'attachments', credentials = {{ kind = 'instance' }} }}]\n"
            ),
            "attachments.target",
            "has the wrong type or value".into(),
        ),
        (
            s3_line("credentials = ['instance']"),
            "attachments.target.S3.credentials",
            "has the wrong type or value".into(),
        ),
        (
            s3_line("credentials = { kind = { instance = {} } }"),
            "attachments.target.S3.credentials.kind",
            "has the wrong type or value".into(),
        ),
        (
            s3_line(
                "credentials = { kind = { static = [] }, access_key_id = 'a', secret_access_key = 's' }",
            ),
            "attachments.target.S3.credentials.kind",
            "has the wrong type or value".into(),
        ),
        (
            s3_line("credentials = ['bogus']"),
            "attachments.target.S3.credentials",
            "has the wrong type or value".into(),
        ),
        // A date or a time, which no key takes.
        (
            "[server]\nidentifier = 1979-05-27\n[database]\nurl = 'postgres://localhost/xmtp'\n"
                .into(),
            "server.identifier",
            "has the wrong type or value".into(),
        ),
        (
            s3("", "kind = 'profile'\nname = 07:32:00"),
            "attachments.target.S3.credentials.name",
            "has the wrong type or value".into(),
        ),
        (
            format!("{MINIMAL}[auth]\nenabled = false\naudiences = [1979-05-27]\n"),
            "auth.audiences[0]",
            "has the wrong type or value".into(),
        ),
        // A key that holds a backtick, which the decoder's message also uses.
        (
            format!("\"a`b\" = 1\n{MINIMAL}"),
            "\"a`b\"",
            "is not a configuration key".into(),
        ),
        (
            format!("{MINIMAL}[streams]\n\"x`y\" = 1\n"),
            "streams.\"x`y\"",
            "is not a configuration key".into(),
        ),
        (
            s3("", "kind = 'instance'\n\"q`r\" = 1"),
            "attachments.target.S3.credentials.\"q`r\"",
            "is not a configuration key".into(),
        ),
        // A credential key that the kind does not take, or requires.
        (
            s3(
                "",
                "kind = 'static'\naccess_key_id = 'a'\nsecret_access_key = 's'\nrole_arn = 'r'",
            ),
            "attachments.target.S3.credentials.role_arn",
            "is not a configuration key".into(),
        ),
        (
            s3("", "kind = 'profile'"),
            "attachments.target.S3.credentials.name",
            "is required".into(),
        ),
        // Environment references name the key and the variable.
        (
            format!("{REQUIRED_SERVER}[database]\nurl = 'env:{MISSING}'\n"),
            "database.url",
            format!("environment variable {MISSING} is not available"),
        ),
        (
            format!("{REQUIRED_SERVER}[database]\nurl = 'env:'\n"),
            "database.url",
            "environment variable name is empty".into(),
        ),
        (
            format!("{MINIMAL}[auth]\nenabled = false\naudiences = ['a', 'env:{MISSING}']\n"),
            "auth.audiences[1]",
            format!("environment variable {MISSING} is not available"),
        ),
        (
            format!(
                "{MINIMAL}[telemetry.resource_attributes]\n\"deployment.environment\" = 'env:{MISSING}'\n"
            ),
            "telemetry.resource_attributes.\"deployment.environment\"",
            format!("environment variable {MISSING} is not available"),
        ),
        // A key the operator chooses.
        (
            format!("{MINIMAL}[chains]\n\"cosmos:hub\" = 'https://rpc.example.org'\n"),
            "chains.\"cosmos:hub\"",
            "keys must be EIP-155 CAIP-2 chain ids".into(),
        ),
        (
            format!("{MINIMAL}[chains]\n\"eip155:1\" = 'ftp://rpc.example.org'\n"),
            "chains.\"eip155:1\"",
            "must be a non-empty URL".into(),
        ),
        (
            format!("{MINIMAL}[telemetry.resource_attributes]\n\"service.name\" = 'x'\n"),
            "telemetry.resource_attributes.\"service.name\"",
            "service.name, service.version, and xmtp.backend.identifier are reserved".into(),
        ),
        (
            auth("[auth.api_keys]\n\"ci.bot\" = 'short'"),
            "auth.api_keys.\"ci.bot\"",
            "value must be 32 to 8192 bytes".into(),
        ),
        // A rule that relates two keys names one; its reason names the other.
        (
            auth(""),
            "auth.enabled",
            "set api_keys, jwks_url, or keys".into(),
        ),
        (
            auth(&format!(
                "jwks_url = 'https://issuer.example/keys'\nkeys = [{}]",
                key("a", "ES256")
            )),
            "auth.keys",
            "must not be set together with auth.jwks_url".into(),
        ),
        (
            auth("jwks_url = 'http://issuer.example/keys'"),
            "auth.jwks_url",
            "must be HTTPS or HTTP on a loopback host".into(),
        ),
        (
            auth(&format!("keys = [{}]", key("a", "HS256"))),
            "auth.keys[0].alg",
            "alg must be a supported asymmetric algorithm".into(),
        ),
        (
            auth(&format!(
                "keys = [{}, {}]",
                key("a", "ES256"),
                key("a", "ES256")
            )),
            "auth.keys[1].kid",
            "kid must be non-empty, unique, and at most 256 bytes".into(),
        ),
        // A check that once named only its table.
        (
            format!(
                "{MINIMAL}[retention]\nwelcome_seconds = {}\n",
                MAX_RETENTION_SECONDS + 1
            ),
            "retention.welcome_seconds",
            "duration is too large for nanosecond expiry arithmetic".into(),
        ),
        (
            format!("{MINIMAL}[publishing]\nmax_barrier_wait_ms = 2147483648\n"),
            "publishing.max_barrier_wait_ms",
            "exceeds the Postgres timeout range".into(),
        ),
        // Each application component key.
        (
            format!("{MINIMAL}{}", component(0xBFFF, "a", true)),
            "application_components[0].component_id",
            "component_id must be in 0xC000-0xFEFF".into(),
        ),
        (
            format!("{MINIMAL}{}", component(0xC000, "", true)),
            "application_components[0].name",
            "name must be 1 to 100 UTF-8 bytes".into(),
        ),
        (
            format!("{MINIMAL}{}", component(0xC000, "a", false)),
            "application_components[0].in_groups",
            "in_groups or in_dms must be true".into(),
        ),
        (
            format!(
                "{MINIMAL}{}{}",
                component(0xC000, "a", true),
                component(0xC000, "b", true)
            ),
            "application_components[1].component_id",
            "component_id repeats an earlier definition".into(),
        ),
        (
            format!(
                "{MINIMAL}{}{}",
                component(0xC000, "a", true),
                component(0xC001, "a", true)
            ),
            "application_components[1].name",
            "name repeats an earlier definition".into(),
        ),
        (
            format!("{MINIMAL}{}", component(0xC000, "USER_DISPLAY_NAME", true)),
            "application_components[0].name",
            "name must not equal a well-known component name".into(),
        ),
    ];
    let loaded: Vec<&str> = cases
        .iter()
        .filter(|(document, ..)| ValidatedConfig::load_str(document).is_ok())
        .map(|(_, field, _)| *field)
        .collect();
    assert!(loaded.is_empty(), "these must be refused: {loaded:?}");
    for (document, field, reason) in &cases {
        let error = ValidatedConfig::load_str(document).unwrap_err();
        assert_eq!(error.field(), Some(*field), "{document}");
        assert_eq!(
            error.to_string(),
            format!("configuration is invalid: {field} ({reason})"),
            "{document}"
        );
    }
}

/// A key name parses back, as a TOML dotted key, to the name it was built
/// from. Only a name that is not a bare key is quoted. An array entry is `[N]`.
#[xmtp_common::test(unwrap_try = true)]
fn key_paths_are_toml_dotted_keys() {
    for (name, bare) in [
        ("plain_Name-1", true),
        ("eip155:1", false),
        ("service.name", false),
        ("", false),
        ("ébot", false),
        ("quote\"back\\slash", false),
        ("line\nbreak\ttab\u{7f}", false),
    ] {
        let path = key_path("table", Step::Key(name));
        assert_eq!(path == format!("table.{name}"), bare, "{path}");
        let document: toml::Table = toml::from_str(&format!("{path} = 1"))?;
        assert_eq!(document["table"][name].as_integer(), Some(1), "{path}");
    }
    assert_eq!(key_path("auth.keys", Step::Entry(3)), "auth.keys[3]");
    assert_eq!(key_path("", Step::Key("server")), "server");
}
