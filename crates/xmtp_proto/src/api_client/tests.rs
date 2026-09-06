pub trait XmtpTestClient {
    type Builder;
    fn create() -> Self::Builder;
}

xmtp_common::if_native! {
    #[cfg(any(test, feature = "test-utils-network"))]
    mod native;
    #[cfg(any(test, feature = "test-utils-network"))]
    pub use native::*;
}
