// @generated
// This file is @generated by prost-build.
/// Proto representation of a stored group
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GroupSave {
    #[prost(bytes="vec", tag="1")]
    pub id: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag="2")]
    pub created_at_ns: i64,
    #[prost(enumeration="GroupMembershipStateSave", tag="3")]
    pub membership_state: i32,
    #[prost(int64, tag="4")]
    pub installations_last_checked: i64,
    #[prost(string, tag="5")]
    pub added_by_inbox_id: ::prost::alloc::string::String,
    #[prost(int64, optional, tag="6")]
    pub welcome_id: ::core::option::Option<i64>,
    #[prost(int64, tag="7")]
    pub rotated_at_ns: i64,
    #[prost(enumeration="ConversationTypeSave", tag="8")]
    pub conversation_type: i32,
    #[prost(string, optional, tag="9")]
    pub dm_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(int64, optional, tag="10")]
    pub last_message_ns: ::core::option::Option<i64>,
    #[prost(int64, optional, tag="11")]
    pub message_disappear_from_ns: ::core::option::Option<i64>,
    #[prost(int64, optional, tag="12")]
    pub message_disappear_in_ns: ::core::option::Option<i64>,
    /// metadata fields
    #[prost(message, optional, tag="13")]
    pub metadata: ::core::option::Option<ImmutableMetadataSave>,
    #[prost(message, optional, tag="14")]
    pub mutable_metadata: ::core::option::Option<MutableMetadataSave>,
    #[prost(string, optional, tag="15")]
    pub paused_for_version: ::core::option::Option<::prost::alloc::string::String>,
}
impl ::prost::Name for GroupSave {
const NAME: &'static str = "GroupSave";
const PACKAGE: &'static str = "xmtp.device_sync.group_backup";
fn full_name() -> ::prost::alloc::string::String { "xmtp.device_sync.group_backup.GroupSave".into() }fn type_url() -> ::prost::alloc::string::String { "/xmtp.device_sync.group_backup.GroupSave".into() }}
/// A Groups's mutable metadata
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MutableMetadataSave {
    #[prost(map="string, string", tag="1")]
    pub attributes: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    #[prost(string, repeated, tag="2")]
    pub admin_list: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, repeated, tag="3")]
    pub super_admin_list: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
impl ::prost::Name for MutableMetadataSave {
const NAME: &'static str = "MutableMetadataSave";
const PACKAGE: &'static str = "xmtp.device_sync.group_backup";
fn full_name() -> ::prost::alloc::string::String { "xmtp.device_sync.group_backup.MutableMetadataSave".into() }fn type_url() -> ::prost::alloc::string::String { "/xmtp.device_sync.group_backup.MutableMetadataSave".into() }}
/// A Group's immutable metadata
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ImmutableMetadataSave {
    #[prost(string, tag="1")]
    pub creator_inbox_id: ::prost::alloc::string::String,
}
impl ::prost::Name for ImmutableMetadataSave {
const NAME: &'static str = "ImmutableMetadataSave";
const PACKAGE: &'static str = "xmtp.device_sync.group_backup";
fn full_name() -> ::prost::alloc::string::String { "xmtp.device_sync.group_backup.ImmutableMetadataSave".into() }fn type_url() -> ::prost::alloc::string::String { "/xmtp.device_sync.group_backup.ImmutableMetadataSave".into() }}
/// Group membership state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum GroupMembershipStateSave {
    Unspecified = 0,
    Allowed = 1,
    Rejected = 2,
    Pending = 3,
    /// A group is marked as this state when it is restored
    /// from a backup. This is a non-functional archive state
    /// that can be reactivated when the user is re-added to
    /// the group.
    Restored = 4,
}
impl GroupMembershipStateSave {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            GroupMembershipStateSave::Unspecified => "GROUP_MEMBERSHIP_STATE_SAVE_UNSPECIFIED",
            GroupMembershipStateSave::Allowed => "GROUP_MEMBERSHIP_STATE_SAVE_ALLOWED",
            GroupMembershipStateSave::Rejected => "GROUP_MEMBERSHIP_STATE_SAVE_REJECTED",
            GroupMembershipStateSave::Pending => "GROUP_MEMBERSHIP_STATE_SAVE_PENDING",
            GroupMembershipStateSave::Restored => "GROUP_MEMBERSHIP_STATE_SAVE_RESTORED",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "GROUP_MEMBERSHIP_STATE_SAVE_UNSPECIFIED" => Some(Self::Unspecified),
            "GROUP_MEMBERSHIP_STATE_SAVE_ALLOWED" => Some(Self::Allowed),
            "GROUP_MEMBERSHIP_STATE_SAVE_REJECTED" => Some(Self::Rejected),
            "GROUP_MEMBERSHIP_STATE_SAVE_PENDING" => Some(Self::Pending),
            "GROUP_MEMBERSHIP_STATE_SAVE_RESTORED" => Some(Self::Restored),
            _ => None,
        }
    }
}
/// Conversation type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ConversationTypeSave {
    Unspecified = 0,
    Group = 1,
    Dm = 2,
    Sync = 3,
}
impl ConversationTypeSave {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ConversationTypeSave::Unspecified => "CONVERSATION_TYPE_SAVE_UNSPECIFIED",
            ConversationTypeSave::Group => "CONVERSATION_TYPE_SAVE_GROUP",
            ConversationTypeSave::Dm => "CONVERSATION_TYPE_SAVE_DM",
            ConversationTypeSave::Sync => "CONVERSATION_TYPE_SAVE_SYNC",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "CONVERSATION_TYPE_SAVE_UNSPECIFIED" => Some(Self::Unspecified),
            "CONVERSATION_TYPE_SAVE_GROUP" => Some(Self::Group),
            "CONVERSATION_TYPE_SAVE_DM" => Some(Self::Dm),
            "CONVERSATION_TYPE_SAVE_SYNC" => Some(Self::Sync),
            _ => None,
        }
    }
}
/// Encoded file descriptor set for the `xmtp.device_sync.group_backup` package
pub const FILE_DESCRIPTOR_SET: &[u8] = &[
    0x0a, 0xca, 0x1f, 0x0a, 0x1e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63,
    0x2f, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x2e, 0x70, 0x72,
    0x6f, 0x74, 0x6f, 0x12, 0x1d, 0x78, 0x6d, 0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65,
    0x5f, 0x73, 0x79, 0x6e, 0x63, 0x2e, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b,
    0x75, 0x70, 0x22, 0xdb, 0x07, 0x0a, 0x09, 0x47, 0x72, 0x6f, 0x75, 0x70, 0x53, 0x61, 0x76, 0x65,
    0x12, 0x0e, 0x0a, 0x02, 0x69, 0x64, 0x18, 0x01, 0x20, 0x01, 0x28, 0x0c, 0x52, 0x02, 0x69, 0x64,
    0x12, 0x22, 0x0a, 0x0d, 0x63, 0x72, 0x65, 0x61, 0x74, 0x65, 0x64, 0x5f, 0x61, 0x74, 0x5f, 0x6e,
    0x73, 0x18, 0x02, 0x20, 0x01, 0x28, 0x03, 0x52, 0x0b, 0x63, 0x72, 0x65, 0x61, 0x74, 0x65, 0x64,
    0x41, 0x74, 0x4e, 0x73, 0x12, 0x62, 0x0a, 0x10, 0x6d, 0x65, 0x6d, 0x62, 0x65, 0x72, 0x73, 0x68,
    0x69, 0x70, 0x5f, 0x73, 0x74, 0x61, 0x74, 0x65, 0x18, 0x03, 0x20, 0x01, 0x28, 0x0e, 0x32, 0x37,
    0x2e, 0x78, 0x6d, 0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e,
    0x63, 0x2e, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x2e, 0x47,
    0x72, 0x6f, 0x75, 0x70, 0x4d, 0x65, 0x6d, 0x62, 0x65, 0x72, 0x73, 0x68, 0x69, 0x70, 0x53, 0x74,
    0x61, 0x74, 0x65, 0x53, 0x61, 0x76, 0x65, 0x52, 0x0f, 0x6d, 0x65, 0x6d, 0x62, 0x65, 0x72, 0x73,
    0x68, 0x69, 0x70, 0x53, 0x74, 0x61, 0x74, 0x65, 0x12, 0x3c, 0x0a, 0x1a, 0x69, 0x6e, 0x73, 0x74,
    0x61, 0x6c, 0x6c, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x73, 0x5f, 0x6c, 0x61, 0x73, 0x74, 0x5f, 0x63,
    0x68, 0x65, 0x63, 0x6b, 0x65, 0x64, 0x18, 0x04, 0x20, 0x01, 0x28, 0x03, 0x52, 0x18, 0x69, 0x6e,
    0x73, 0x74, 0x61, 0x6c, 0x6c, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x73, 0x4c, 0x61, 0x73, 0x74, 0x43,
    0x68, 0x65, 0x63, 0x6b, 0x65, 0x64, 0x12, 0x29, 0x0a, 0x11, 0x61, 0x64, 0x64, 0x65, 0x64, 0x5f,
    0x62, 0x79, 0x5f, 0x69, 0x6e, 0x62, 0x6f, 0x78, 0x5f, 0x69, 0x64, 0x18, 0x05, 0x20, 0x01, 0x28,
    0x09, 0x52, 0x0e, 0x61, 0x64, 0x64, 0x65, 0x64, 0x42, 0x79, 0x49, 0x6e, 0x62, 0x6f, 0x78, 0x49,
    0x64, 0x12, 0x22, 0x0a, 0x0a, 0x77, 0x65, 0x6c, 0x63, 0x6f, 0x6d, 0x65, 0x5f, 0x69, 0x64, 0x18,
    0x06, 0x20, 0x01, 0x28, 0x03, 0x48, 0x00, 0x52, 0x09, 0x77, 0x65, 0x6c, 0x63, 0x6f, 0x6d, 0x65,
    0x49, 0x64, 0x88, 0x01, 0x01, 0x12, 0x22, 0x0a, 0x0d, 0x72, 0x6f, 0x74, 0x61, 0x74, 0x65, 0x64,
    0x5f, 0x61, 0x74, 0x5f, 0x6e, 0x73, 0x18, 0x07, 0x20, 0x01, 0x28, 0x03, 0x52, 0x0b, 0x72, 0x6f,
    0x74, 0x61, 0x74, 0x65, 0x64, 0x41, 0x74, 0x4e, 0x73, 0x12, 0x60, 0x0a, 0x11, 0x63, 0x6f, 0x6e,
    0x76, 0x65, 0x72, 0x73, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x5f, 0x74, 0x79, 0x70, 0x65, 0x18, 0x08,
    0x20, 0x01, 0x28, 0x0e, 0x32, 0x33, 0x2e, 0x78, 0x6d, 0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69,
    0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63, 0x2e, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61,
    0x63, 0x6b, 0x75, 0x70, 0x2e, 0x43, 0x6f, 0x6e, 0x76, 0x65, 0x72, 0x73, 0x61, 0x74, 0x69, 0x6f,
    0x6e, 0x54, 0x79, 0x70, 0x65, 0x53, 0x61, 0x76, 0x65, 0x52, 0x10, 0x63, 0x6f, 0x6e, 0x76, 0x65,
    0x72, 0x73, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x54, 0x79, 0x70, 0x65, 0x12, 0x18, 0x0a, 0x05, 0x64,
    0x6d, 0x5f, 0x69, 0x64, 0x18, 0x09, 0x20, 0x01, 0x28, 0x09, 0x48, 0x01, 0x52, 0x04, 0x64, 0x6d,
    0x49, 0x64, 0x88, 0x01, 0x01, 0x12, 0x2b, 0x0a, 0x0f, 0x6c, 0x61, 0x73, 0x74, 0x5f, 0x6d, 0x65,
    0x73, 0x73, 0x61, 0x67, 0x65, 0x5f, 0x6e, 0x73, 0x18, 0x0a, 0x20, 0x01, 0x28, 0x03, 0x48, 0x02,
    0x52, 0x0d, 0x6c, 0x61, 0x73, 0x74, 0x4d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x4e, 0x73, 0x88,
    0x01, 0x01, 0x12, 0x3e, 0x0a, 0x19, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x5f, 0x64, 0x69,
    0x73, 0x61, 0x70, 0x70, 0x65, 0x61, 0x72, 0x5f, 0x66, 0x72, 0x6f, 0x6d, 0x5f, 0x6e, 0x73, 0x18,
    0x0b, 0x20, 0x01, 0x28, 0x03, 0x48, 0x03, 0x52, 0x16, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65,
    0x44, 0x69, 0x73, 0x61, 0x70, 0x70, 0x65, 0x61, 0x72, 0x46, 0x72, 0x6f, 0x6d, 0x4e, 0x73, 0x88,
    0x01, 0x01, 0x12, 0x3a, 0x0a, 0x17, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x5f, 0x64, 0x69,
    0x73, 0x61, 0x70, 0x70, 0x65, 0x61, 0x72, 0x5f, 0x69, 0x6e, 0x5f, 0x6e, 0x73, 0x18, 0x0c, 0x20,
    0x01, 0x28, 0x03, 0x48, 0x04, 0x52, 0x14, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x44, 0x69,
    0x73, 0x61, 0x70, 0x70, 0x65, 0x61, 0x72, 0x49, 0x6e, 0x4e, 0x73, 0x88, 0x01, 0x01, 0x12, 0x50,
    0x0a, 0x08, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x18, 0x0d, 0x20, 0x01, 0x28, 0x0b,
    0x32, 0x34, 0x2e, 0x78, 0x6d, 0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73,
    0x79, 0x6e, 0x63, 0x2e, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70,
    0x2e, 0x49, 0x6d, 0x6d, 0x75, 0x74, 0x61, 0x62, 0x6c, 0x65, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61,
    0x74, 0x61, 0x53, 0x61, 0x76, 0x65, 0x52, 0x08, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61,
    0x12, 0x5d, 0x0a, 0x10, 0x6d, 0x75, 0x74, 0x61, 0x62, 0x6c, 0x65, 0x5f, 0x6d, 0x65, 0x74, 0x61,
    0x64, 0x61, 0x74, 0x61, 0x18, 0x0e, 0x20, 0x01, 0x28, 0x0b, 0x32, 0x32, 0x2e, 0x78, 0x6d, 0x74,
    0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63, 0x2e, 0x67, 0x72,
    0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x2e, 0x4d, 0x75, 0x74, 0x61, 0x62,
    0x6c, 0x65, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x53, 0x61, 0x76, 0x65, 0x52, 0x0f,
    0x6d, 0x75, 0x74, 0x61, 0x62, 0x6c, 0x65, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x12,
    0x31, 0x0a, 0x12, 0x70, 0x61, 0x75, 0x73, 0x65, 0x64, 0x5f, 0x66, 0x6f, 0x72, 0x5f, 0x76, 0x65,
    0x72, 0x73, 0x69, 0x6f, 0x6e, 0x18, 0x0f, 0x20, 0x01, 0x28, 0x09, 0x48, 0x05, 0x52, 0x10, 0x70,
    0x61, 0x75, 0x73, 0x65, 0x64, 0x46, 0x6f, 0x72, 0x56, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, 0x88,
    0x01, 0x01, 0x42, 0x0d, 0x0a, 0x0b, 0x5f, 0x77, 0x65, 0x6c, 0x63, 0x6f, 0x6d, 0x65, 0x5f, 0x69,
    0x64, 0x42, 0x08, 0x0a, 0x06, 0x5f, 0x64, 0x6d, 0x5f, 0x69, 0x64, 0x42, 0x12, 0x0a, 0x10, 0x5f,
    0x6c, 0x61, 0x73, 0x74, 0x5f, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x5f, 0x6e, 0x73, 0x42,
    0x1c, 0x0a, 0x1a, 0x5f, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x5f, 0x64, 0x69, 0x73, 0x61,
    0x70, 0x70, 0x65, 0x61, 0x72, 0x5f, 0x66, 0x72, 0x6f, 0x6d, 0x5f, 0x6e, 0x73, 0x42, 0x1a, 0x0a,
    0x18, 0x5f, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x5f, 0x64, 0x69, 0x73, 0x61, 0x70, 0x70,
    0x65, 0x61, 0x72, 0x5f, 0x69, 0x6e, 0x5f, 0x6e, 0x73, 0x42, 0x15, 0x0a, 0x13, 0x5f, 0x70, 0x61,
    0x75, 0x73, 0x65, 0x64, 0x5f, 0x66, 0x6f, 0x72, 0x5f, 0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e,
    0x22, 0x81, 0x02, 0x0a, 0x13, 0x4d, 0x75, 0x74, 0x61, 0x62, 0x6c, 0x65, 0x4d, 0x65, 0x74, 0x61,
    0x64, 0x61, 0x74, 0x61, 0x53, 0x61, 0x76, 0x65, 0x12, 0x62, 0x0a, 0x0a, 0x61, 0x74, 0x74, 0x72,
    0x69, 0x62, 0x75, 0x74, 0x65, 0x73, 0x18, 0x01, 0x20, 0x03, 0x28, 0x0b, 0x32, 0x42, 0x2e, 0x78,
    0x6d, 0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73, 0x79, 0x6e, 0x63, 0x2e,
    0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x2e, 0x4d, 0x75, 0x74,
    0x61, 0x62, 0x6c, 0x65, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x53, 0x61, 0x76, 0x65,
    0x2e, 0x41, 0x74, 0x74, 0x72, 0x69, 0x62, 0x75, 0x74, 0x65, 0x73, 0x45, 0x6e, 0x74, 0x72, 0x79,
    0x52, 0x0a, 0x61, 0x74, 0x74, 0x72, 0x69, 0x62, 0x75, 0x74, 0x65, 0x73, 0x12, 0x1d, 0x0a, 0x0a,
    0x61, 0x64, 0x6d, 0x69, 0x6e, 0x5f, 0x6c, 0x69, 0x73, 0x74, 0x18, 0x02, 0x20, 0x03, 0x28, 0x09,
    0x52, 0x09, 0x61, 0x64, 0x6d, 0x69, 0x6e, 0x4c, 0x69, 0x73, 0x74, 0x12, 0x28, 0x0a, 0x10, 0x73,
    0x75, 0x70, 0x65, 0x72, 0x5f, 0x61, 0x64, 0x6d, 0x69, 0x6e, 0x5f, 0x6c, 0x69, 0x73, 0x74, 0x18,
    0x03, 0x20, 0x03, 0x28, 0x09, 0x52, 0x0e, 0x73, 0x75, 0x70, 0x65, 0x72, 0x41, 0x64, 0x6d, 0x69,
    0x6e, 0x4c, 0x69, 0x73, 0x74, 0x1a, 0x3d, 0x0a, 0x0f, 0x41, 0x74, 0x74, 0x72, 0x69, 0x62, 0x75,
    0x74, 0x65, 0x73, 0x45, 0x6e, 0x74, 0x72, 0x79, 0x12, 0x10, 0x0a, 0x03, 0x6b, 0x65, 0x79, 0x18,
    0x01, 0x20, 0x01, 0x28, 0x09, 0x52, 0x03, 0x6b, 0x65, 0x79, 0x12, 0x14, 0x0a, 0x05, 0x76, 0x61,
    0x6c, 0x75, 0x65, 0x18, 0x02, 0x20, 0x01, 0x28, 0x09, 0x52, 0x05, 0x76, 0x61, 0x6c, 0x75, 0x65,
    0x3a, 0x02, 0x38, 0x01, 0x22, 0x41, 0x0a, 0x15, 0x49, 0x6d, 0x6d, 0x75, 0x74, 0x61, 0x62, 0x6c,
    0x65, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x53, 0x61, 0x76, 0x65, 0x12, 0x28, 0x0a,
    0x10, 0x63, 0x72, 0x65, 0x61, 0x74, 0x6f, 0x72, 0x5f, 0x69, 0x6e, 0x62, 0x6f, 0x78, 0x5f, 0x69,
    0x64, 0x18, 0x01, 0x20, 0x01, 0x28, 0x09, 0x52, 0x0e, 0x63, 0x72, 0x65, 0x61, 0x74, 0x6f, 0x72,
    0x49, 0x6e, 0x62, 0x6f, 0x78, 0x49, 0x64, 0x2a, 0xed, 0x01, 0x0a, 0x18, 0x47, 0x72, 0x6f, 0x75,
    0x70, 0x4d, 0x65, 0x6d, 0x62, 0x65, 0x72, 0x73, 0x68, 0x69, 0x70, 0x53, 0x74, 0x61, 0x74, 0x65,
    0x53, 0x61, 0x76, 0x65, 0x12, 0x2b, 0x0a, 0x27, 0x47, 0x52, 0x4f, 0x55, 0x50, 0x5f, 0x4d, 0x45,
    0x4d, 0x42, 0x45, 0x52, 0x53, 0x48, 0x49, 0x50, 0x5f, 0x53, 0x54, 0x41, 0x54, 0x45, 0x5f, 0x53,
    0x41, 0x56, 0x45, 0x5f, 0x55, 0x4e, 0x53, 0x50, 0x45, 0x43, 0x49, 0x46, 0x49, 0x45, 0x44, 0x10,
    0x00, 0x12, 0x27, 0x0a, 0x23, 0x47, 0x52, 0x4f, 0x55, 0x50, 0x5f, 0x4d, 0x45, 0x4d, 0x42, 0x45,
    0x52, 0x53, 0x48, 0x49, 0x50, 0x5f, 0x53, 0x54, 0x41, 0x54, 0x45, 0x5f, 0x53, 0x41, 0x56, 0x45,
    0x5f, 0x41, 0x4c, 0x4c, 0x4f, 0x57, 0x45, 0x44, 0x10, 0x01, 0x12, 0x28, 0x0a, 0x24, 0x47, 0x52,
    0x4f, 0x55, 0x50, 0x5f, 0x4d, 0x45, 0x4d, 0x42, 0x45, 0x52, 0x53, 0x48, 0x49, 0x50, 0x5f, 0x53,
    0x54, 0x41, 0x54, 0x45, 0x5f, 0x53, 0x41, 0x56, 0x45, 0x5f, 0x52, 0x45, 0x4a, 0x45, 0x43, 0x54,
    0x45, 0x44, 0x10, 0x02, 0x12, 0x27, 0x0a, 0x23, 0x47, 0x52, 0x4f, 0x55, 0x50, 0x5f, 0x4d, 0x45,
    0x4d, 0x42, 0x45, 0x52, 0x53, 0x48, 0x49, 0x50, 0x5f, 0x53, 0x54, 0x41, 0x54, 0x45, 0x5f, 0x53,
    0x41, 0x56, 0x45, 0x5f, 0x50, 0x45, 0x4e, 0x44, 0x49, 0x4e, 0x47, 0x10, 0x03, 0x12, 0x28, 0x0a,
    0x24, 0x47, 0x52, 0x4f, 0x55, 0x50, 0x5f, 0x4d, 0x45, 0x4d, 0x42, 0x45, 0x52, 0x53, 0x48, 0x49,
    0x50, 0x5f, 0x53, 0x54, 0x41, 0x54, 0x45, 0x5f, 0x53, 0x41, 0x56, 0x45, 0x5f, 0x52, 0x45, 0x53,
    0x54, 0x4f, 0x52, 0x45, 0x44, 0x10, 0x04, 0x2a, 0xa0, 0x01, 0x0a, 0x14, 0x43, 0x6f, 0x6e, 0x76,
    0x65, 0x72, 0x73, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x54, 0x79, 0x70, 0x65, 0x53, 0x61, 0x76, 0x65,
    0x12, 0x26, 0x0a, 0x22, 0x43, 0x4f, 0x4e, 0x56, 0x45, 0x52, 0x53, 0x41, 0x54, 0x49, 0x4f, 0x4e,
    0x5f, 0x54, 0x59, 0x50, 0x45, 0x5f, 0x53, 0x41, 0x56, 0x45, 0x5f, 0x55, 0x4e, 0x53, 0x50, 0x45,
    0x43, 0x49, 0x46, 0x49, 0x45, 0x44, 0x10, 0x00, 0x12, 0x20, 0x0a, 0x1c, 0x43, 0x4f, 0x4e, 0x56,
    0x45, 0x52, 0x53, 0x41, 0x54, 0x49, 0x4f, 0x4e, 0x5f, 0x54, 0x59, 0x50, 0x45, 0x5f, 0x53, 0x41,
    0x56, 0x45, 0x5f, 0x47, 0x52, 0x4f, 0x55, 0x50, 0x10, 0x01, 0x12, 0x1d, 0x0a, 0x19, 0x43, 0x4f,
    0x4e, 0x56, 0x45, 0x52, 0x53, 0x41, 0x54, 0x49, 0x4f, 0x4e, 0x5f, 0x54, 0x59, 0x50, 0x45, 0x5f,
    0x53, 0x41, 0x56, 0x45, 0x5f, 0x44, 0x4d, 0x10, 0x02, 0x12, 0x1f, 0x0a, 0x1b, 0x43, 0x4f, 0x4e,
    0x56, 0x45, 0x52, 0x53, 0x41, 0x54, 0x49, 0x4f, 0x4e, 0x5f, 0x54, 0x59, 0x50, 0x45, 0x5f, 0x53,
    0x41, 0x56, 0x45, 0x5f, 0x53, 0x59, 0x4e, 0x43, 0x10, 0x03, 0x42, 0xc3, 0x01, 0x0a, 0x21, 0x63,
    0x6f, 0x6d, 0x2e, 0x78, 0x6d, 0x74, 0x70, 0x2e, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x5f, 0x73,
    0x79, 0x6e, 0x63, 0x2e, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x5f, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70,
    0x42, 0x10, 0x47, 0x72, 0x6f, 0x75, 0x70, 0x42, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x50, 0x72, 0x6f,
    0x74, 0x6f, 0x50, 0x01, 0xa2, 0x02, 0x03, 0x58, 0x44, 0x47, 0xaa, 0x02, 0x1b, 0x58, 0x6d, 0x74,
    0x70, 0x2e, 0x44, 0x65, 0x76, 0x69, 0x63, 0x65, 0x53, 0x79, 0x6e, 0x63, 0x2e, 0x47, 0x72, 0x6f,
    0x75, 0x70, 0x42, 0x61, 0x63, 0x6b, 0x75, 0x70, 0xca, 0x02, 0x1b, 0x58, 0x6d, 0x74, 0x70, 0x5c,
    0x44, 0x65, 0x76, 0x69, 0x63, 0x65, 0x53, 0x79, 0x6e, 0x63, 0x5c, 0x47, 0x72, 0x6f, 0x75, 0x70,
    0x42, 0x61, 0x63, 0x6b, 0x75, 0x70, 0xe2, 0x02, 0x27, 0x58, 0x6d, 0x74, 0x70, 0x5c, 0x44, 0x65,
    0x76, 0x69, 0x63, 0x65, 0x53, 0x79, 0x6e, 0x63, 0x5c, 0x47, 0x72, 0x6f, 0x75, 0x70, 0x42, 0x61,
    0x63, 0x6b, 0x75, 0x70, 0x5c, 0x47, 0x50, 0x42, 0x4d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61,
    0xea, 0x02, 0x1d, 0x58, 0x6d, 0x74, 0x70, 0x3a, 0x3a, 0x44, 0x65, 0x76, 0x69, 0x63, 0x65, 0x53,
    0x79, 0x6e, 0x63, 0x3a, 0x3a, 0x47, 0x72, 0x6f, 0x75, 0x70, 0x42, 0x61, 0x63, 0x6b, 0x75, 0x70,
    0x4a, 0x82, 0x10, 0x0a, 0x06, 0x12, 0x04, 0x01, 0x00, 0x3f, 0x01, 0x0a, 0x23, 0x0a, 0x01, 0x0c,
    0x12, 0x03, 0x01, 0x00, 0x12, 0x1a, 0x19, 0x20, 0x44, 0x65, 0x66, 0x69, 0x6e, 0x69, 0x74, 0x69,
    0x6f, 0x6e, 0x73, 0x20, 0x66, 0x6f, 0x72, 0x20, 0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x73, 0x0a,
    0x0a, 0x08, 0x0a, 0x01, 0x02, 0x12, 0x03, 0x02, 0x00, 0x26, 0x0a, 0x34, 0x0a, 0x02, 0x04, 0x00,
    0x12, 0x04, 0x07, 0x00, 0x1a, 0x01, 0x1a, 0x28, 0x20, 0x50, 0x72, 0x6f, 0x74, 0x6f, 0x20, 0x72,
    0x65, 0x70, 0x72, 0x65, 0x73, 0x65, 0x6e, 0x74, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x20, 0x6f, 0x66,
    0x20, 0x61, 0x20, 0x73, 0x74, 0x6f, 0x72, 0x65, 0x64, 0x20, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x0a,
    0x0a, 0x0a, 0x0a, 0x03, 0x04, 0x00, 0x01, 0x12, 0x03, 0x07, 0x08, 0x11, 0x0a, 0x0b, 0x0a, 0x04,
    0x04, 0x00, 0x02, 0x00, 0x12, 0x03, 0x08, 0x02, 0x0f, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02,
    0x00, 0x05, 0x12, 0x03, 0x08, 0x02, 0x07, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x00, 0x01,
    0x12, 0x03, 0x08, 0x08, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x00, 0x03, 0x12, 0x03,
    0x08, 0x0d, 0x0e, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x01, 0x12, 0x03, 0x09, 0x02, 0x1a,
    0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x01, 0x05, 0x12, 0x03, 0x09, 0x02, 0x07, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x01, 0x01, 0x12, 0x03, 0x09, 0x08, 0x15, 0x0a, 0x0c, 0x0a, 0x05,
    0x04, 0x00, 0x02, 0x01, 0x03, 0x12, 0x03, 0x09, 0x18, 0x19, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00,
    0x02, 0x02, 0x12, 0x03, 0x0a, 0x02, 0x30, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x06,
    0x12, 0x03, 0x0a, 0x02, 0x1a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x01, 0x12, 0x03,
    0x0a, 0x1b, 0x2b, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x03, 0x12, 0x03, 0x0a, 0x2e,
    0x2f, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x03, 0x12, 0x03, 0x0b, 0x02, 0x27, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x03, 0x05, 0x12, 0x03, 0x0b, 0x02, 0x07, 0x0a, 0x0c, 0x0a, 0x05,
    0x04, 0x00, 0x02, 0x03, 0x01, 0x12, 0x03, 0x0b, 0x08, 0x22, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00,
    0x02, 0x03, 0x03, 0x12, 0x03, 0x0b, 0x25, 0x26, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x04,
    0x12, 0x03, 0x0c, 0x02, 0x1f, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x04, 0x05, 0x12, 0x03,
    0x0c, 0x02, 0x08, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x04, 0x01, 0x12, 0x03, 0x0c, 0x09,
    0x1a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x04, 0x03, 0x12, 0x03, 0x0c, 0x1d, 0x1e, 0x0a,
    0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x05, 0x12, 0x03, 0x0d, 0x02, 0x20, 0x0a, 0x0c, 0x0a, 0x05,
    0x04, 0x00, 0x02, 0x05, 0x04, 0x12, 0x03, 0x0d, 0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00,
    0x02, 0x05, 0x05, 0x12, 0x03, 0x0d, 0x0b, 0x10, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x05,
    0x01, 0x12, 0x03, 0x0d, 0x11, 0x1b, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x05, 0x03, 0x12,
    0x03, 0x0d, 0x1e, 0x1f, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x06, 0x12, 0x03, 0x0e, 0x02,
    0x1a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x06, 0x05, 0x12, 0x03, 0x0e, 0x02, 0x07, 0x0a,
    0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x06, 0x01, 0x12, 0x03, 0x0e, 0x08, 0x15, 0x0a, 0x0c, 0x0a,
    0x05, 0x04, 0x00, 0x02, 0x06, 0x03, 0x12, 0x03, 0x0e, 0x18, 0x19, 0x0a, 0x0b, 0x0a, 0x04, 0x04,
    0x00, 0x02, 0x07, 0x12, 0x03, 0x0f, 0x02, 0x2d, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x07,
    0x06, 0x12, 0x03, 0x0f, 0x02, 0x16, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x07, 0x01, 0x12,
    0x03, 0x0f, 0x17, 0x28, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x07, 0x03, 0x12, 0x03, 0x0f,
    0x2b, 0x2c, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x08, 0x12, 0x03, 0x10, 0x02, 0x1c, 0x0a,
    0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x08, 0x04, 0x12, 0x03, 0x10, 0x02, 0x0a, 0x0a, 0x0c, 0x0a,
    0x05, 0x04, 0x00, 0x02, 0x08, 0x05, 0x12, 0x03, 0x10, 0x0b, 0x11, 0x0a, 0x0c, 0x0a, 0x05, 0x04,
    0x00, 0x02, 0x08, 0x01, 0x12, 0x03, 0x10, 0x12, 0x17, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02,
    0x08, 0x03, 0x12, 0x03, 0x10, 0x1a, 0x1b, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x09, 0x12,
    0x03, 0x11, 0x02, 0x26, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x09, 0x04, 0x12, 0x03, 0x11,
    0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x09, 0x05, 0x12, 0x03, 0x11, 0x0b, 0x10,
    0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x09, 0x01, 0x12, 0x03, 0x11, 0x11, 0x20, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x09, 0x03, 0x12, 0x03, 0x11, 0x23, 0x25, 0x0a, 0x0b, 0x0a, 0x04,
    0x04, 0x00, 0x02, 0x0a, 0x12, 0x03, 0x12, 0x02, 0x30, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02,
    0x0a, 0x04, 0x12, 0x03, 0x12, 0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0a, 0x05,
    0x12, 0x03, 0x12, 0x0b, 0x10, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0a, 0x01, 0x12, 0x03,
    0x12, 0x11, 0x2a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0a, 0x03, 0x12, 0x03, 0x12, 0x2d,
    0x2f, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x0b, 0x12, 0x03, 0x13, 0x02, 0x2e, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x0b, 0x04, 0x12, 0x03, 0x13, 0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05,
    0x04, 0x00, 0x02, 0x0b, 0x05, 0x12, 0x03, 0x13, 0x0b, 0x10, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00,
    0x02, 0x0b, 0x01, 0x12, 0x03, 0x13, 0x11, 0x28, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0b,
    0x03, 0x12, 0x03, 0x13, 0x2b, 0x2d, 0x0a, 0x1e, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x0c, 0x12, 0x03,
    0x16, 0x02, 0x26, 0x1a, 0x11, 0x20, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x20, 0x66,
    0x69, 0x65, 0x6c, 0x64, 0x73, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0c, 0x06, 0x12,
    0x03, 0x16, 0x02, 0x17, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0c, 0x01, 0x12, 0x03, 0x16,
    0x18, 0x20, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0c, 0x03, 0x12, 0x03, 0x16, 0x23, 0x25,
    0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x0d, 0x12, 0x03, 0x17, 0x02, 0x2c, 0x0a, 0x0c, 0x0a,
    0x05, 0x04, 0x00, 0x02, 0x0d, 0x06, 0x12, 0x03, 0x17, 0x02, 0x15, 0x0a, 0x0c, 0x0a, 0x05, 0x04,
    0x00, 0x02, 0x0d, 0x01, 0x12, 0x03, 0x17, 0x16, 0x26, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02,
    0x0d, 0x03, 0x12, 0x03, 0x17, 0x29, 0x2b, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x0e, 0x12,
    0x03, 0x19, 0x02, 0x2a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0e, 0x04, 0x12, 0x03, 0x19,
    0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0e, 0x05, 0x12, 0x03, 0x19, 0x0b, 0x11,
    0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x0e, 0x01, 0x12, 0x03, 0x19, 0x12, 0x24, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x0e, 0x03, 0x12, 0x03, 0x19, 0x27, 0x29, 0x0a, 0x24, 0x0a, 0x02,
    0x05, 0x00, 0x12, 0x04, 0x1d, 0x00, 0x27, 0x01, 0x1a, 0x18, 0x20, 0x47, 0x72, 0x6f, 0x75, 0x70,
    0x20, 0x6d, 0x65, 0x6d, 0x62, 0x65, 0x72, 0x73, 0x68, 0x69, 0x70, 0x20, 0x73, 0x74, 0x61, 0x74,
    0x65, 0x0a, 0x0a, 0x0a, 0x0a, 0x03, 0x05, 0x00, 0x01, 0x12, 0x03, 0x1d, 0x05, 0x1d, 0x0a, 0x0b,
    0x0a, 0x04, 0x05, 0x00, 0x02, 0x00, 0x12, 0x03, 0x1e, 0x02, 0x2e, 0x0a, 0x0c, 0x0a, 0x05, 0x05,
    0x00, 0x02, 0x00, 0x01, 0x12, 0x03, 0x1e, 0x02, 0x29, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02,
    0x00, 0x02, 0x12, 0x03, 0x1e, 0x2c, 0x2d, 0x0a, 0x0b, 0x0a, 0x04, 0x05, 0x00, 0x02, 0x01, 0x12,
    0x03, 0x1f, 0x02, 0x2a, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02, 0x01, 0x01, 0x12, 0x03, 0x1f,
    0x02, 0x25, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02, 0x01, 0x02, 0x12, 0x03, 0x1f, 0x28, 0x29,
    0x0a, 0x0b, 0x0a, 0x04, 0x05, 0x00, 0x02, 0x02, 0x12, 0x03, 0x20, 0x02, 0x2b, 0x0a, 0x0c, 0x0a,
    0x05, 0x05, 0x00, 0x02, 0x02, 0x01, 0x12, 0x03, 0x20, 0x02, 0x26, 0x0a, 0x0c, 0x0a, 0x05, 0x05,
    0x00, 0x02, 0x02, 0x02, 0x12, 0x03, 0x20, 0x29, 0x2a, 0x0a, 0x0b, 0x0a, 0x04, 0x05, 0x00, 0x02,
    0x03, 0x12, 0x03, 0x21, 0x02, 0x2a, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02, 0x03, 0x01, 0x12,
    0x03, 0x21, 0x02, 0x25, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02, 0x03, 0x02, 0x12, 0x03, 0x21,
    0x28, 0x29, 0x0a, 0xbc, 0x01, 0x0a, 0x04, 0x05, 0x00, 0x02, 0x04, 0x12, 0x03, 0x26, 0x02, 0x2b,
    0x1a, 0xae, 0x01, 0x20, 0x41, 0x20, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x20, 0x69, 0x73, 0x20, 0x6d,
    0x61, 0x72, 0x6b, 0x65, 0x64, 0x20, 0x61, 0x73, 0x20, 0x74, 0x68, 0x69, 0x73, 0x20, 0x73, 0x74,
    0x61, 0x74, 0x65, 0x20, 0x77, 0x68, 0x65, 0x6e, 0x20, 0x69, 0x74, 0x20, 0x69, 0x73, 0x20, 0x72,
    0x65, 0x73, 0x74, 0x6f, 0x72, 0x65, 0x64, 0x0a, 0x20, 0x66, 0x72, 0x6f, 0x6d, 0x20, 0x61, 0x20,
    0x62, 0x61, 0x63, 0x6b, 0x75, 0x70, 0x2e, 0x20, 0x54, 0x68, 0x69, 0x73, 0x20, 0x69, 0x73, 0x20,
    0x61, 0x20, 0x6e, 0x6f, 0x6e, 0x2d, 0x66, 0x75, 0x6e, 0x63, 0x74, 0x69, 0x6f, 0x6e, 0x61, 0x6c,
    0x20, 0x61, 0x72, 0x63, 0x68, 0x69, 0x76, 0x65, 0x20, 0x73, 0x74, 0x61, 0x74, 0x65, 0x0a, 0x20,
    0x74, 0x68, 0x61, 0x74, 0x20, 0x63, 0x61, 0x6e, 0x20, 0x62, 0x65, 0x20, 0x72, 0x65, 0x61, 0x63,
    0x74, 0x69, 0x76, 0x61, 0x74, 0x65, 0x64, 0x20, 0x77, 0x68, 0x65, 0x6e, 0x20, 0x74, 0x68, 0x65,
    0x20, 0x75, 0x73, 0x65, 0x72, 0x20, 0x69, 0x73, 0x20, 0x72, 0x65, 0x2d, 0x61, 0x64, 0x64, 0x65,
    0x64, 0x20, 0x74, 0x6f, 0x0a, 0x20, 0x74, 0x68, 0x65, 0x20, 0x67, 0x72, 0x6f, 0x75, 0x70, 0x2e,
    0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02, 0x04, 0x01, 0x12, 0x03, 0x26, 0x02, 0x26, 0x0a,
    0x0c, 0x0a, 0x05, 0x05, 0x00, 0x02, 0x04, 0x02, 0x12, 0x03, 0x26, 0x29, 0x2a, 0x0a, 0x1f, 0x0a,
    0x02, 0x05, 0x01, 0x12, 0x04, 0x2a, 0x00, 0x2f, 0x01, 0x1a, 0x13, 0x20, 0x43, 0x6f, 0x6e, 0x76,
    0x65, 0x72, 0x73, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x20, 0x74, 0x79, 0x70, 0x65, 0x0a, 0x0a, 0x0a,
    0x0a, 0x03, 0x05, 0x01, 0x01, 0x12, 0x03, 0x2a, 0x05, 0x19, 0x0a, 0x0b, 0x0a, 0x04, 0x05, 0x01,
    0x02, 0x00, 0x12, 0x03, 0x2b, 0x02, 0x29, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02, 0x00, 0x01,
    0x12, 0x03, 0x2b, 0x02, 0x24, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02, 0x00, 0x02, 0x12, 0x03,
    0x2b, 0x27, 0x28, 0x0a, 0x0b, 0x0a, 0x04, 0x05, 0x01, 0x02, 0x01, 0x12, 0x03, 0x2c, 0x02, 0x23,
    0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02, 0x01, 0x01, 0x12, 0x03, 0x2c, 0x02, 0x1e, 0x0a, 0x0c,
    0x0a, 0x05, 0x05, 0x01, 0x02, 0x01, 0x02, 0x12, 0x03, 0x2c, 0x21, 0x22, 0x0a, 0x0b, 0x0a, 0x04,
    0x05, 0x01, 0x02, 0x02, 0x12, 0x03, 0x2d, 0x02, 0x20, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02,
    0x02, 0x01, 0x12, 0x03, 0x2d, 0x02, 0x1b, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02, 0x02, 0x02,
    0x12, 0x03, 0x2d, 0x1e, 0x1f, 0x0a, 0x0b, 0x0a, 0x04, 0x05, 0x01, 0x02, 0x03, 0x12, 0x03, 0x2e,
    0x02, 0x22, 0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02, 0x03, 0x01, 0x12, 0x03, 0x2e, 0x02, 0x1d,
    0x0a, 0x0c, 0x0a, 0x05, 0x05, 0x01, 0x02, 0x03, 0x02, 0x12, 0x03, 0x2e, 0x20, 0x21, 0x0a, 0x29,
    0x0a, 0x02, 0x04, 0x01, 0x12, 0x04, 0x32, 0x00, 0x36, 0x01, 0x1a, 0x1d, 0x20, 0x41, 0x20, 0x47,
    0x72, 0x6f, 0x75, 0x70, 0x73, 0x27, 0x73, 0x20, 0x6d, 0x75, 0x74, 0x61, 0x62, 0x6c, 0x65, 0x20,
    0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x0a, 0x0a, 0x0a, 0x0a, 0x03, 0x04, 0x01, 0x01,
    0x12, 0x03, 0x32, 0x08, 0x1b, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x01, 0x02, 0x00, 0x12, 0x03, 0x33,
    0x02, 0x25, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02, 0x00, 0x06, 0x12, 0x03, 0x33, 0x02, 0x15,
    0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02, 0x00, 0x01, 0x12, 0x03, 0x33, 0x16, 0x20, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x01, 0x02, 0x00, 0x03, 0x12, 0x03, 0x33, 0x23, 0x24, 0x0a, 0x0b, 0x0a, 0x04,
    0x04, 0x01, 0x02, 0x01, 0x12, 0x03, 0x34, 0x02, 0x21, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02,
    0x01, 0x04, 0x12, 0x03, 0x34, 0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02, 0x01, 0x05,
    0x12, 0x03, 0x34, 0x0b, 0x11, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02, 0x01, 0x01, 0x12, 0x03,
    0x34, 0x12, 0x1c, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02, 0x01, 0x03, 0x12, 0x03, 0x34, 0x1f,
    0x20, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x01, 0x02, 0x02, 0x12, 0x03, 0x35, 0x02, 0x27, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x01, 0x02, 0x02, 0x04, 0x12, 0x03, 0x35, 0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05,
    0x04, 0x01, 0x02, 0x02, 0x05, 0x12, 0x03, 0x35, 0x0b, 0x11, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01,
    0x02, 0x02, 0x01, 0x12, 0x03, 0x35, 0x12, 0x22, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x01, 0x02, 0x02,
    0x03, 0x12, 0x03, 0x35, 0x25, 0x26, 0x0a, 0x2a, 0x0a, 0x02, 0x04, 0x02, 0x12, 0x04, 0x39, 0x00,
    0x3f, 0x01, 0x1a, 0x1e, 0x20, 0x41, 0x20, 0x47, 0x72, 0x6f, 0x75, 0x70, 0x27, 0x73, 0x20, 0x69,
    0x6d, 0x6d, 0x75, 0x74, 0x61, 0x62, 0x6c, 0x65, 0x20, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74,
    0x61, 0x0a, 0x0a, 0x0a, 0x0a, 0x03, 0x04, 0x02, 0x01, 0x12, 0x03, 0x39, 0x08, 0x1d, 0x0a, 0x0b,
    0x0a, 0x04, 0x04, 0x02, 0x02, 0x00, 0x12, 0x03, 0x3a, 0x02, 0x1e, 0x0a, 0x0c, 0x0a, 0x05, 0x04,
    0x02, 0x02, 0x00, 0x05, 0x12, 0x03, 0x3a, 0x02, 0x08, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x02, 0x02,
    0x00, 0x01, 0x12, 0x03, 0x3a, 0x09, 0x19, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x02, 0x02, 0x00, 0x03,
    0x12, 0x03, 0x3a, 0x1c, 0x1d, 0x62, 0x06, 0x70, 0x72, 0x6f, 0x74, 0x6f, 0x33,
];
include!("xmtp.device_sync.group_backup.serde.rs");
// @@protoc_insertion_point(module)