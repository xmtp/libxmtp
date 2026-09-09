use serde_json::{Value, json};

use crate::config::Config;

/// Use the published artifact and standard format assertions, as a schema consumer does.
fn validator() -> jsonschema::Validator {
    let schema: Value =
        serde_json::from_str(include_str!("../../../../../docs/schemas/backend-v1.json"))
            .expect("published schema");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("valid schema")
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_checks_logging_levels_and_switch() {
    let validator = validator();
    for level in [
        "off",
        "error",
        "warn",
        "info",
        "debug",
        "trace",
        "env:LOG_LEVEL",
    ] {
        assert!(validator.is_valid(&json!({"database": {"url": "env:DB"}, "server": {"log_level": level, "request_logger": false}})));
    }
    assert!(
        !validator
            .is_valid(&json!({"database": {"url": "env:DB"}, "server": {"log_level": "verbose"}}))
    );
    assert!(
        !validator.is_valid(
            &json!({"database": {"url": "env:DB"}, "server": {"request_logger": "false"}})
        )
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_accepts_the_example_and_rejects_unknown_keys() {
    let validator = validator();
    let example: toml::Value =
        toml::from_str(include_str!("../../../../../dev/backend/local.toml"))?;
    let example = serde_json::to_value(example)?;
    assert!(validator.is_valid(&example));
    for section in [
        "",
        "server",
        "database",
        "publishing",
        "streams",
        "retention",
        "validation",
        "limits",
    ] {
        let mut instance = example.clone();
        if section.is_empty() {
            instance["unknown"] = json!(1);
        } else {
            instance[section]["unknown"] = json!(1);
        }
        assert!(!validator.is_valid(&instance), "unknown key in {section}");
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_enforces_every_numeric_scalar_range() {
    let validator = validator();
    let config: Config = toml::from_str("[database]\nurl = 'postgres://localhost/xmtp'")?;
    let baseline = serde_json::to_value(config)?;
    assert!(validator.is_valid(&baseline));
    for (section, properties) in baseline.as_object()? {
        for (field, value) in properties.as_object()? {
            if !value.is_number() {
                continue;
            }
            let maximum = match (section.as_str(), field.as_str()) {
                ("database", "max_connections")
                | ("streams", "keepalive_interval_ms")
                | (
                    "limits",
                    "max_http2_streams"
                    | "max_update_frames_per_second"
                    | "max_update_burst"
                    | "max_ping_frames_per_second"
                    | "max_ping_burst",
                ) => u32::MAX as u64,
                ("database", "max_statement_timeout_ms") | ("publishing", _) => i32::MAX as u64,
                ("retention", _) => i64::MAX as u64 / 1_000_000_000,
                ("limits", "max_query_limit" | "default_query_limit") => i64::MAX as u64 - 1,
                _ => u64::MAX,
            };
            for (value, accepted) in [
                (json!(1), true),
                (json!(maximum), true),
                (json!(0), false),
                (json!(-1), false),
                (json!(1.5), false),
                (
                    serde_json::from_str(&(u128::from(maximum) + 1).to_string())?,
                    false,
                ),
            ] {
                let mut instance = baseline.clone();
                instance[section][field] = value.clone();
                assert_eq!(
                    validator.is_valid(&instance),
                    accepted,
                    "{section}.{field} = {value}"
                );
            }
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_checks_socket_addresses_with_the_runtime_parser() {
    let validator = validator();
    for address in [
        "0.0.0.0:0",
        "255.255.255.255:65535",
        "127.0.0.1:0005050",
        "[::]:0",
        "[::1]:5050",
        "[1:2:3:4:5:6:7:8]:65535",
        "[1:2:3:4:5:6:192.0.2.1]:9",
        "[::ffff:192.0.2.1]:80",
        "[1:2::192.0.2.1%4294967295]:80",
        "[fe80::1%0002]:00080",
        "",
        "localhost:5050",
        "256.0.0.1:1",
        "01.2.3.4:1",
        "1.2.3:1",
        "127.0.0.1:65536",
        "127.0.0.1:-1",
        "127.0.0.1:+1",
        "[1:2:3:4:5:6:7]:1",
        "[1:2:3:4:5:6:7:8:9]:1",
        "[1:2:3:4:5:6:7::8]:1",
        "[1::2::3]:1",
        "[::gg]:1",
        "[::ffff:256.1.2.3]:1",
        "[1:2:3:4:5::192.0.2.1]:1",
        "[1:2:3:4:5:6::192.0.2.1]:1",
        "[::1%4294967296]:1",
        "[::1%eth0]:1",
        "[::1%]:1",
        "::1:80",
        "127.0.0.1:80\n",
    ] {
        let instance = json!({"database": {"url": "postgres://localhost/xmtp"}, "server": {"listen": address}});
        assert_eq!(
            validator.is_valid(&instance),
            address.parse::<std::net::SocketAddr>().is_ok(),
            "{address:?}"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_checks_urls_chain_keys_and_environment_references() {
    let validator = validator();
    for (section, field, accepted, rejected) in [
        (
            "database",
            "url",
            vec![
                "postgres://localhost/xmtp",
                "postgresql://user:pass@host/db",
                "env:DATABASE_URL",
            ],
            vec![
                "",
                "env:",
                "env:BAD=NAME",
                "env:BAD\0NAME",
                "https://host/db",
                "postgres://[bad]/db",
                "postgres://host/bad%ZZ",
            ],
        ),
        (
            "database",
            "replica_url",
            vec!["postgres://localhost/replica", "env:REPLICA_URL"],
            vec!["", "env:", "mysql://localhost/db"],
        ),
        ("server", "listen", vec!["env:LISTEN"], vec!["env:"]),
        (
            "chains",
            "eip155:1",
            vec![
                "https://rpc.example/path?key=value",
                "http://localhost:8545",
                "env:RPC_URL",
            ],
            vec![
                "",
                "env:",
                "ftp://host",
                "https://",
                "https://[bad]",
                "https://host/bad%ZZ",
            ],
        ),
    ] {
        for (values, expected) in [(accepted, true), (rejected, false)] {
            for value in values {
                let mut instance = json!({"database": {"url": "postgres://localhost/xmtp"}});
                instance[section][field] = json!(value);
                assert_eq!(
                    validator.is_valid(&instance),
                    expected,
                    "{section}.{field} = {value}"
                );
            }
        }
    }
    for chain in [
        "eip155:0",
        "eip155:1",
        "eip155:+001",
        "eip155:18446744073709551615",
        "eip155:18446744073709551616",
        "eip155:-1",
        "eip155:",
        "cosmos:1",
        "eip155:1\n",
    ] {
        let instance = json!({"database": {"url": "postgres://localhost/xmtp"}, "chains": {chain: "https://rpc.example"}});
        let runtime = xmtp_id::associations::AccountId::new(chain.to_owned(), "0x0".to_owned())
            .get_chain_id_u64()
            .is_ok();
        assert_eq!(validator.is_valid(&instance), runtime, "{chain:?}");
    }
    assert!(validator.is_valid(&json!({"database": {"url": "env:DB", "replica_url": null}})));
}

#[xmtp_common::test(unwrap_try = true)]
fn keepalive_interval_fits_the_started_wire_field() {
    let mut config: Config = toml::from_str("[database]\nurl = 'postgres://localhost/xmtp'")?;
    config.streams.keepalive_interval_ms = u32::MAX as u64;
    config.streams.max_pong_wait_ms = u32::MAX as u64 + 2;
    config.validate()?;
    config.streams.keepalive_interval_ms += 1;
    assert!(config.validate().is_err());
}
