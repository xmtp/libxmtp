//! Consistent Stream behavior between WebAssembly and Native utilizing `tokio::task::spawn` in native and
//! `wasm_bindgen_futures::spawn` for web.

#[cfg(target_arch = "wasm32")]
pub type GenericStreamHandle<O> = dyn StreamHandle<StreamOutput = O>;

#[cfg(not(target_arch = "wasm32"))]
pub type GenericStreamHandle<O> = dyn StreamHandle<StreamOutput = O> + Send + Sync;

#[derive(thiserror::Error, Debug)]
pub enum StreamHandleError {
    #[error("Result Channel closed")]
    ChannelClosed,
    #[error("The stream was closed")]
    StreamClosed,
    #[error(transparent)]
    JoinHandleError(#[from] tokio::task::JoinError),
    #[error("Stream Cancelled")]
    Cancelled,
    #[error("Stream Panicked With {0}")]
    Panicked(String),
}
/// A handle to a spawned Stream
/// the spawned stream can be 'joined` by awaiting its Future implementation.
/// All spawned tasks are detached, so waiting the handle is not required.
#[allow(async_fn_in_trait)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait StreamHandle {
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
pub trait AbortHandle: Send + Sync {
    /// Send a signal to end the stream, without waiting for a result.
    fn end(&self);
    fn is_finished(&self) -> bool;
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
#[allow(unused)]
pub use wasm::*;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::future::Either;

    use super::*;

    pub struct WasmStreamHandle<T> {
        result: tokio::sync::oneshot::Receiver<T>,
        // we only send once but oneshot senders aren't cloneable
        // so we use mpsc here to keep the `&self` on `end`.
        closer: tokio::sync::mpsc::Sender<()>,
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    impl<T: std::fmt::Debug> Future for WasmStreamHandle<Result<T, StreamHandleError>> {
        type Output = Result<T, StreamHandleError>;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            // safe because we consider `result` to be structurally pinned
            // pinning: https://doc.rust-lang.org/std/pin/#choosing-pinning-to-be-structural-for-field
            let result = unsafe { self.map_unchecked_mut(|r| &mut r.result) };
            result.poll(cx).map(|r| match r {
                Ok(r) => r,
                Err(_) => Err(StreamHandleError::ChannelClosed),
            })
        }
    }

    #[async_trait::async_trait(?Send)]
    impl<T: std::fmt::Debug> StreamHandle for WasmStreamHandle<Result<T, StreamHandleError>> {
        type StreamOutput = T;

        async fn wait_for_ready(&mut self) {
            if let Some(s) = self.ready.take() {
                let _ = s.await;
            }
        }

        async fn end_and_wait(&mut self) -> Result<Self::StreamOutput, StreamHandleError> {
            self.end();
            self.await
        }

        fn end(&self) {
            let _ = self.closer.try_send(());
        }

        fn abort_handle(&self) -> Box<dyn AbortHandle> {
            Box::new(CloseHandle(self.closer.clone()))
        }

        async fn join(self) -> Result<Self::StreamOutput, StreamHandleError> {
            self.await
        }
    }

    #[derive(Clone)]
    pub struct CloseHandle(tokio::sync::mpsc::Sender<()>);
    impl AbortHandle for CloseHandle {
        fn end(&self) {
            let _ = self.0.try_send(());
        }

        fn is_finished(&self) -> bool {
            self.0.is_closed()
        }
    }

    /// Spawn a future on the `wasm-bindgen` local current-thread executer
    ///  future does not require `Send`.
    ///  optionally pass in `ready` to signal when stream will be ready.
    pub fn spawn<F>(
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
        future: F,
    ) -> impl StreamHandle<StreamOutput = F::Output>
    where
        F: Future + 'static,
        F::Output: 'static + std::fmt::Debug,
    {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();
        let (closer_tx, closer_rx) = tokio::sync::mpsc::channel::<()>(1);
        let closer_handle = CloserHandle::new(closer_rx);

        let handle = WasmStreamHandle {
            result: res_rx,
            closer: closer_tx,
            ready,
        };
        tracing::info!("Spawning local task on web executor");
        wasm_bindgen_futures::spawn_local(async move {
            futures::pin_mut!(closer_handle);
            futures::pin_mut!(future);
            let value = match futures::future::select(closer_handle, future).await {
                Either::Left((_, _)) => {
                    tracing::warn!("stream closed");
                    Err(StreamHandleError::StreamClosed)
                }
                Either::Right((v, _)) => {
                    tracing::debug!("Future ended with value");
                    Ok(v)
                }
            };
            let _ = res_tx.send(value);
            tracing::info!("spawned local future closing");
        });

        handle
    }

    struct CloserHandle(tokio::sync::mpsc::Receiver<()>);

    impl CloserHandle {
        fn new(receiver: tokio::sync::mpsc::Receiver<()>) -> Self {
            Self(receiver)
        }
    }

    impl Future for CloserHandle {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            match self.0.poll_recv(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Some(_)) => Poll::Ready(()),
                // if the channel is closed, the task has detached and must
                // be kept alive for the duration for the program
                Poll::Ready(None) => Poll::Pending,
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

    #[async_trait::async_trait]
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
}
