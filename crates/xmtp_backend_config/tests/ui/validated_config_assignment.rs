// A section of a `ValidatedConfig` cannot be changed after validation, even
// through a mutable binding.
use xmtp_backend_config::ValidatedConfig;

fn main() {
    let mut validated = ValidatedConfig::load_str(
        "[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n",
    )
    .unwrap();
    let _ = &mut validated;
    validated.limits.max_query_limit = 1;
}
