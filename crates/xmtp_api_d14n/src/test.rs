mod mock_client;
pub use mock_client::*;

mod traits;
#[allow(unused)]
pub use traits::*;

mod definitions;
pub use definitions::*;

xmtp_common::if_native! {
    #[cfg(test)]
    #[ctor::ctor]
    fn _setup() {
        xmtp_common::logger();
    }
}
