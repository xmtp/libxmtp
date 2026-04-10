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
