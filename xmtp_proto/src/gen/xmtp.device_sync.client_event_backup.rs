// @generated
// This file is @generated by prost-build.
/// Proto representation of a client record save
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientEventSave {
    #[prost(int64, tag="1")]
    pub created_at_ns: i64,
    #[prost(string, tag="2")]
    pub event: ::prost::alloc::string::String,
    #[prost(bytes="vec", tag="3")]
    pub details: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes="vec", optional, tag="4")]
    pub group_id: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
}
impl ::prost::Name for ClientEventSave {
const NAME: &'static str = "ClientEventSave";
const PACKAGE: &'static str = "xmtp.device_sync.client_event_backup";
fn full_name() -> ::prost::alloc::string::String { "xmtp.device_sync.client_event_backup.ClientEventSave".into() }fn type_url() -> ::prost::alloc::string::String { "/xmtp.device_sync.client_event_backup.ClientEventSave".into() }}
/// Encoded file descriptor set for the `xmtp.device_sync.client_event_backup` package
pub const FILE_DESCRIPTOR_SET: &[u8] = &[
    0x0a, 0xc1, 0x06, 0x0a, 0x25, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63,
    0x2f, 0x63, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x5f, 0x65, 0x76, 0x65, 0x6e, 0x74, 0x5f, 0x62, 0x61,
    0x63, 0x6b, 0x75, 0x70, 0x2e, 0x70, 0x72, 0x6f, 0x74, 0x6f, 0x12, 0x24, 0x78, 0x6d, 0x74, 0x70,
    0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63, 0x2e, 0x63, 0x6c, 0x69,
    0x65, 0x6e, 0x74, 0x5f, 0x65, 0x76, 0x65, 0x6e, 0x74, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70,
    0x22, 0x92, 0x01, 0x0a, 0x0f, 0x43, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x45, 0x76, 0x65, 0x6e, 0x74,
    0x53, 0x61, 0x76, 0x65, 0x12, 0x22, 0x0a, 0x0d, 0x63, 0x72, 0x65, 0x61, 0x74, 0x65, 0x64, 0x5f,
    0x61, 0x74, 0x5f, 0x6e, 0x73, 0x18, 0x01, 0x20, 0x01, 0x28, 0x03, 0x52, 0x0b, 0x63, 0x72, 0x65,
    0x61, 0x74, 0x65, 0x64, 0x41, 0x74, 0x4e, 0x73, 0x12, 0x14, 0x0a, 0x05, 0x65, 0x76, 0x65, 0x6e,
    0x74, 0x18, 0x02, 0x20, 0x01, 0x28, 0x09, 0x52, 0x05, 0x65, 0x76, 0x65, 0x6e, 0x74, 0x12, 0x18,
    0x0a, 0x07, 0x64, 0x65, 0x74, 0x61, 0x69, 0x6c, 0x73, 0x18, 0x03, 0x20, 0x01, 0x28, 0x0c, 0x52,
    0x07, 0x64, 0x65, 0x74, 0x61, 0x69, 0x6c, 0x73, 0x12, 0x1e, 0x0a, 0x08, 0x67, 0x72, 0x6f, 0x75,
    0x70, 0x5f, 0x69, 0x64, 0x18, 0x04, 0x20, 0x01, 0x28, 0x0c, 0x48, 0x00, 0x52, 0x07, 0x67, 0x72,
    0x6f, 0x75, 0x70, 0x49, 0x64, 0x88, 0x01, 0x01, 0x42, 0x0b, 0x0a, 0x09, 0x5f, 0x67, 0x72, 0x6f,
    0x75, 0x70, 0x5f, 0x69, 0x64, 0x42, 0xe8, 0x01, 0x0a, 0x28, 0x63, 0x6f, 0x6d, 0x2e, 0x78, 0x6d,
    0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63, 0x2e, 0x63,
    0x6c, 0x69, 0x65, 0x6e, 0x74, 0x5f, 0x65, 0x76, 0x65, 0x6e, 0x74, 0x5f, 0x62, 0x61, 0x63, 0x6b,
    0x75, 0x70, 0x42, 0x16, 0x43, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x45, 0x76, 0x65, 0x6e, 0x74, 0x42,
    0x61, 0x63, 0x6b, 0x75, 0x70, 0x50, 0x72, 0x6f, 0x74, 0x6f, 0x50, 0x01, 0xa2, 0x02, 0x03, 0x58,
    0x44, 0x43, 0xaa, 0x02, 0x21, 0x58, 0x6d, 0x74, 0x70, 0x2e, 0x44, 0x65, 0x76, 0x69, 0x63, 0x65,
    0x53, 0x79, 0x6e, 0x63, 0x2e, 0x43, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x45, 0x76, 0x65, 0x6e, 0x74,
    0x42, 0x61, 0x63, 0x6b, 0x75, 0x70, 0xca, 0x02, 0x21, 0x58, 0x6d, 0x74, 0x70, 0x5c, 0x44, 0x65,
    0x76, 0x69, 0x63, 0x65, 0x53, 0x79, 0x6e, 0x63, 0x5c, 0x43, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x45,
    0x76, 0x65, 0x6e, 0x74, 0x42, 0x61, 0x63, 0x6b, 0x75, 0x70, 0xe2, 0x02, 0x2d, 0x58, 0x6d, 0x74,
    0x70, 0x5c, 0x44, 0x65, 0x76, 0x69, 0x63, 0x65, 0x53, 0x79, 0x6e, 0x63, 0x5c, 0x43, 0x6c, 0x69,
    0x65, 0x6e, 0x74, 0x45, 0x76, 0x65, 0x6e, 0x74, 0x42, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x5c, 0x47,
    0x50, 0x42, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0xea, 0x02, 0x23, 0x58, 0x6d, 0x74,
    0x70, 0x3a, 0x3a, 0x44, 0x65, 0x76, 0x69, 0x63, 0x65, 0x53, 0x79, 0x6e, 0x63, 0x3a, 0x3a, 0x43,
    0x6c, 0x69, 0x65, 0x6e, 0x74, 0x45, 0x76, 0x65, 0x6e, 0x74, 0x42, 0x61, 0x63, 0x6b, 0x75, 0x70,
    0x4a, 0xe9, 0x02, 0x0a, 0x06, 0x12, 0x04, 0x01, 0x00, 0x0c, 0x01, 0x0a, 0x23, 0x0a, 0x01, 0x0c,
    0x12, 0x03, 0x01, 0x00, 0x12, 0x1a, 0x19, 0x20, 0x44, 0x65, 0x66, 0x69, 0x6e, 0x69, 0x74, 0x69,
    0x6f, 0x6e, 0x73, 0x20, 0x66, 0x6f, 0x72, 0x20, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x73, 0x0a,
    0x0a, 0x08, 0x0a, 0x01, 0x02, 0x12, 0x03, 0x02, 0x00, 0x2d, 0x0a, 0x3a, 0x0a, 0x02, 0x04, 0x00,
    0x12, 0x04, 0x07, 0x00, 0x0c, 0x01, 0x1a, 0x2e, 0x20, 0x50, 0x72, 0x6f, 0x74, 0x6f, 0x20, 0x72,
    0x65, 0x70, 0x72, 0x65, 0x73, 0x65, 0x6e, 0x74, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x20, 0x6f, 0x66,
    0x20, 0x61, 0x20, 0x63, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x20, 0x72, 0x65, 0x63, 0x6f, 0x72, 0x64,
    0x20, 0x73, 0x61, 0x76, 0x65, 0x0a, 0x0a, 0x0a, 0x0a, 0x03, 0x04, 0x00, 0x01, 0x12, 0x03, 0x07,
    0x08, 0x17, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x00, 0x12, 0x03, 0x08, 0x04, 0x1c, 0x0a,
    0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x00, 0x05, 0x12, 0x03, 0x08, 0x04, 0x09, 0x0a, 0x0c, 0x0a,
    0x05, 0x04, 0x00, 0x02, 0x00, 0x01, 0x12, 0x03, 0x08, 0x0a, 0x17, 0x0a, 0x0c, 0x0a, 0x05, 0x04,
    0x00, 0x02, 0x00, 0x03, 0x12, 0x03, 0x08, 0x1a, 0x1b, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02,
    0x01, 0x12, 0x03, 0x09, 0x04, 0x15, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x01, 0x05, 0x12,
    0x03, 0x09, 0x04, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x01, 0x01, 0x12, 0x03, 0x09,
    0x0b, 0x10, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x01, 0x03, 0x12, 0x03, 0x09, 0x13, 0x14,
    0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x02, 0x12, 0x03, 0x0a, 0x04, 0x16, 0x0a, 0x0c, 0x0a,
    0x05, 0x04, 0x00, 0x02, 0x02, 0x05, 0x12, 0x03, 0x0a, 0x04, 0x09, 0x0a, 0x0c, 0x0a, 0x05, 0x04,
    0x00, 0x02, 0x02, 0x01, 0x12, 0x03, 0x0a, 0x0a, 0x11, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02,
    0x02, 0x03, 0x12, 0x03, 0x0a, 0x14, 0x15, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x03, 0x12,
    0x03, 0x0b, 0x04, 0x20, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x03, 0x04, 0x12, 0x03, 0x0b,
    0x04, 0x0c, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x03, 0x05, 0x12, 0x03, 0x0b, 0x0d, 0x12,
    0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x03, 0x01, 0x12, 0x03, 0x0b, 0x13, 0x1b, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x03, 0x03, 0x12, 0x03, 0x0b, 0x1e, 0x1f, 0x62, 0x06, 0x70, 0x72,
    0x6f, 0x74, 0x6f, 0x33,
];
include!("xmtp.device_sync.client_event_backup.serde.rs");
// @@protoc_insertion_point(module)