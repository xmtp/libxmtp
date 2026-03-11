pub const DEFAULT_GROUP_NAME: &str = "";
pub const DEFAULT_GROUP_DESCRIPTION: &str = "";
pub const DEFAULT_GROUP_IMAGE_URL_SQUARE: &str = "";

/// MLS Extension Types
///
/// Copied from draft-ietf-mls-protocol-16:
///
/// | Value            | Name                     | Message(s) | Recommended | Reference |
/// |:-----------------|:-------------------------|:-----------|:------------|:----------|
/// | 0x0000           | RESERVED                 | N/A        | N/A         | RFC XXXX  |
/// | 0x0001           | application_id           | LN         | Y           | RFC XXXX  |
/// | 0x0002           | ratchet_tree             | GI         | Y           | RFC XXXX  |
/// | 0x0003           | required_capabilities    | GC         | Y           | RFC XXXX  |
/// | 0x0004           | external_pub             | GI         | Y           | RFC XXXX  |
/// | 0x0005           | external_senders         | GC         | Y           | RFC XXXX  |
/// | 0xff00  - 0xffff | Reserved for Private Use | N/A        | N/A         | RFC XXXX  |
pub const MUTABLE_METADATA_EXTENSION_ID: u16 = 0xff00;
pub const GROUP_MEMBERSHIP_EXTENSION_ID: u16 = 0xff01;
pub const GROUP_PERMISSIONS_EXTENSION_ID: u16 = 0xff02;
pub const WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID: u16 = 0xff03;
pub const WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID: u16 = 0xff04;
/// Extension ID for proposal support.
/// - On leaf nodes: indicates the installation supports proposal-by-reference flow
/// - On group context: indicates the group uses proposal-by-reference flow exclusively
pub const PROPOSAL_SUPPORT_EXTENSION_ID: u16 = 0xff05;
