//! Common Primitive Types that may be shared across all XMTP Crates
//! Types should not have any dependencies other than std and std-adjacent crates (like bytes)

pub type Address = String;
pub type InboxId = String;
pub type WalletAddress = String;
