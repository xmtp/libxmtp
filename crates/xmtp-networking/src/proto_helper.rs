mod xmtp {
    mod keystore_api {
        mod v1 {
            include!("xmtp.keystore_api.v1.rs");
        }
    }
    mod message_api {
        mod v1 {
            include!("xmtp.message_api.v1.rs");
        }
    }
    mod message_contents {
        mod v1 {
            include!("xmtp.message_contents.rs");
        }
    }
}
