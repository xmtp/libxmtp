// Settings that skipped validation cannot be handed to a function that takes
// a validated section, such as a storage target constructor.
use xmtp_backend_config::{
    Validated,
    attachments::{CredentialsConfig, S3Config},
};

fn build_target(_settings: Validated<'_, S3Config>) {}

fn main() {
    let settings = S3Config {
        endpoint: "http://storage.example".into(),
        region: String::new(),
        bucket: "attachments".into(),
        key_prefix: String::new(),
        credentials: CredentialsConfig::Environment,
        presign_ttl_seconds: Some(1),
    };
    build_target(Validated(&settings));
}
