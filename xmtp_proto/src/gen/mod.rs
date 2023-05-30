// @generated
pub mod xmtp {
    pub mod keystore_api {
        #[cfg(feature = "xmtp-keystore_api-v1")]
        // @@protoc_insertion_point(attribute:xmtp.keystore_api.v1)
        pub mod v1 {
            include!("xmtp.keystore_api.v1.rs");
            // @@protoc_insertion_point(xmtp.keystore_api.v1)
        }
    }
    pub mod message_api {
        #[cfg(feature = "xmtp-message_api-v1")]
        // @@protoc_insertion_point(attribute:xmtp.message_api.v1)
        pub mod v1 {
            include!("xmtp.message_api.v1.rs");
            // @@protoc_insertion_point(xmtp.message_api.v1)
        }
    }
    #[cfg(feature = "xmtp-message_contents")]
    // @@protoc_insertion_point(attribute:xmtp.message_contents)
    pub mod message_contents {
        include!("xmtp.message_contents.rs");
        // @@protoc_insertion_point(xmtp.message_contents)
    }
    pub mod v3 {
        #[cfg(feature = "xmtp-v3-message_contents")]
        // @@protoc_insertion_point(attribute:xmtp.v3.message_contents)
        pub mod message_contents {
            include!("xmtp.v3.message_contents.rs");
            // @@protoc_insertion_point(xmtp.v3.message_contents)
        }
    }
}