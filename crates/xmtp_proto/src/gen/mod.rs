pub mod xmtp {
    pub mod device_sync {
        include!("xmtp.device_sync.rs");
        include!("xmtp.device_sync.serde.rs");
        pub mod consent_backup {
            include!("xmtp.device_sync.consent_backup.rs");
            include!("xmtp.device_sync.consent_backup.serde.rs");
        }
        pub mod content {
            include!("xmtp.device_sync.content.rs");
            include!("xmtp.device_sync.content.serde.rs");
        }
        pub mod event_backup {
            include!("xmtp.device_sync.event_backup.rs");
            include!("xmtp.device_sync.event_backup.serde.rs");
        }
        pub mod group_backup {
            include!("xmtp.device_sync.group_backup.rs");
            include!("xmtp.device_sync.group_backup.serde.rs");
        }
        pub mod message_backup {
            include!("xmtp.device_sync.message_backup.rs");
            include!("xmtp.device_sync.message_backup.serde.rs");
        }
    }
    pub mod identity {
        include!("xmtp.identity.rs");
        include!("xmtp.identity.serde.rs");
        pub mod api {
            pub mod v1 {
                include!("xmtp.identity.api.v1.rs");
                include!("xmtp.identity.api.v1.serde.rs");
            }
        }
        pub mod associations {
            include!("xmtp.identity.associations.rs");
            include!("xmtp.identity.associations.serde.rs");
        }
    }
    pub mod keystore_api {
        pub mod v1 {
            include!("xmtp.keystore_api.v1.rs");
            include!("xmtp.keystore_api.v1.serde.rs");
        }
    }
    pub mod message_api {
        pub mod v1 {
            include!("xmtp.message_api.v1.rs");
            include!("xmtp.message_api.v1.serde.rs");
        }
    }
    pub mod message_contents {
        include!("xmtp.message_contents.rs");
        include!("xmtp.message_contents.serde.rs");
    }
    pub mod mls {
        pub mod api {
            pub mod v1 {
                include!("xmtp.mls.api.v1.rs");
                include!("xmtp.mls.api.v1.serde.rs");
            }
        }
        pub mod database {
            include!("xmtp.mls.database.rs");
            include!("xmtp.mls.database.serde.rs");
        }
        pub mod message_contents {
            include!("xmtp.mls.message_contents.rs");
            include!("xmtp.mls.message_contents.serde.rs");
            pub mod content_types {
                include!("xmtp.mls.message_contents.content_types.rs");
                include!("xmtp.mls.message_contents.content_types.serde.rs");
            }
        }
    }
    pub mod mls_validation {
        pub mod v1 {
            include!("xmtp.mls_validation.v1.rs");
            include!("xmtp.mls_validation.v1.serde.rs");
        }
    }
    pub mod xmtpv4 {
        pub mod envelopes {
            include!("xmtp.xmtpv4.envelopes.rs");
            include!("xmtp.xmtpv4.envelopes.serde.rs");
        }
        pub mod message_api {
            include!("xmtp.xmtpv4.message_api.rs");
            include!("xmtp.xmtpv4.message_api.serde.rs");
        }
        pub mod metadata_api {
            include!("xmtp.xmtpv4.metadata_api.rs");
            include!("xmtp.xmtpv4.metadata_api.serde.rs");
        }
        pub mod payer_api {
            include!("xmtp.xmtpv4.payer_api.rs");
            include!("xmtp.xmtpv4.payer_api.serde.rs");
        }
    }
    pub mod migration {
        pub mod api {
            pub mod v1 {
                include!("xmtp.migration.api.v1.rs");
                include!("xmtp.migration.api.v1.serde.rs");
            }
        }
    }
}
