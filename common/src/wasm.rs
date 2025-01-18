use std::{pin::Pin, task::Poll, future::Future, sync::atomic::{Ordering, AtomicU32, AtomicBool}};
use futures::{Stream, FutureExt, StreamExt};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
static LOCAL_EXECUTOR_TICKS: AtomicU32 = AtomicU32::new(0u32);
const YIELD_AFTER: u32 = 10_000;
static NEEDS_YIELD: AtomicBool = AtomicBool::new(true);

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

#[cfg(target_arch = "wasm32")]
mod inner {
    use super::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen (extends = js_sys::Object, js_name = Scheduler, typescript_type = "Scheduler")]
        pub type Scheduler;

        #[wasm_bindgen(method, structural, js_class = "Scheduler", js_name = yield)]
        pub fn r#yield(this: &Scheduler) -> js_sys::Promise;
    }
}
#[cfg(target_arch = "wasm32")]
use inner::*;

#[cfg(target_arch = "wasm32")]
pub fn ensure_browser_fairness() {
    use js_sys::wasm_bindgen::JsCast;
    use js_sys::wasm_bindgen::UnwrapThrowExt;
    // use web_sys::WorkerGlobalScope;
    use wasm_bindgen_futures::JsFuture;

    wasm_bindgen_futures::spawn_local(async move {
        NEEDS_YIELD.store(false, Ordering::SeqCst);
        // tracing::info!("yielding");
        // let scheduler = js_sys::Reflect::get(&js_sys::global(), &"scheduler".into()).unwrap();
        // let scheduler = scheduler.dyn_into::<Scheduler>().unwrap();
        // .expect("xmtp_mls should always be in worker");
        // let scheduler = worker.scheduler();
        crate::time::sleep(crate::time::Duration::from_millis(100)).await;
        // yield_().await;
        // JsFuture::from(scheduler.r#yield()).await.unwrap_throw();

        NEEDS_YIELD.store(true, Ordering::SeqCst);
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_browser_fairness() {
}

#[derive(Default)]
pub struct Fairness;

impl Fairness {
    pub fn wake() {
        tracing::info!("NEEDS YIELD {}", NEEDS_YIELD.load(Ordering::SeqCst));
        let ticks = LOCAL_EXECUTOR_TICKS.load(Ordering::SeqCst);
        if ticks % YIELD_AFTER == 0  && NEEDS_YIELD.load(Ordering::SeqCst) {
            ensure_browser_fairness();
        }
        // wraps on overflow
        LOCAL_EXECUTOR_TICKS.fetch_add(1, Ordering::SeqCst);
    }
}
