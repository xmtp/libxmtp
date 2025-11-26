pub mod tokio {
    pub mod task {
        crate::if_native! {
            pub use tokio::task::*;
        }
        crate::if_wasm! {
            pub use tokio_with_wasm::task::*;
        }
    }
}

pub use tokio::*;

crate::if_wasm! {
    /// Marker trait to determine whether a type implements `Send` or not.
    pub trait MaybeSend {}
    impl<T: ?Sized> MaybeSend for T {}

    /// Marker trait to determine whether a type implements `Send` or not.
    pub trait MaybeSync {}
    impl<T: ?Sized> MaybeSync for T {}

    /// Global Marker trait for WebAssembly
    pub trait Wasm {}
    impl<T> Wasm for T {}

    pub type BoxDynError = Box<dyn std::error::Error>;

    pub use futures::future::LocalBoxFuture as BoxDynFuture;

    pub use futures::stream::LocalBoxStream as BoxDynStream;

}

crate::if_native! {
    /// Marker trait to determine whether a type implements `Send` or not.
    pub trait MaybeSend: Send {}
    impl<T: Send + ?Sized> MaybeSend for T {}

    /// Marker trait to determine whether a type implements `Sync` or not.
    pub trait MaybeSync: Sync {}
    impl<T: Sync + ?Sized> MaybeSync for T {}

    pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

    pub use futures::future::BoxFuture as BoxDynFuture;

    pub use futures::stream::BoxStream as BoxDynStream;

}

pub trait MaybeSendFuture: Future + MaybeSend {}
impl<T: Future + MaybeSend> MaybeSendFuture for T {}
