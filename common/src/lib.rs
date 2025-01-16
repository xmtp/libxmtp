//! Common types shared among all XMTP Crates

mod macros;

#[cfg(feature = "test-utils")]
mod test;
#[cfg(feature = "test-utils")]
pub use test::*;

#[cfg(feature = "bench")]
pub mod bench;

pub mod retry;
pub use retry::*;

/// Global Marker trait for WebAssembly
#[cfg(target_arch = "wasm32")]
pub trait Wasm {}
#[cfg(target_arch = "wasm32")]
impl<T> Wasm for T {}

pub mod time;

use rand::{
    distributions::{Alphanumeric, DistString},
    RngCore,
};
use xmtp_cryptography::utils as crypto_utils;
use std::{pin::Pin, task::Poll, future::Future};
use futures::{Stream, FutureExt, StreamExt};

#[cfg(not(target_arch = "wasm32"))]
pub struct StreamWrapper<'a, I> {
    inner: Pin<Box<dyn Stream<Item = I> + Send + 'a>>,
}

#[cfg(target_arch = "wasm32")]
pub struct StreamWrapper<'a, I> {
    inner: Pin<Box<dyn Stream<Item = I> + 'a>>,
}

impl<'a, I> Stream for StreamWrapper<'a, I> {
    type Item = I;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
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

impl<'a, O> Future for FutureWrapper<'a, O> {
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

pub fn rand_string<const N: usize>() -> String {
    Alphanumeric.sample_string(&mut crypto_utils::rng(), N)
}

pub fn rand_array<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    crypto_utils::rng().fill_bytes(&mut buffer);
    buffer
}

/// Yield back control to the async runtime
#[cfg(not(target_arch = "wasm32"))]
pub async fn yield_() {
    tokio::task::yield_now().await
}

/// Yield back control to the async runtime
#[cfg(target_arch = "wasm32")]
pub async fn yield_() {
    time::sleep(crate::time::Duration::from_millis(1)).await;
}
