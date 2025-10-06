use tracing_subscriber::EnvFilter;

pub fn filter_directive(level: &str) -> EnvFilter {
    let filter = format!(
        "xmtp_mls={level},xmtp_id={level},\
        xmtp_api={level},xmtp_api_grpc={level},xmtp_proto={level},\
        xmtp_common={level},xmtp_api_d14n={level},\
        xmtp_content_types={level},xmtp_cryptography={level},\
        xmtp_user_preferences={level},xmtpv3={level},xmtp_db={level},\
        bindings_wasm={level},bindings_node={level}"
    );
    EnvFilter::builder().parse_lossy(filter)
}
