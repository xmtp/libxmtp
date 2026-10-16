//! The backend's identifier check and the client's must agree on every input.
//! A deployment the backend starts with must publish an identifier the client
//! stores, and the backend must refuse every identifier a client would reject.

use super::*;

// verifies: CONF-002, CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn the_backend_and_the_client_judge_identifiers_alike() {
    let identifiers = [
        String::new(),
        "a".into(),
        "org.xmtp.test".into(),
        "a".repeat(MAX_SERVER_IDENTIFIER_BYTES),
        "a".repeat(MAX_SERVER_IDENTIFIER_BYTES + 1),
        // The bound counts bytes, not characters.
        "é".repeat(MAX_SERVER_IDENTIFIER_BYTES / 2),
        "é".repeat(MAX_SERVER_IDENTIFIER_BYTES / 2 + 1),
        format!("{}é", "a".repeat(MAX_SERVER_IDENTIFIER_BYTES - 1)),
        " ".into(),
        " org.xmtp".into(),
        "org.xmtp ".into(),
        "org xmtp".into(),
        "org\txmtp".into(),
        "org\nxmtp".into(),
        "org\rxmtp".into(),
        "org\u{0}xmtp".into(),
        "org\u{7f}xmtp".into(),
        "org\u{85}xmtp".into(),
        "org\u{9f}xmtp".into(),
        "org\u{a0}xmtp".into(),
        "org\u{1680}xmtp".into(),
        "org\u{2003}xmtp".into(),
        "org\u{2028}xmtp".into(),
        "org\u{3000}xmtp".into(),
        // Format characters are neither whitespace nor control characters.
        "org\u{200b}xmtp".into(),
        "org\u{feff}xmtp".into(),
        "org.xmtp/dev:1#é".into(),
    ];
    let mut config: Config =
        toml::from_str("[database]\nurl = 'postgres://localhost/xmtp'\n[server]\n")?;
    for identifier in identifiers {
        config.server.identifier = Some(identifier.clone());
        let backend_refuses = match config.server.validate() {
            Ok(()) => false,
            Err(ConfigError::Invalid {
                field: "server.identifier",
                ..
            }) => true,
            Err(other) => panic!("{identifier:?} failed another check: {other}"),
        };
        let client_refuses = xmtp_configuration::validate_server_identifier(&identifier).is_err();
        assert_eq!(backend_refuses, client_refuses, "{identifier:?}");
    }
}
