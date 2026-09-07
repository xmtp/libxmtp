use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn logging_defaults_and_overrides_are_typed() {
    let config: Config = toml::from_str(MINIMAL)?;
    assert!(matches!(config.server.log_level, LogLevel::Info));
    assert!(config.server.request_logger);
    for level in ["off", "error", "warn", "info", "debug", "trace"] {
        let config: Config = toml::from_str(&format!(
            "{MINIMAL}\n[server]\nlog_level = '{level}'\nrequest_logger = false"
        ))?;
        let shared: xmtp_logging::Level = config.server.log_level.into();
        assert_eq!(shared.as_str(), level);
        assert!(!config.server.request_logger);
    }
    assert!(
        toml::from_str::<Config>(&format!("{MINIMAL}\n[server]\nlog_level = 'verbose'")).is_err()
    );
}
const MINIMAL: &str = "[database]\nurl = 'postgres://localhost/xmtp'\n";
const RESPONSE_TEST_ENVELOPE_BYTES: usize = 1_000_000;
const RESPONSE_TEST_REQUEST_BYTES: usize = 2_000_000;

#[xmtp_common::test(unwrap_try = true)]
fn minimal_configuration_uses_defaults() {
    let config: Config = toml::from_str(MINIMAL)?;
    config.validate()?;

    assert_eq!(config.server.listen, DEFAULT_LISTEN);
    assert_eq!(
        config.limits.max_request_bytes,
        BACKEND_DEFAULT_MAX_REQUEST_BYTES
    );
    assert!(config.chains.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
fn unknown_keys_are_rejected() {
    let error =
        toml::from_str::<Config>("[database]\nurl = 'postgres://localhost/xmtp'\nextra = true\n")
            .expect_err("unknown keys must fail");
    assert!(error.to_string().contains("unknown field"));
}

#[xmtp_common::test(unwrap_try = true)]
fn framing_bound_covers_maximum_metadata_and_nested_messages() {
    use crate::api;
    use prost::Message;

    const STORED_TOPIC_BYTES: usize = 128;
    const HASH_BYTES: usize = 32;
    let meta = api::EnvelopeMeta {
        cursor: Some(api::Cursor {
            sequence_id: u64::MAX,
        }),
        server_ns: u64::MAX,
        message_hash: Some(api::MessageHash {
            hash: Some(api::message_hash::Hash::Sha256(vec![0; HASH_BYTES])),
        }),
        topic: Some(api::Topic {
            topic: vec![0; STORED_TOPIC_BYTES],
        }),
        expiry_ns: u64::MAX,
        is_commit_or_proposal: true,
    };
    assert_eq!(meta.encoded_len(), MAX_METADATA_BYTES);
    for size in [
        0,
        127,
        128,
        16_383,
        16_384,
        BACKEND_DEFAULT_MAX_ENVELOPE_BYTES,
    ] {
        let envelope = api::ClientEnvelope {
            payload: Some(api::client_envelope::Payload::GroupMessage(
                api::GroupMessage {
                    data: vec![0; size],
                    ..Default::default()
                },
            )),
        };
        let bound = envelope.encoded_len() + ENVELOPE_METADATA_AND_FRAMING_BYTES;
        let response = api::SubscribeResponse {
            response: Some(api::subscribe_response::Response::Messages(
                api::subscribe_response::Messages {
                    envelopes: vec![api::ServerEnvelope {
                        meta: Some(meta.clone()),
                        envelope: Some(envelope),
                    }],
                },
            )),
        };
        assert!(response.encoded_len() + GRPC_HEADER_BYTES <= bound);
    }
}

#[xmtp_common::test(unwrap_try = true)]
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
        serde_json::from_str(include_str!("../../../../docs/schemas/backend-v1.json"))?;
    assert_eq!(generated, published);
    assert_eq!(generated["$id"], SCHEMA_ID);
}

#[xmtp_common::test(unwrap_try = true)]
fn environment_reference_returns_variable_value() {
    let path = std::env::var("PATH")?;
    assert_eq!(resolve_env("env:PATH")?, path);
}

#[xmtp_common::test(unwrap_try = true)]
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
        let representable = xmtp_common::time::Instant::now()
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
