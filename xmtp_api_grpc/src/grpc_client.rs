pub mod client;
pub use client::*;

xmtp_common::if_wasm! {
    mod wasm;
    pub use wasm::*;
    pub type GrpcService = wasm::GrpcWebService;
    // it's better to take the hit and unsafe impl send on the client
    // rather then infect a Send bound into all the rest of the code
    // that depends on a client.
    // When web supports threads (and therefore JsValue must become Send & Sync),
    // we can delete this.
    unsafe impl Send for GrpcWebService { }
    unsafe impl Sync for GrpcWebService { }
}

xmtp_common::if_native! {
    mod native;
    pub use native::*;
    pub type GrpcService = native::NativeGrpcService;
}
