//! Consistent Stream behavior between WebAssembly and Native utilizing `tokio::task::spawn` in native and
//! `wasm_bindgen_futures::spawn` for web.
use futures::{Future, FutureExt};

#[derive(thiserror::Error, Debug)]
pub enum StreamHandleError {
    #[error("Result Channel closed")]
    ChannelClosed,
    #[error("The stream was closed")]
    StreamClosed,
}

/// A handle to a spawned Stream
/// the spawned stream can be 'joined` by awaiting its Future implementation.
/// All spawned tasks are detached, so waiting the handle is not required.
#[allow(async_fn_in_trait)]
pub trait StreamHandle: Future<Output = Result<<Self as StreamHandle>::Output, StreamHandleError>> {
    /// The Output type for the stream
    type Output;
    /// Asyncronously waits for the stream to be fully spawned
    async fn wait_for_ready(&mut self);
    /// Signal the stream to end
    /// Does not wait for the stream to end, so will not receive the result of stream.
    fn end(&self);
    /// End the stream and asyncronously wait for it to shutdown, getting the result of its
    /// execution.
    async fn end_and_wait(self) -> Result<<Self as StreamHandle>::Output, StreamHandleError>;
    /// Get an Abort Handle to the stream.
    /// This handle may be cloned/sent/etc easily
    /// and many handles may exist at once.
    fn abort_handle<'b>(&self) -> impl AbortHandle + 'b;
}

/// A handle that can be moved/cloned/sent, but can only close the stream.
pub trait AbortHandle: Send + Sync + Clone {
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
    use futures::future::Either;

    use super::*;

    pub struct WasmStreamHandle<T> {
        result: tokio::sync::oneshot::Receiver<T>,
        // we only send once but oneshot senders aren't cloneable
        // so we use mpsc here to keep the `&self` on `end`.
        closer: tokio::sync::mpsc::Sender<()>,
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    impl<T> Future for WasmStreamHandle<Result<T, StreamHandleError>> {
        type Output = Result<T, StreamHandleError>;

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            FutureExt::poll_unpin(&mut self.result, cx)
                .map(|r| {
                    match r {
                        Ok(r) => r,
                        Err(_) => Err(StreamHandleError::ChannelClosed)
                    }
                })
            }
    }

    impl<T> StreamHandle for WasmStreamHandle<Result<T, StreamHandleError>> {
        type Output = T;
        async fn wait_for_ready(&mut self) {
            if let Some(s) = self.ready.take() {
                let _ = s.await;
            }
        }

        fn end(&self) {
            let _ = self.closer.try_send(());
        }

        async fn end_and_wait(self) -> Result<<Self as StreamHandle>::Output, StreamHandleError> {
            self.end();
            self.await
        }

        fn abort_handle<'b>(&self) -> impl AbortHandle + 'b {
            CloseHandle(self.closer.clone())
        }
    }

    #[derive(Clone)]
    struct CloseHandle(tokio::sync::mpsc::Sender<()>);
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
    ///  optionally pass in `ready` to signal whne stream will be ready.
    // its pretty annoying but `unused_must_use` doesn't work here for some reason,
    // so we still get a bunch of warnings taht `unused implementor of Future must be used`
    // if we dont write the spawn in the form: `let _ = crate::spawn()`
    #[allow(unused_must_use)]
    pub fn spawn<F>(
        ready: Option<tokio::sync::oneshot::Receiver<()>>,
        future: F,
    ) -> impl StreamHandle<Output = F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();
        let (closer_tx, mut closer_rx) = tokio::sync::mpsc::channel::<()>(1);

        let handle = WasmStreamHandle {
            result: res_rx,
            closer: closer_tx,
            ready,
        };

        wasm_bindgen_futures::spawn_local(async move {
            let recv = closer_rx.recv();
            futures::pin_mut!(recv);
            futures::pin_mut!(future);
            let value = match futures::future::select(recv, future).await {
                Either::Left((_, _)) => Err(StreamHandleError::StreamClosed),
                Either::Right((v, _)) => Ok(v)
            };
            let _ = res_tx.send(value);
        });

        handle
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use tokio::task::JoinHandle;

    pub struct TokioStreamHandle<T> {
        inner: JoinHandle<T>,
    }

    pub fn spawn<F>(future: F) -> impl StreamHandle<Output = F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::task::spawn(future)
    }
}
