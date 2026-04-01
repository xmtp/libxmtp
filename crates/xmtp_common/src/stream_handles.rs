//! Consistent Stream behavior between WebAssembly and Native utilizing `tokio::task::spawn` in native and
//! `tokio_with_wasm::task::spawn` for web.

use crate::{MaybeSend, MaybeSync, if_native, if_wasm};

pub type GenericStreamHandle<O> = dyn StreamHandle<StreamOutput = O>;

#[derive(thiserror::Error, Debug)]
pub enum StreamHandleError {
    #[error("Stream Cancelled")]
    Cancelled,
    #[error("Stream Panicked With {0}")]
    Panicked(String),
    #[cfg(not(target_arch = "wasm32"))]
    #[error(transparent)]
    JoinHandleError(#[from] tokio::task::JoinError),
    #[cfg(target_arch = "wasm32")]
    #[error(transparent)]
    JoinHandleError(#[from] crate::wasm::tokio::task::JoinError),
}
/// A handle to a spawned Stream
/// the spawned stream can be 'joined` by awaiting its Future implementation.
/// All spawned tasks are detached, so waiting the handle is not required.
#[xmtp_macro::async_trait]
pub trait StreamHandle: MaybeSend + MaybeSync {
    /// The Output type for the stream
    type StreamOutput;

    /// Asynchronously waits for the stream to be fully spawned
    async fn wait_for_ready(&mut self);
    /// Signal the stream to end
    /// Does not wait for the stream to end, so will not receive the result of stream.
    fn end(&self);

    // Its better to:
    // `StreamHandle: Future<Output = Result<Self::StreamOutput,StreamHandleError>>`
    // but then crate::spawn` generates `Unused future must be used` since
    // `async fn` desugars to `fn() -> impl Future`. There's no way
    // to get rid of that warning, so we separate the future impl to here.
    // See this rust-playground for an example:
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=a2a88b144c9459176e8fae41ee569553
    /// Join the task back to the current thread, waiting until it ends.
    async fn join(self) -> Result<Self::StreamOutput, StreamHandleError>;

    /// End the stream and asynchronously wait for it to shutdown, getting the result of its
    /// execution.
    async fn end_and_wait(&mut self) -> Result<Self::StreamOutput, StreamHandleError>;
    /// Get an Abort Handle to the stream.
    /// This handle may be cloned/sent/etc easily
    /// and many handles may exist at once.
    fn abort_handle(&self) -> Box<dyn AbortHandle>;
}

/// A handle that can be moved/cloned/sent, but can only close the stream.
pub trait AbortHandle: crate::MaybeSend + crate::MaybeSync {
    /// Send a signal to end the stream, without waiting for a result.
    fn end(&self);
    fn is_finished(&self) -> bool;
}

if_wasm! {
pub use wasm::*;
mod wasm {
    use std::{
        future::Future,
        sync::{Arc, atomic::{AtomicBool, Ordering}},
    };

    use super::*;

    pub struct WasmStreamHandle<T> {
        inner: crate::wasm::tokio::task::JoinHandle<T>,
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
        finished: Arc<AtomicBool>,
    }

    impl<T> Future for WasmStreamHandle<T> {
        type Output = Result<T, StreamHandleError>;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            let inner = unsafe { self.map_unchecked_mut(|v| &mut v.inner) };
            inner.poll(cx).map_err(StreamHandleError::from)
        }
    }

    #[xmtp_common::async_trait]
    impl<T> StreamHandle for WasmStreamHandle<T>
    where T: 'static
    {
        type StreamOutput = T;

        async fn wait_for_ready(&mut self) {
            if let Some(s) = self.ready.take() {
                let _ = s.await;
            }
        }

        fn end(&self) {
            self.inner.abort();
        }

        async fn end_and_wait(&mut self) -> Result<Self::StreamOutput, StreamHandleError> {
            use StreamHandleError::*;
            self.end();
            match self.await {
                Err(JoinHandleError(e)) if e.is_cancelled() => Err(Cancelled),
                Ok(t) => Ok(t),
                Err(e) => Err(e),
            }
        }

        fn abort_handle(&self) -> Box<dyn AbortHandle> {
            Box::new(WasmAbortHandle {
                inner: self.inner.abort_handle(),
                finished: self.finished.clone(),
            })
        }

        async fn join(self) -> Result<Self::StreamOutput, StreamHandleError> {
            self.await
        }
    }

    #[derive(Clone)]
    pub struct WasmAbortHandle {
        inner: crate::wasm::tokio::task::AbortHandle,
        finished: Arc<AtomicBool>,
    }

    impl AbortHandle for WasmAbortHandle {
        fn end(&self) {
            self.inner.abort();
        }

        fn is_finished(&self) -> bool {
            self.finished.load(Ordering::Relaxed)
        }
    }

    pub fn spawn<F>(
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
        future: F,
    ) -> impl StreamHandle<StreamOutput = F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = finished.clone();

        let inner = crate::wasm::tokio::task::spawn(async move {
            let result = future.await;
            finished_clone.store(true, Ordering::Relaxed);
            result
        });

        WasmStreamHandle {
            inner,
            ready,
            finished,
        }
    }
}}

if_native! {
pub use native::*;
mod native {
    use super::*;
    use std::future::Future;
    use tokio::task::JoinHandle;

    pub struct TokioStreamHandle<T> {
        inner: JoinHandle<T>,
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    impl<T> Future for TokioStreamHandle<T> {
        type Output = Result<T, StreamHandleError>;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            // safe because we consider `inner` to be structurally pinned
            // https://doc.rust-lang.org/std/pin/#choosing-pinning-to-be-structural-for-field
            let inner = unsafe { self.map_unchecked_mut(|v| &mut v.inner) };
            inner.poll(cx).map_err(StreamHandleError::from)
        }
    }

    #[xmtp_common::async_trait]
    impl<T: Send> StreamHandle for TokioStreamHandle<T> {
        type StreamOutput = T;

        async fn wait_for_ready(&mut self) {
            if let Some(s) = self.ready.take() {
                let _ = s.await;
            }
        }

        fn end(&self) {
            self.inner.abort();
        }

        async fn end_and_wait(&mut self) -> Result<Self::StreamOutput, StreamHandleError> {
            use crate::StreamHandleError::*;

            self.end();
            match self.await {
                Err(JoinHandleError(e)) if e.is_panic() => Err(Panicked(e.to_string())),
                Err(JoinHandleError(e)) if e.is_cancelled() => Err(Cancelled),
                Ok(t) => Ok(t),
                Err(e) => Err(e),
            }
        }

        fn abort_handle(&self) -> Box<dyn AbortHandle> {
            Box::new(self.inner.abort_handle())
        }

        async fn join(self) -> Result<Self::StreamOutput, StreamHandleError> {
            self.await
        }
    }

    impl AbortHandle for tokio::task::AbortHandle {
        fn end(&self) {
            self.abort()
        }

        fn is_finished(&self) -> bool {
            self.is_finished()
        }
    }

    pub fn spawn<F>(
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
        future: F,
    ) -> impl StreamHandle<StreamOutput = F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        TokioStreamHandle {
            inner: tokio::task::spawn(future),
            ready,
        }
    }

    crate::if_test! {
        pub fn spawn_instrumented<F>(
            ready: Option<tokio::sync::oneshot::Receiver<()>>,
            future: F,
        ) -> impl StreamHandle<StreamOutput = F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            TokioStreamHandle {
                inner: tokio::task::spawn(future),
                ready,
            }
        }
    }
}}
