#[allow(clippy::all)]
#[allow(warnings)]
mod generated {
    //! Module structure of Protos for XMTP

    include!(concat!(env!("OUT_DIR"), "/mod.rs"));
    pub const FILE_DESCRIPTOR_SET: &'static [u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/proto_descriptor.bin"));
}

pub mod api_client;
pub mod codec;
mod convert;
mod error;
mod impls;
mod traits;
pub mod types;

pub use error::*;
pub use generated::*;
pub use impls::update_dedupe::GroupUpdateDeduper;
pub use traits::short_hex::ShortHex;

pub mod api {
    pub use super::traits::combinators::*;
    pub use super::traits::stream::*;
    pub use super::traits::*;
}

#[cfg(test)]
pub mod test {
    xmtp_common::if_native! {
        #[cfg(test)]
        #[ctor::ctor(unsafe)]
        fn _setup() {
            xmtp_common::logger()
        }
    }
}

pub mod prelude {
    pub use super::FILE_DESCRIPTOR_SET;
    xmtp_common::if_test! {
        pub use super::api_client::XmtpTestClient;
    }
    pub use super::api_client::{
        ApiBuilder, ArcedXmtpApi, BoxedXmtpApi, NetConnectConfig, XmtpBackendClient, XmtpMlsStreams,
    };
    pub use super::traits::{ApiClientError, BytesStream, Client, Endpoint, Query, QueryStream};
}

pub mod identity_v1 {
    pub use super::xmtp::identity::api::v1::*;
}

pub mod backend_v1 {
    pub use super::xmtp::backend::v1::*;
}

#[cfg(test)]
mod descriptor_tests {
    use prost::Message;
    use prost_types::field_descriptor_proto::{Label, Type};

    type ExpectedField = (&'static str, i32, Type, Label, Option<&'static str>);

    fn assert_fields(message: &prost_types::DescriptorProto, expected: &[ExpectedField]) {
        let message_name = message.name.as_deref().expect("message name");
        let mut numbers = std::collections::HashSet::new();
        for field in &message.field {
            assert!(
                numbers.insert(field.number),
                "{message_name} reuses a field number"
            );
        }
        for &(name, number, kind, label, type_name) in expected {
            let field = message
                .field
                .iter()
                .find(|field| field.name.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("{message_name}.{name} descriptor"));
            assert_eq!(field.number, Some(number), "{message_name}.{name}");
            assert_eq!(field.r#type(), kind, "{message_name}.{name}");
            assert_eq!(field.label(), label, "{message_name}.{name}");
            assert_eq!(
                field.type_name.as_deref(),
                type_name,
                "{message_name}.{name}"
            );
        }
    }

    // verifies: CONF-017
    #[xmtp_common::test(unwrap_try = true)]
    fn public_configuration_fields_match_the_wire_contract() {
        use Label::{Optional, Repeated};
        use Type::{Bool, Message, String, Uint32, Uint64};

        let descriptors = prost_types::FileDescriptorSet::decode(crate::FILE_DESCRIPTOR_SET)?;
        let file = descriptors
            .file
            .iter()
            .find(|file| {
                file.package.as_deref() == Some("xmtp.backend.v1")
                    && file
                        .message_type
                        .iter()
                        .any(|message| message.name.as_deref() == Some("GetConfigurationResponse"))
            })
            .expect("backend configuration file descriptor");
        let message = |name: &str| {
            file.message_type
                .iter()
                .find(|message| message.name.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("{name} descriptor"))
        };

        assert!(message("GetConfigurationRequest").field.is_empty());
        let auth = message("AuthConfiguration");
        let signing_key = auth
            .nested_type
            .iter()
            .find(|message| message.name.as_deref() == Some("SigningKey"))
            .expect("AuthConfiguration.SigningKey descriptor");
        assert_fields(
            signing_key,
            &[
                ("kid", 1, String, Optional, None),
                ("alg", 2, String, Optional, None),
            ],
        );
        assert_fields(
            auth,
            &[
                ("enabled", 1, Bool, Optional, None),
                (
                    "keys",
                    2,
                    Message,
                    Repeated,
                    Some(".xmtp.backend.v1.AuthConfiguration.SigningKey"),
                ),
                ("audiences", 3, String, Repeated, None),
                ("issuers", 4, String, Repeated, None),
                ("required_scopes", 5, String, Repeated, None),
            ],
        );
        assert_fields(
            message("RetentionConfiguration"),
            &[
                ("group_message_seconds", 1, Uint64, Optional, None),
                ("welcome_seconds", 2, Uint64, Optional, None),
                ("key_package_seconds", 3, Uint64, Optional, None),
            ],
        );
        assert_fields(
            message("LimitsConfiguration"),
            &[
                ("max_envelope_bytes", 1, Uint64, Optional, None),
                ("max_request_bytes", 2, Uint64, Optional, None),
                ("max_response_bytes", 3, Uint64, Optional, None),
                ("max_publish_topics", 4, Uint32, Optional, None),
                ("max_query_topics", 5, Uint32, Optional, None),
                ("max_query_limit", 6, Uint32, Optional, None),
                ("default_query_limit", 7, Uint32, Optional, None),
                ("max_newest_metadata_topics", 8, Uint32, Optional, None),
                ("max_newest_full_topics", 9, Uint32, Optional, None),
                ("max_update_adds", 10, Uint32, Optional, None),
                ("max_update_removes", 11, Uint32, Optional, None),
                ("max_stream_topics", 12, Uint32, Optional, None),
                ("max_static_topics", 13, Uint32, Optional, None),
                ("max_lookup_identifiers", 14, Uint32, Optional, None),
                ("max_scw_signatures", 15, Uint32, Optional, None),
                ("max_identity_entries", 16, Uint32, Optional, None),
                ("max_update_frames_per_second", 17, Uint32, Optional, None),
                ("max_update_burst", 18, Uint32, Optional, None),
                ("max_ping_frames_per_second", 19, Uint32, Optional, None),
                ("max_ping_burst", 20, Uint32, Optional, None),
            ],
        );
        let mls = message("MlsConfiguration");
        assert_fields(
            mls,
            &[
                ("max_group_members", 1, Uint32, Optional, None),
                ("max_installations_per_inbox", 2, Uint32, Optional, None),
                ("commit_log_enabled", 3, Bool, Optional, None),
            ],
        );
        let commit_log = mls
            .field
            .iter()
            .find(|field| field.name.as_deref() == Some("commit_log_enabled"))
            .expect("commit_log_enabled descriptor");
        assert_eq!(commit_log.proto3_optional, Some(true));
        assert_fields(
            message("GetConfigurationResponse"),
            &[
                ("identifier", 1, String, Optional, None),
                ("server_version", 2, String, Optional, None),
                ("min_libxmtp_version", 3, String, Optional, None),
                (
                    "auth",
                    4,
                    Message,
                    Optional,
                    Some(".xmtp.backend.v1.AuthConfiguration"),
                ),
                (
                    "retention",
                    5,
                    Message,
                    Optional,
                    Some(".xmtp.backend.v1.RetentionConfiguration"),
                ),
                (
                    "limits",
                    6,
                    Message,
                    Optional,
                    Some(".xmtp.backend.v1.LimitsConfiguration"),
                ),
                (
                    "mls",
                    7,
                    Message,
                    Optional,
                    Some(".xmtp.backend.v1.MlsConfiguration"),
                ),
                ("smart_contract_wallet_chains", 8, String, Repeated, None),
                (
                    "attachments",
                    9,
                    Message,
                    Optional,
                    Some(".xmtp.backend.v1.AttachmentsConfiguration"),
                ),
            ],
        );
    }

    // verifies: ATCH-020
    #[xmtp_common::test(unwrap_try = true)]
    fn create_upload_is_unary_in_backend_descriptor() {
        let descriptors = prost_types::FileDescriptorSet::decode(crate::FILE_DESCRIPTOR_SET)?;
        let service = descriptors
            .file
            .iter()
            .filter(|file| file.package.as_deref() == Some("xmtp.backend.v1"))
            .flat_map(|file| &file.service)
            .find(|service| service.name.as_deref() == Some("AttachmentService"))
            .expect("AttachmentService descriptor");
        let method = service
            .method
            .iter()
            .find(|method| method.name.as_deref() == Some("CreateUpload"))
            .expect("CreateUpload descriptor");

        assert!(!method.client_streaming.unwrap_or(false));
        assert!(!method.server_streaming.unwrap_or(false));
    }
}
