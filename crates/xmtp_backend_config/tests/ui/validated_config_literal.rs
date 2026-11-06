// A caller cannot build a `ValidatedConfig` without validating.
use xmtp_backend_config::{Config, ValidatedConfig};

fn main() {
    let source =
        "[server]\nidentifier = 'org.xmtp.test'\n[database]\nurl = 'postgres://localhost/xmtp'\n";
    let rate_limits = ValidatedConfig::load_str(source)
        .unwrap()
        .rate_limits()
        .clone();
    let config = Config::from_toml_str(source).unwrap();
    let _validated = ValidatedConfig {
        config,
        scw_cache_entries: std::num::NonZeroUsize::MIN,
        rate_limits,
    };
}
