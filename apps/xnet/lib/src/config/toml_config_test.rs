use super::toml_config::TomlConfig;

#[test]
fn paused_defaults_to_false() {
    let toml_str = "[xnet]\n";
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.xnet.paused);
}

#[test]
fn paused_parses_true() {
    let toml_str = "[xnet]\npaused = true\n";
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert!(config.xnet.paused);
}

#[test]
fn paused_parses_false_explicit() {
    let toml_str = "[xnet]\npaused = false\n";
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.xnet.paused);
}

#[test]
fn node_use_standard_port_defaults_to_false() {
    let toml_str = "[[xmtpd.nodes]]\nenable = true\n";
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.xmtpd.nodes[0].use_standard_port);
}

#[test]
fn node_use_standard_port_parses_true() {
    let toml_str = "[[xmtpd.nodes]]\nenable = true\nuse_standard_port = true\n";
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert!(config.xmtpd.nodes[0].use_standard_port);
}

#[test]
fn validation_rejects_two_standard_port_nodes() {
    use crate::config::loadable::validate_node_toml;
    use crate::config::toml_config::NodeToml;
    let nodes = vec![
        NodeToml {
            enable: true,
            use_standard_port: true,
            ..Default::default()
        },
        NodeToml {
            enable: true,
            use_standard_port: true,
            ..Default::default()
        },
    ];
    let result = validate_node_toml(&nodes);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("at most one"));
}

#[test]
fn validation_rejects_standard_port_with_explicit_port() {
    use crate::config::loadable::validate_node_toml;
    use crate::config::toml_config::NodeToml;
    let nodes = vec![NodeToml {
        enable: true,
        use_standard_port: true,
        port: Some(9000),
        name: Some("alice".into()),
        ..Default::default()
    }];
    let result = validate_node_toml(&nodes);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("cannot set both"));
}

#[test]
fn validation_allows_one_standard_port_node() {
    use crate::config::loadable::validate_node_toml;
    use crate::config::toml_config::NodeToml;
    let nodes = vec![
        NodeToml {
            enable: true,
            use_standard_port: true,
            ..Default::default()
        },
        NodeToml {
            enable: true,
            ..Default::default()
        },
    ];
    assert!(validate_node_toml(&nodes).is_ok());
}

#[test]
fn validation_allows_zero_standard_port_nodes() {
    use crate::config::loadable::validate_node_toml;
    use crate::config::toml_config::NodeToml;
    let nodes = vec![
        NodeToml {
            enable: true,
            port: Some(3000),
            ..Default::default()
        },
        NodeToml {
            enable: true,
            ..Default::default()
        },
    ];
    assert!(validate_node_toml(&nodes).is_ok());
}
