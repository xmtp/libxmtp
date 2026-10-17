pub type SignatureWithNonce = alloy::sol! { tuple(uint256, bytes) };

xmtp_common::if_native! {
    #[cfg(any(test, feature = "test-utils-anvil"))]
    mod native;
    #[cfg(any(test, feature = "test-utils-anvil"))]
    pub use native::*;
}
