use std::future::Future;

/// Run a foreign future in a task that outlives the caller if it is cancelled.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn call<T, F>(future: F) -> Result<T, &'static str>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || runtime.block_on(future))
        .await
        .map_err(|_| "foreign call task failed")
}

/// On wasm, a local task keeps polling after the caller stops waiting.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn call<T, F>(future: F) -> Result<T, &'static str>
where
    T: 'static,
    F: Future<Output = T> + 'static,
{
    let (sender, receiver) = futures::channel::oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = sender.send(future.await);
    });
    receiver.await.map_err(|_| "foreign call task failed")
}
