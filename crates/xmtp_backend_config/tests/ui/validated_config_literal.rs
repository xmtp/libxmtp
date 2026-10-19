// A caller cannot build a `ValidatedConfig` without validating.
use xmtp_backend_config::{Config, ValidatedConfig};

fn main() {
    let config = Config::from_toml_str("[database]\nurl = 'postgres://localhost/xmtp'\n").unwrap();
    let _validated = ValidatedConfig {
        config,
        scw_cache_entries: std::num::NonZeroUsize::MIN,
    };
}
