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

#[test]
fn extra_traefik_routes_defaults_to_empty() {
    let toml_str = "[xnet]\n";
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert!(config.extra_traefik_routes.is_empty());
}

#[test]
fn extra_traefik_routes_parses_single_route() {
    let toml_str = r#"
[[extra_traefik_routes]]
name = "status-page"
rule = "Host(`migrate.xmtp.run`)"
url = "http://127.0.0.1:8899"
priority = 100
"#;
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.extra_traefik_routes.len(), 1);
    let route = &config.extra_traefik_routes[0];
    assert_eq!(route.name, "status-page");
    assert_eq!(route.rule, "Host(`migrate.xmtp.run`)");
    assert_eq!(route.url, "http://127.0.0.1:8899");
    assert_eq!(route.priority, Some(100));
}

#[test]
fn remote_domain_is_valid() {
    crate::config::loadable::validate_remote_domain(&Some("xmtp.run".to_string())).unwrap();
}

#[test]
fn no_remote_domain_is_valid() {
    crate::config::loadable::validate_remote_domain(&None).unwrap();
}

#[test]
fn remote_domain_rejects_empty() {
    let err = crate::config::loadable::validate_remote_domain(&Some("".to_string())).unwrap_err();
    assert!(
        err.to_string().contains("empty"),
        "expected empty domain error, got: {}",
        err
    );
}

#[test]
fn remote_domain_rejects_leading_dot() {
    let err = crate::config::loadable::validate_remote_domain(&Some(".xmtp.run".to_string()))
        .unwrap_err();
    assert!(
        err.to_string().contains("must not start or end with '.'"),
        "expected leading dot error, got: {}",
        err
    );
}

#[test]
fn remote_domain_rejects_trailing_dot() {
    let err = crate::config::loadable::validate_remote_domain(&Some("xmtp.run.".to_string()))
        .unwrap_err();
    assert!(
        err.to_string().contains("must not start or end with '.'"),
        "expected trailing dot error, got: {}",
        err
    );
}

#[test]
fn extra_traefik_routes_parses_multiple_routes() {
    let toml_str = r#"
[[extra_traefik_routes]]
name = "status-page"
rule = "Host(`migrate.xmtp.run`)"
url = "http://127.0.0.1:8899"
priority = 100

[[extra_traefik_routes]]
name = "another-service"
rule = "Host(`other.example.com`)"
url = "http://127.0.0.1:9999"
"#;
    let config: TomlConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.extra_traefik_routes.len(), 2);
    assert_eq!(config.extra_traefik_routes[0].name, "status-page");
    assert_eq!(config.extra_traefik_routes[1].name, "another-service");
    assert_eq!(config.extra_traefik_routes[1].priority, None);
}
