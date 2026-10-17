//! Common Primitive Types that may be shared across all XMTP Crates
//! Types should not have any dependencies other than std and std-adjacent crates (like bytes)

pub type Address = String;
pub type InboxId = String;
pub type WalletAddress = String;

/// The three values used to match a content codec.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
}
