use futures::{
    Stream,
    future::{Future, FutureExt},
    stream::StreamExt,
};
use std::{pin::Pin, task::Poll};
mod tokio;

pub use tokio::*;

crate::if_wasm! {
    /// Marker trait to determine whether a type implements `Send` or not.
    pub trait MaybeSend {}
    impl<T> MaybeSend for T {}

    /// Global Marker trait for WebAssembly
    pub trait Wasm {}
    impl<T> Wasm for T {}

    pub struct StreamWrapper<'a, I> {
        inner: Pin<Box<dyn Stream<Item = I> + 'a>>,
    }
}

crate::if_native! {
    /// Marker trait to determine whether a type implements `Send` or not.
    pub trait MaybeSend: Send {}
    impl<T: Send> MaybeSend for T {}

    pub struct StreamWrapper<'a, I> {
        inner: Pin<Box<dyn Stream<Item = I> + Send + 'a>>,
    }
}

impl<I> Stream for StreamWrapper<'_, I> {
    type Item = I;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let inner = &mut self.inner;
        futures::pin_mut!(inner);
        inner.as_mut().poll_next(cx)
    }
}

impl<'a, I> StreamWrapper<'a, I> {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = I> + Send + 'a,
    {
        Self {
            inner: stream.boxed(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = I> + 'a,
    {
        Self {
            inner: stream.boxed_local(),
        }
    }
}

// Wrappers to deal with Send Bounds
#[cfg(not(target_arch = "wasm32"))]
pub struct FutureWrapper<'a, O> {
    inner: Pin<Box<dyn Future<Output = O> + Send + 'a>>,
}

#[cfg(target_arch = "wasm32")]
pub struct FutureWrapper<'a, O> {
    inner: Pin<Box<dyn Future<Output = O> + 'a>>,
}

impl<O> Future for FutureWrapper<'_, O> {
    type Output = O;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.inner;
        futures::pin_mut!(inner);
        inner.as_mut().poll(cx)
    }
}

impl<'a, O> FutureWrapper<'a, O> {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = O> + Send + 'a,
    {
        Self {
            inner: future.boxed(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = O> + 'a,
    {
        Self {
            inner: future.boxed_local(),
        }
    }
}
