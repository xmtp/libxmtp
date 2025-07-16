pub mod task {
    #[cfg(not(target_arch = "wasm32"))]
    pub use tokio::task::*;

    #[cfg(target_arch = "wasm32")]
    pub use tokio_with_wasm::task::*;
}
