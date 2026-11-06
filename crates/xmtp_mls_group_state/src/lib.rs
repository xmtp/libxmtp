//! MLS group state the client and the backend both read: the app data
//! dictionary and its components, group metadata, inbox IDs and versions.

pub mod app_data;
pub mod group;
pub mod group_metadata;
pub mod group_mutable_metadata;
pub mod inbox_id;
pub mod libxmtp_version;
