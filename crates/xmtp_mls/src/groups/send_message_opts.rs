use derive_builder::Builder;

// Options to apply when sending a message
#[derive(Debug, Clone, Builder, Default)]
pub struct SendMessageOpts {
    pub should_push: bool,
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
