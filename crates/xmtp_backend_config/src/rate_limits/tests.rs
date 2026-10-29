use std::net::IpAddr;

use super::*;
use crate::{ValidatedConfig, tests::assert_invalid};

const MINIMAL: &str =
    "[server]\nidentifier = 'org.xmtp.test'\n[database]\nurl = 'postgres://localhost/xmtp'\n";

fn load(rate_limits: &str) -> Result<ValidatedConfig, ConfigError> {
    ValidatedConfig::load_str(&format!("{MINIMAL}{rate_limits}"))
}

/// An override for `caller`, written as TOML keys, that limits `request`.
fn caller_override(caller: &str) -> String {
    format!(
        "[[rate_limits.overrides]]\n{caller}\n[rate_limits.overrides.limits]\n\
         request = [{{ count = 1, window_seconds = 1 }}]\n"
    )
}

fn ip(text: &str) -> CallerKey {
    CallerKey::from_ip(text.parse::<IpAddr>().expect("an IP address"))
}

/// Each list or override rule refuses the file and names the key, and the file
/// starts at each clause's boundary.
// verifies: CONF-094
#[xmtp_common::test(unwrap_try = true)]
fn lists_and_overrides_that_disagree_are_refused() {
    let limit = |window: u32| format!("{{ count = 10, window_seconds = {window} }}");
    let three = [limit(1), limit(60), limit(3600)].join(", ");
    let four = [limit(1), limit(60), limit(3600), limit(86400)].join(", ");
    load(&format!(
        "[rate_limits.categories.publish]\nip = [{three}]\n\
         [[rate_limits.overrides]]\napi_key = 'bot'\n[rate_limits.overrides.limits]\n\
         publish = [{three}]\n"
    ))?;
    for (document, field) in [
        // More than three limits in a list.
        (
            format!("[rate_limits.categories.publish]\nip = [{four}]\n"),
            "rate_limits.categories.publish.ip",
        ),
        (
            format!(
                "[[rate_limits.overrides]]\napi_key = 'bot'\n[rate_limits.overrides.limits]\n\
                 publish = [{four}]\n"
            ),
            "rate_limits.overrides[0].limits.publish",
        ),
        // Two limits with one window.
        (
            format!(
                "[rate_limits.categories.request]\ndefault = [{}, {}]\n",
                limit(60),
                limit(60)
            ),
            "rate_limits.categories.request.default[1].window_seconds",
        ),
        (
            format!(
                "[[rate_limits.overrides]]\nip = '192.0.2.1'\n[rate_limits.overrides.limits]\n\
                 upload = [{}, {}, {}]\n",
                limit(60),
                limit(3600),
                limit(60)
            ),
            "rate_limits.overrides[0].limits.upload[2].window_seconds",
        ),
        // No caller, or more than one.
        (caller_override(""), "rate_limits.overrides[0]"),
        (
            caller_override("api_key = 'bot'\nip = '192.0.2.1'"),
            "rate_limits.overrides[0]",
        ),
        (
            caller_override("api_key = 'bot'\njwt_subject = 'user'"),
            "rate_limits.overrides[0]",
        ),
        (
            caller_override("jwt_subject = 'user'\nip = '2001:db8::1'"),
            "rate_limits.overrides[0]",
        ),
        // An issuer without a subject.
        (
            caller_override("jwt_issuer = 'https://auth.example.org'"),
            "rate_limits.overrides[0].jwt_issuer",
        ),
        (
            caller_override("jwt_issuer = 'https://auth.example.org'\napi_key = 'bot'"),
            "rate_limits.overrides[0].jwt_issuer",
        ),
        // Two overrides for one caller key.
        (
            caller_override("api_key = 'bot'") + &caller_override("api_key = 'bot'"),
            "rate_limits.overrides[1]",
        ),
        (
            caller_override("jwt_issuer = 'https://a.example'\njwt_subject = 'user'")
                + &caller_override("jwt_issuer = 'https://a.example'\njwt_subject = 'user'"),
            "rate_limits.overrides[1]",
        ),
        // A token without `iss` has the empty issuer.
        (
            caller_override("jwt_subject = 'user'")
                + &caller_override("jwt_issuer = ''\njwt_subject = 'user'"),
            "rate_limits.overrides[1]",
        ),
        // One IPv6 /64, and an IPv4 address and its IPv4-mapped form.
        (
            caller_override("ip = '2001:db8:0:1::1'")
                + &caller_override("ip = '2001:db8:0:1:ffff:ffff:ffff:ffff'"),
            "rate_limits.overrides[1]",
        ),
        (
            caller_override("ip = '192.0.2.1'") + &caller_override("ip = '::ffff:192.0.2.1'"),
            "rate_limits.overrides[1]",
        ),
    ] {
        assert_invalid(load(&document), field);
    }
    // Different caller keys: other issuers, other /64s, other auth types.
    load(
        &[
            caller_override("jwt_issuer = 'https://a.example'\njwt_subject = 'user'"),
            caller_override("jwt_issuer = 'https://b.example'\njwt_subject = 'user'"),
            caller_override("jwt_subject = 'user'"),
            caller_override("ip = '2001:db8:0:1::1'"),
            caller_override("ip = '2001:db8:0:2::1'"),
            caller_override("ip = '192.0.2.1'"),
            caller_override("ip = '::192.0.2.1'"),
            caller_override("api_key = 'user'"),
        ]
        .concat(),
    )?;
}

/// An override needs its `limits` table, and a limit needs both keys.
#[xmtp_common::test(unwrap_try = true)]
fn required_keys_are_refused_when_absent() {
    for (document, field) in [
        (
            "[[rate_limits.overrides]]\napi_key = 'bot'\n",
            "rate_limits.overrides[0].limits",
        ),
        (
            "[rate_limits.categories.push]\njwt = [{ count = 1 }]\n",
            "rate_limits.categories.push.jwt[0].window_seconds",
        ),
        (
            "[[rate_limits.overrides]]\napi_key = 'bot'\n[rate_limits.overrides.limits]\n\
             push = [{ window_seconds = 1 }]\n",
            "rate_limits.overrides[0].limits.push[0].count",
        ),
    ] {
        let error = assert_invalid(load(document), field);
        assert!(
            matches!(error, ConfigError::Invalid { reason, .. } if reason == "is required"),
            "{error}"
        );
    }
}

/// Addresses in one IPv6 /64 are one caller, and an IPv4-mapped IPv6
/// address is the IPv4 caller. An IPv4-compatible address is not mapped.
// verifies: RATE-003
#[xmtp_common::test(unwrap_try = true)]
fn caller_keys_from_addresses_follow_the_ipv6_prefix_and_ipv4_mapping() {
    assert_eq!(
        ip("2001:db8:1:2::1"),
        ip("2001:db8:1:2:ffff:ffff:ffff:ffff")
    );
    assert_eq!(
        ip("2001:db8:1:2::1"),
        CallerKey::Ipv6Prefix([0x20, 0x01, 0x0d, 0xb8, 0, 1, 0, 2])
    );
    assert_ne!(ip("2001:db8:1:2::1"), ip("2001:db8:1:3::1"));
    assert_eq!(ip("::ffff:192.0.2.1"), ip("192.0.2.1"));
    assert_eq!(ip("192.0.2.1"), CallerKey::Ipv4([192, 0, 2, 1]));
    assert_ne!(ip("192.0.2.1"), ip("192.0.2.2"));
    assert_eq!(ip("::192.0.2.1"), CallerKey::Ipv6Prefix([0; 8]));
    for (key, auth_type) in [
        (ip("192.0.2.1"), AuthType::Ip),
        (ip("2001:db8::1"), AuthType::Ip),
        (CallerKey::ApiKey("bot".into()), AuthType::ApiKey),
        (
            CallerKey::Jwt {
                issuer: String::new(),
                subject: "user".into(),
            },
            AuthType::Jwt,
        ),
    ] {
        assert_eq!(key.auth_type(), auth_type, "{key:?}");
    }
}

const RATE: &str = include_str!("../../../../docs/specs/RATE-backend-rate-limits.md");

/// The default limits table of RATE section 3, by category name, each limit
/// as written: `6000 per 60 s`.
fn rate_default_limits() -> Vec<(String, Vec<LimitConfig>)> {
    let section = &RATE[RATE.find("## 3. Limits").expect("RATE has section 3")..];
    let table = &section[section
        .find("| Category | Default limits |")
        .expect("section 3 has the default limits table")..];
    table
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let name = cells[1].trim_matches('`').to_owned();
            let limits = cells[2]
                .split("; ")
                .map(|limit| {
                    let (count, window) = limit
                        .strip_suffix(" s")
                        .and_then(|limit| limit.split_once(" per "))
                        .unwrap_or_else(|| panic!("{name}: {limit} is not `C per W s`"));
                    LimitConfig {
                        count: count.parse().expect("a count"),
                        window_seconds: window.parse().expect("a window"),
                    }
                })
                .collect();
            (name, limits)
        })
        .collect()
}

/// The default limits are the table of RATE section 3, row for row in
/// `Category::ALL` order, so a changed or swapped constant fails.
// verifies: RATE-025
#[xmtp_common::test(unwrap_try = true)]
fn the_default_limits_are_the_rate_table() {
    let table = rate_default_limits();
    assert_eq!(table.len(), Category::ALL.len());
    for (category, (name, limits)) in Category::ALL.into_iter().zip(table) {
        assert_eq!(category.as_str(), name);
        assert_eq!(category.default_limits(), limits, "{name}");
    }
}

/// Different caller keys hash different bytes, including keys whose payload
/// bytes would collide without the auth type tag or the issuer's length.
#[xmtp_common::test(unwrap_try = true)]
fn caller_key_bytes_differ_for_different_keys() {
    let jwt = |issuer: &str, subject: &str| CallerKey::Jwt {
        issuer: issuer.into(),
        subject: subject.into(),
    };
    let keys = [
        CallerKey::ApiKey(String::new()),
        CallerKey::ApiKey("\u{0}\u{0}\u{0}\u{0}".into()),
        CallerKey::ApiKey("ab".into()),
        // Without its tag, the address 3.97.98.99 is the bytes of this key.
        CallerKey::ApiKey("abc".into()),
        // With one tag for both, this key is the bytes of `jwt("", "")`.
        CallerKey::ApiKey("\u{0}".repeat(8)),
        jwt("", ""),
        jwt("", "ab"),
        jwt("a", "b"),
        jwt("ab", ""),
        ip("0.0.0.0"),
        ip("3.97.98.99"),
        ip("::"),
        ip("2001:db8::"),
    ];
    let encoded: Vec<Vec<u8>> = keys
        .iter()
        .map(|key| {
            let mut bytes = Vec::new();
            key.to_bytes(&mut bytes);
            bytes
        })
        .collect();
    for (index, bytes) in encoded.iter().enumerate() {
        for (other, other_bytes) in encoded.iter().enumerate().skip(index + 1) {
            assert_ne!(
                bytes, other_bytes,
                "{:?} and {:?}",
                keys[index], keys[other]
            );
        }
    }
    // The bytes are appended.
    let mut bytes = vec![7];
    CallerKey::ApiKey("ab".into()).to_bytes(&mut bytes);
    assert_eq!(bytes[0], 7);
    assert_eq!(&bytes[1..], &encoded[2][..]);
}

/// The override for the caller key, then its auth type's limits, then the
/// category's `default`, then the default limits table, for each category
/// on its own. An empty list stays empty.
// verifies: RATE-025
#[xmtp_common::test(unwrap_try = true)]
fn the_most_specific_limits_apply() {
    let config = load(
        "[rate_limits.categories.publish]\n\
         default = [{ count = 1, window_seconds = 1 }]\n\
         jwt = [{ count = 2, window_seconds = 2 }]\n\
         api_key = []\n\
         [rate_limits.categories.push]\n\
         ip = [{ count = 3, window_seconds = 3 }]\n\
         [[rate_limits.overrides]]\n\
         jwt_issuer = 'https://auth.example.org'\njwt_subject = 'user'\n\
         [rate_limits.overrides.limits]\n\
         publish = [{ count = 4, window_seconds = 4 }, { count = 40, window_seconds = 40 }]\n\
         [[rate_limits.overrides]]\nip = '2001:db8::1'\n\
         [rate_limits.overrides.limits]\npush = []\n",
    )?;
    let limits = config.rate_limits();
    let one = |count, window_seconds| {
        vec![LimitConfig {
            count,
            window_seconds,
        }]
    };
    let user = CallerKey::Jwt {
        issuer: "https://auth.example.org".into(),
        subject: "user".into(),
    };
    let other_issuer = CallerKey::Jwt {
        issuer: "https://other.example.org".into(),
        subject: "user".into(),
    };
    let bot = CallerKey::ApiKey("bot".into());
    let address = ip("192.0.2.1");
    let prefix = ip("2001:db8::ffff");
    for (category, caller, expected) in [
        (
            Category::Publish,
            &user,
            vec![
                LimitConfig {
                    count: 4,
                    window_seconds: 4,
                },
                LimitConfig {
                    count: 40,
                    window_seconds: 40,
                },
            ],
        ),
        (Category::Publish, &other_issuer, one(2, 2)),
        (Category::Publish, &address, one(1, 1)),
        (Category::Publish, &prefix, one(1, 1)),
        (Category::Publish, &bot, vec![]),
        (Category::Push, &address, one(3, 3)),
        (Category::Push, &prefix, vec![]),
        (
            Category::Push,
            &user,
            Category::Push.default_limits().to_vec(),
        ),
        (
            Category::Request,
            &user,
            Category::Request.default_limits().to_vec(),
        ),
    ] {
        assert_eq!(
            limits.limits(category, caller),
            expected.as_slice(),
            "{category:?} for {caller:?}"
        );
    }
}

/// A server converts every list once with `try_map`: each caller then gets
/// the converted form of the list `limits` gives it, and a list the
/// conversion refuses fails the whole table.
#[xmtp_common::test(unwrap_try = true)]
fn mapped_limits_keep_each_callers_list() {
    let config = load(
        "[rate_limits.categories.publish]\n\
         jwt = [{ count = 2, window_seconds = 2 }]\n\
         [[rate_limits.overrides]]\nip = '192.0.2.1'\n\
         [rate_limits.overrides.limits]\n\
         publish = [{ count = 4, window_seconds = 4 }, { count = 40, window_seconds = 40 }]\n",
    )?;
    let limits = config.rate_limits();
    let counts = limits
        .caller_limits()
        .try_map(|list| Ok::<_, ()>(list.iter().map(|limit| limit.count).collect::<Vec<_>>()))
        .expect("an infallible map");
    let jwt = CallerKey::Jwt {
        issuer: String::new(),
        subject: "user".into(),
    };
    for caller in [ip("192.0.2.1"), ip("192.0.2.2"), jwt] {
        for category in Category::ALL {
            let expected: Vec<u32> = limits
                .limits(category, &caller)
                .iter()
                .map(|limit| limit.count)
                .collect();
            assert_eq!(counts.get(category, &caller), &expected, "{category:?}");
        }
    }
    let refused = limits.caller_limits().try_map(|list| match list.len() {
        2 if list[0].count == 4 => Err("the override"),
        _ => Ok(()),
    });
    assert_eq!(refused.err(), Some("the override"));
}

/// Without a `[rate_limits]` table, limits are on with the default limits
/// table for every caller, 100 MiB of state, and no trusted proxy.
#[xmtp_common::test(unwrap_try = true)]
fn an_absent_table_applies_the_defaults() {
    let config = load("")?;
    let limits = config.rate_limits();
    assert!(limits.enabled);
    assert_eq!(limits.max_memory_mib, 100);
    assert!(limits.trusted_proxies.is_empty());
    for category in Category::ALL {
        for caller in [
            ip("192.0.2.1"),
            CallerKey::ApiKey("bot".into()),
            CallerKey::Jwt {
                issuer: String::new(),
                subject: "user".into(),
            },
        ] {
            assert_eq!(
                limits.limits(category, &caller),
                category.default_limits(),
                "{category:?}"
            );
        }
        let windows: Vec<u32> = category
            .default_limits()
            .iter()
            .map(|limit| limit.window_seconds)
            .collect();
        assert!(windows.len() <= 3 && !windows.is_empty(), "{category:?}");
    }
    assert!(
        !load("[rate_limits]\nenabled = false\n")?
            .rate_limits()
            .enabled
    );
}

/// Each trusted proxy network resolves, with an IPv4 and an IPv6 network.
#[xmtp_common::test(unwrap_try = true)]
fn trusted_proxies_resolve_to_networks() {
    let config =
        load("[rate_limits]\ntrusted_proxies = ['10.0.0.0/8', 'fd00::/8', '192.0.2.7/032']\n")?;
    let networks: Vec<String> = config
        .rate_limits()
        .trusted_proxies
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(networks, ["10.0.0.0/8", "fd00::/8", "192.0.2.7/32"]);
}

/// A peer with an IPv4-mapped address is compared as its IPv4 address, so an
/// IPv4-mapped network resolves to the IPv4 network it maps. A network wider
/// than the mapped range stays IPv6.
#[xmtp_common::test(unwrap_try = true)]
fn ipv4_mapped_trusted_proxies_resolve_to_ipv4_networks() {
    let config = load(
        "[rate_limits]\ntrusted_proxies = ['::ffff:10.0.0.0/104', '::ffff:192.0.2.7/128', \
         '::ffff:0.0.0.0/96', '::/0']\n",
    )?;
    let networks: Vec<String> = config
        .rate_limits()
        .trusted_proxies
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        networks,
        ["10.0.0.0/8", "192.0.2.7/32", "0.0.0.0/0", "::/0"]
    );
    let peer = "::ffff:10.1.2.3".parse::<IpAddr>()?.to_canonical();
    assert!(config.rate_limits().trusted_proxies[0].contains(&peer));
}

/// Debug output names an API key, a principal, and no other caller value.
#[xmtp_common::test(unwrap_try = true)]
fn debug_output_redacts_caller_values() {
    let config = load(
        &[
            caller_override("jwt_issuer = 'issuer-7C2D'\njwt_subject = 'subject-7C2D'"),
            caller_override("ip = '198.51.100.73'"),
            caller_override("api_key = 'bridge-bot'"),
        ]
        .concat(),
    )?;
    for text in [
        format!("{config:?}"),
        format!(
            "{:?}",
            CallerKey::Jwt {
                issuer: "issuer-7C2D".into(),
                subject: "subject-7C2D".into(),
            }
        ),
        format!("{:?}", ip("198.51.100.73")),
        format!("{:?}", ip("2001:db8:7c2d::1")),
    ] {
        for secret in [
            "issuer-7C2D",
            "subject-7C2D",
            "198.51.100.73",
            "198, 51, 100, 73",
            "124, 45",
        ] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
    }
    assert!(format!("{config:?}").contains("bridge-bot"));
}
