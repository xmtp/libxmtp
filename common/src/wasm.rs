use futures::{FutureExt, Stream, StreamExt};
use std::{future::Future, pin::Pin, task::Poll};

/// Global Marker trait for WebAssembly
#[cfg(target_arch = "wasm32")]
pub trait Wasm {}
#[cfg(target_arch = "wasm32")]
impl<T> Wasm for T {}

#[cfg(not(target_arch = "wasm32"))]
pub struct StreamWrapper<'a, I> {
    inner: Pin<Box<dyn Stream<Item = I> + Send + 'a>>,
}

#[cfg(target_arch = "wasm32")]
pub struct StreamWrapper<'a, I> {
    inner: Pin<Box<dyn Stream<Item = I> + 'a>>,
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

/// Yield back control to the async runtime
#[cfg(not(target_arch = "wasm32"))]
pub async fn yield_() {
    tokio::task::yield_now().await
}

/// Yield back control to the async runtime
#[cfg(target_arch = "wasm32")]
pub async fn yield_() {
    crate::time::sleep(crate::time::Duration::from_millis(100)).await;
}
