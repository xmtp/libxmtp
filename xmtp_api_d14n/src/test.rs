mod mock_client;
pub use mock_client::*;

mod test_client;
#[allow(unused)]
pub use test_client::*;

mod definitions;
pub use definitions::*;

xmtp_common::if_wasm! {
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
}

xmtp_common::if_native! {
    #[cfg(test)]
    #[ctor::ctor]
    fn _setup() {
        xmtp_common::logger();
    }
}
