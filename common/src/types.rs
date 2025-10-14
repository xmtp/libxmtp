//! Common Primitive Types that may be shared across all XMTP Crates
//! Types should not have any dependencies other than std and std-adjacent crates (like bytes)

pub type Address = String;
pub type InboxId = String;
pub type WalletAddress = String;

#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}
