pub mod xmtp {
    pub mod keystore_api {
        pub mod v1 {
            include!("xmtp.keystore_api.v1.rs");
        }
    }
    pub mod message_api {
        pub mod v1 {
            include!("xmtp.message_api.v1.rs");
        }
    }
    pub mod message_contents {
        include!("xmtp.message_contents.rs");
    }
}
