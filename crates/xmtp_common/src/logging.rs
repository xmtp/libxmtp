use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

pub fn filter_directive(level: &str) -> EnvFilter {
    let level: LevelFilter = level
        .parse()
        .inspect_err(|_| tracing::error!("invalid level `{}`, defaulting to `INFO`", level))
        .unwrap_or(LevelFilter::INFO);

    let filter = format!(
        "xmtp_mls={level},xmtp_mls_common={level},xmtp_id={level},\
        xmtp_api={level},xmtp_api_grpc={level},xmtp_proto={level},\
        xmtp_common={level},xmtp_api_d14n={level},\
        xmtp_content_types={level},xmtp_cryptography={level},\
        xmtp_user_preferences={level},xmtpv3={level},xmtp_db={level},\
        bindings_wasm={level},bindings_node={level},xdbg=error"
    );
    EnvFilter::builder()
        .parse(filter)
        .expect("Static filter must be correct")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[xmtp_common::test]
    fn test_filter_correct() {
        filter_directive("OFF");
        filter_directive("ERROR");
        filter_directive("WARN");
        filter_directive("INFO");
        filter_directive("DEBUG");
        filter_directive("TRACE");
        filter_directive("INCORRECT_DOES_NOT_PANIC");
    }
}
