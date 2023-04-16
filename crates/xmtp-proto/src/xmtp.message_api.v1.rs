// @generated
/// Token is used by clients to prove to the nodes
/// that they are serving a specific wallet.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Token {
    /// identity key signed by a wallet
    #[prost(message, optional, tag="1")]
    pub identity_key: ::core::option::Option<super::super::message_contents::PublicKey>,
    /// encoded bytes of AuthData
    #[prost(bytes="vec", tag="2")]
    pub auth_data_bytes: ::prost::alloc::vec::Vec<u8>,
    /// identity key signature of AuthData bytes
    #[prost(message, optional, tag="3")]
    pub auth_data_signature: ::core::option::Option<super::super::message_contents::Signature>,
}
/// AuthData carries token parameters that are authenticated
/// by the identity key signature.
/// It is embedded in the Token structure as bytes
/// so that the bytes don't need to be reconstructed
/// to verify the token signature.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthData {
    /// address of the wallet
    #[prost(string, tag="1")]
    pub wallet_addr: ::prost::alloc::string::String,
    /// time when the token was generated/signed 
    #[prost(uint64, tag="2")]
    pub created_ns: u64,
}
/// This is based off of the go-waku Index type, but with the
/// receiverTime and pubsubTopic removed for simplicity.
/// Both removed fields are optional
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IndexCursor {
    #[prost(bytes="vec", tag="1")]
    pub digest: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag="2")]
    pub sender_time_ns: u64,
}
/// Wrapper for potentially multiple types of cursor
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Cursor {
    /// Making the cursor a one-of type, as I would like to change the way we
    /// handle pagination to use a precomputed sort field.
    /// This way we can handle both methods
    #[prost(oneof="cursor::Cursor", tags="1")]
    pub cursor: ::core::option::Option<cursor::Cursor>,
}
/// Nested message and enum types in `Cursor`.
pub mod cursor {
    /// Making the cursor a one-of type, as I would like to change the way we
    /// handle pagination to use a precomputed sort field.
    /// This way we can handle both methods
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Cursor {
        #[prost(message, tag="1")]
        Index(super::IndexCursor),
    }
}
/// This is based off of the go-waku PagingInfo struct, but with the direction
/// changed to our SortDirection enum format
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PagingInfo {
    /// Note: this is a uint32, while go-waku's pageSize is a uint64
    #[prost(uint32, tag="1")]
    pub limit: u32,
    #[prost(message, optional, tag="2")]
    pub cursor: ::core::option::Option<Cursor>,
    #[prost(enumeration="SortDirection", tag="3")]
    pub direction: i32,
}
/// Envelope encapsulates a message while in transit.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Envelope {
    /// The topic the message belongs to,
    /// If the message includes the topic as well
    /// it MUST be the same as the topic in the envelope.
    #[prost(string, tag="1")]
    pub content_topic: ::prost::alloc::string::String,
    /// Message creation timestamp
    /// If the message includes the timestamp as well
    /// it MUST be equivalent to the timestamp in the envelope.
    #[prost(uint64, tag="2")]
    pub timestamp_ns: u64,
    #[prost(bytes="vec", tag="3")]
    pub message: ::prost::alloc::vec::Vec<u8>,
}
/// Publish
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublishRequest {
    #[prost(message, repeated, tag="1")]
    pub envelopes: ::prost::alloc::vec::Vec<Envelope>,
}
/// Empty message as a response for Publish
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublishResponse {
}
/// Subscribe
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscribeRequest {
    #[prost(string, repeated, tag="1")]
    pub content_topics: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// SubscribeAll
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscribeAllRequest {
}
/// Query
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryRequest {
    #[prost(string, repeated, tag="1")]
    pub content_topics: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(uint64, tag="2")]
    pub start_time_ns: u64,
    #[prost(uint64, tag="3")]
    pub end_time_ns: u64,
    #[prost(message, optional, tag="4")]
    pub paging_info: ::core::option::Option<PagingInfo>,
}
/// The response, containing envelopes, for a query
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryResponse {
    #[prost(message, repeated, tag="1")]
    pub envelopes: ::prost::alloc::vec::Vec<Envelope>,
    #[prost(message, optional, tag="2")]
    pub paging_info: ::core::option::Option<PagingInfo>,
}
/// BatchQuery
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BatchQueryRequest {
    #[prost(message, repeated, tag="1")]
    pub requests: ::prost::alloc::vec::Vec<QueryRequest>,
}
/// Response containing a list of QueryResponse messages
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BatchQueryResponse {
    #[prost(message, repeated, tag="1")]
    pub responses: ::prost::alloc::vec::Vec<QueryResponse>,
}
/// Sort direction
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SortDirection {
    Unspecified = 0,
    Ascending = 1,
    Descending = 2,
}
impl SortDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SortDirection::Unspecified => "SORT_DIRECTION_UNSPECIFIED",
            SortDirection::Ascending => "SORT_DIRECTION_ASCENDING",
            SortDirection::Descending => "SORT_DIRECTION_DESCENDING",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SORT_DIRECTION_UNSPECIFIED" => Some(Self::Unspecified),
            "SORT_DIRECTION_ASCENDING" => Some(Self::Ascending),
            "SORT_DIRECTION_DESCENDING" => Some(Self::Descending),
            _ => None,
        }
    }
}
include!("xmtp.message_api.v1.serde.rs");
include!("xmtp.message_api.v1.tonic.rs");
// @@protoc_insertion_point(module)