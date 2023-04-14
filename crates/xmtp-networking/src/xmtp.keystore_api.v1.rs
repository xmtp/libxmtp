/// Wrapper class for errors from the Keystore API
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeystoreError {
    #[prost(string, tag = "1")]
    pub message: ::prost::alloc::string::String,
    #[prost(enumeration = "ErrorCode", tag = "2")]
    pub code: i32,
}
/// A light pointer for a conversation that contains no decryption keys
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConversationReference {
    #[prost(string, tag = "1")]
    pub topic: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub peer_address: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub created_ns: u64,
    #[prost(message, optional, tag = "4")]
    pub context: ::core::option::Option<
        super::super::message_contents::invitation_v1::Context,
    >,
}
/// Decrypt a batch of messages using X3DH key agreement
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DecryptV1Request {
    #[prost(message, repeated, tag = "1")]
    pub requests: ::prost::alloc::vec::Vec<decrypt_v1_request::Request>,
}
/// Nested message and enum types in `DecryptV1Request`.
pub mod decrypt_v1_request {
    /// A single decryption request
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Request {
        #[prost(message, optional, tag = "1")]
        pub payload: ::core::option::Option<
            super::super::super::message_contents::Ciphertext,
        >,
        #[prost(message, optional, tag = "2")]
        pub peer_keys: ::core::option::Option<
            super::super::super::message_contents::PublicKeyBundle,
        >,
        #[prost(bytes = "vec", tag = "3")]
        pub header_bytes: ::prost::alloc::vec::Vec<u8>,
        #[prost(bool, tag = "4")]
        pub is_sender: bool,
    }
}
/// Response type for both V1 and V2 decryption requests
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DecryptResponse {
    #[prost(message, repeated, tag = "1")]
    pub responses: ::prost::alloc::vec::Vec<decrypt_response::Response>,
}
/// Nested message and enum types in `DecryptResponse`.
pub mod decrypt_response {
    /// A single decryption response
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Response {
        #[prost(oneof = "response::Response", tags = "1, 2")]
        pub response: ::core::option::Option<response::Response>,
    }
    /// Nested message and enum types in `Response`.
    pub mod response {
        /// Wrapper object for success response
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Success {
            #[prost(bytes = "vec", tag = "1")]
            pub decrypted: ::prost::alloc::vec::Vec<u8>,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Oneof)]
        pub enum Response {
            #[prost(message, tag = "1")]
            Result(Success),
            #[prost(message, tag = "2")]
            Error(super::super::KeystoreError),
        }
    }
}
/// Decrypt a batch of messages using the appropriate topic keys
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DecryptV2Request {
    #[prost(message, repeated, tag = "1")]
    pub requests: ::prost::alloc::vec::Vec<decrypt_v2_request::Request>,
}
/// Nested message and enum types in `DecryptV2Request`.
pub mod decrypt_v2_request {
    /// A single decryption request
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Request {
        #[prost(message, optional, tag = "1")]
        pub payload: ::core::option::Option<
            super::super::super::message_contents::Ciphertext,
        >,
        #[prost(bytes = "vec", tag = "2")]
        pub header_bytes: ::prost::alloc::vec::Vec<u8>,
        #[prost(string, tag = "3")]
        pub content_topic: ::prost::alloc::string::String,
    }
}
/// Encrypt a batch of messages using X3DH key agreement
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EncryptV1Request {
    #[prost(message, repeated, tag = "1")]
    pub requests: ::prost::alloc::vec::Vec<encrypt_v1_request::Request>,
}
/// Nested message and enum types in `EncryptV1Request`.
pub mod encrypt_v1_request {
    /// A single encryption request
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Request {
        #[prost(message, optional, tag = "1")]
        pub recipient: ::core::option::Option<
            super::super::super::message_contents::PublicKeyBundle,
        >,
        #[prost(bytes = "vec", tag = "2")]
        pub payload: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub header_bytes: ::prost::alloc::vec::Vec<u8>,
    }
}
/// Response type for both V1 and V2 encryption requests
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EncryptResponse {
    #[prost(message, repeated, tag = "1")]
    pub responses: ::prost::alloc::vec::Vec<encrypt_response::Response>,
}
/// Nested message and enum types in `EncryptResponse`.
pub mod encrypt_response {
    /// A single encryption response
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Response {
        #[prost(oneof = "response::Response", tags = "1, 2")]
        pub response: ::core::option::Option<response::Response>,
    }
    /// Nested message and enum types in `Response`.
    pub mod response {
        /// Wrapper object for success response
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Success {
            #[prost(message, optional, tag = "1")]
            pub encrypted: ::core::option::Option<
                super::super::super::super::message_contents::Ciphertext,
            >,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Oneof)]
        pub enum Response {
            #[prost(message, tag = "1")]
            Result(Success),
            #[prost(message, tag = "2")]
            Error(super::super::KeystoreError),
        }
    }
}
/// Encrypt a batch of messages using the appropriate topic keys
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EncryptV2Request {
    #[prost(message, repeated, tag = "1")]
    pub requests: ::prost::alloc::vec::Vec<encrypt_v2_request::Request>,
}
/// Nested message and enum types in `EncryptV2Request`.
pub mod encrypt_v2_request {
    /// A single encryption request
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Request {
        #[prost(bytes = "vec", tag = "1")]
        pub payload: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub header_bytes: ::prost::alloc::vec::Vec<u8>,
        #[prost(string, tag = "3")]
        pub content_topic: ::prost::alloc::string::String,
    }
}
/// Request to create an invite payload, and store the topic keys in the Keystore
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateInviteRequest {
    #[prost(message, optional, tag = "1")]
    pub context: ::core::option::Option<
        super::super::message_contents::invitation_v1::Context,
    >,
    #[prost(message, optional, tag = "2")]
    pub recipient: ::core::option::Option<
        super::super::message_contents::SignedPublicKeyBundle,
    >,
    #[prost(uint64, tag = "3")]
    pub created_ns: u64,
}
/// Response to a CreateInviteRequest
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateInviteResponse {
    #[prost(message, optional, tag = "1")]
    pub conversation: ::core::option::Option<ConversationReference>,
    #[prost(bytes = "vec", tag = "2")]
    pub payload: ::prost::alloc::vec::Vec<u8>,
}
/// Request to save a batch of invite messages to the Keystore
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SaveInvitesRequest {
    #[prost(message, repeated, tag = "1")]
    pub requests: ::prost::alloc::vec::Vec<save_invites_request::Request>,
}
/// Nested message and enum types in `SaveInvitesRequest`.
pub mod save_invites_request {
    /// Mirrors xmtp.envelope schema
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Request {
        #[prost(string, tag = "1")]
        pub content_topic: ::prost::alloc::string::String,
        #[prost(uint64, tag = "2")]
        pub timestamp_ns: u64,
        #[prost(bytes = "vec", tag = "3")]
        pub payload: ::prost::alloc::vec::Vec<u8>,
    }
}
/// Response to a SaveInvitesRequest
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SaveInvitesResponse {
    #[prost(message, repeated, tag = "1")]
    pub responses: ::prost::alloc::vec::Vec<save_invites_response::Response>,
}
/// Nested message and enum types in `SaveInvitesResponse`.
pub mod save_invites_response {
    /// A single response
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Response {
        #[prost(oneof = "response::Response", tags = "1, 2")]
        pub response: ::core::option::Option<response::Response>,
    }
    /// Nested message and enum types in `Response`.
    pub mod response {
        /// Wrapper object for success response
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Success {
            #[prost(message, optional, tag = "1")]
            pub conversation: ::core::option::Option<
                super::super::ConversationReference,
            >,
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Oneof)]
        pub enum Response {
            #[prost(message, tag = "1")]
            Result(Success),
            #[prost(message, tag = "2")]
            Error(super::super::KeystoreError),
        }
    }
}
/// CreateAuthTokenRequest is used to create an auth token for the XMTP API
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateAuthTokenRequest {
    #[prost(uint64, optional, tag = "1")]
    pub timestamp_ns: ::core::option::Option<u64>,
}
/// SignDigestRequest is used to sign a digest with either the identity key
/// or a prekey
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignDigestRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub digest: ::prost::alloc::vec::Vec<u8>,
    #[prost(oneof = "sign_digest_request::Signer", tags = "2, 3")]
    pub signer: ::core::option::Option<sign_digest_request::Signer>,
}
/// Nested message and enum types in `SignDigestRequest`.
pub mod sign_digest_request {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Signer {
        #[prost(bool, tag = "2")]
        IdentityKey(bool),
        #[prost(uint32, tag = "3")]
        PrekeyIndex(u32),
    }
}
/// A mapping of topics to their decrypted invitations
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TopicMap {
    #[prost(map = "string, message", tag = "1")]
    pub topics: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        topic_map::TopicData,
    >,
}
/// Nested message and enum types in `TopicMap`.
pub mod topic_map {
    /// TopicData wraps the invitation and the timestamp it was created
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct TopicData {
        #[prost(uint64, tag = "1")]
        pub created_ns: u64,
        #[prost(string, tag = "2")]
        pub peer_address: ::prost::alloc::string::String,
        #[prost(message, optional, tag = "3")]
        pub invitation: ::core::option::Option<
            super::super::super::message_contents::InvitationV1,
        >,
    }
}
/// Application-specific error codes for the Keystore API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ErrorCode {
    Unspecified = 0,
    InvalidInput = 1,
    NoMatchingPrekey = 2,
}
impl ErrorCode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ErrorCode::Unspecified => "ERROR_CODE_UNSPECIFIED",
            ErrorCode::InvalidInput => "ERROR_CODE_INVALID_INPUT",
            ErrorCode::NoMatchingPrekey => "ERROR_CODE_NO_MATCHING_PREKEY",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ERROR_CODE_UNSPECIFIED" => Some(Self::Unspecified),
            "ERROR_CODE_INVALID_INPUT" => Some(Self::InvalidInput),
            "ERROR_CODE_NO_MATCHING_PREKEY" => Some(Self::NoMatchingPrekey),
            _ => None,
        }
    }
}
