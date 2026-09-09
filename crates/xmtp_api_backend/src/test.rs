mod mock_client;
pub use mock_client::*;

mod definitions;
pub use definitions::*;

xmtp_common::if_native! {
    #[cfg(test)]
    pub(crate) mod bidi;
}

xmtp_common::if_native! {
    #[cfg(test)]
    #[ctor::ctor(unsafe)]
    fn _setup() {
        xmtp_common::logger();
    }
}
