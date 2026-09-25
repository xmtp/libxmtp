#![cfg(not(target_arch = "wasm32"))]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    convert::Infallible,
    sync::atomic::{AtomicUsize, Ordering},
};

use bytes::Bytes;
use futures_util::stream;
use http_body_util::StreamBody;
use hyper::{body::Frame, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use xmtp_attachments::{AttachmentOptions, DownloadSink, Transfer};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct CountAlloc;

unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, old, new) };
        if !result.is_null() {
            if new >= old.size() {
                let live = LIVE.fetch_add(new - old.size(), Ordering::Relaxed) + new - old.size();
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(old.size() - new, Ordering::Relaxed);
            }
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

#[derive(Default)]
struct CountSink(usize);

#[async_trait::async_trait]
impl DownloadSink for CountSink {
    async fn write(&mut self, bytes: &[u8]) -> Result<(), xmtp_attachments::AttachmentError> {
        self.0 += bytes.len();
        Ok(())
    }
}

// verifies: ATCH-039
#[xmtp_common::test(unwrap_try = true)]
async fn bounded_allocation() {
    const SIZE: usize = 64 * 1024 * 1024;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/object", listener.local_addr()?);
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let service = service_fn(|_| async {
            let chunks = stream::iter(
                (0..SIZE / (32 * 1024))
                    .map(|_| Ok::<_, Infallible>(Frame::data(Bytes::from(vec![42; 32 * 1024])))),
            );
            Ok::<_, Infallible>(hyper::Response::new(StreamBody::new(chunks)))
        });
        let _ = http1::Builder::new()
            .serve_connection(TokioIo::new(socket), service)
            .await;
    });
    let transfer = Transfer::new(AttachmentOptions {
        allow_private_network: true,
        ..Default::default()
    })?;
    let mut sink = CountSink::default();
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    transfer.get(&url, SIZE as u64, &mut sink).await?;
    assert_eq!(sink.0, SIZE);
    let attributable = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    task.abort();
    let _ = task.await;
    assert!(attributable <= 1_048_576, "peak allocation: {attributable}");
}
