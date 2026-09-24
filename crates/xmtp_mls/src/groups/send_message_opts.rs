use derive_builder::Builder;

// Options to apply when sending a message
#[derive(Debug, Clone, Builder, Default)]
pub struct SendMessageOpts {
    pub should_push: bool,
    /// Optional caller-supplied idempotency key. The message id is derived from
    /// this key, so re-sending identical content with the same key yields the
    /// same id and is deduplicated. When `None`, defaults to the send timestamp,
    /// preserving the historical (always-unique) behavior.
    #[builder(default)]
    pub idempotency_key: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_message_opts_builder() {
        let opts = SendMessageOptsBuilder::default()
            .should_push(true)
            .build()
            .unwrap();

        assert!(opts.should_push);
    }
}
