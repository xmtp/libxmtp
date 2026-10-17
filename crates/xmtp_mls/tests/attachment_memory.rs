//! Measure the live allocation added by each 64 MiB path transfer phase.
//! Native only: it streams from and to the file system and uses a counting global allocator.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    future::Future,
    io::Write as _,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use xmtp_attachments::AttachmentOptions;
use xmtp_mls::{attachments::AttachmentSource, builder::ClientBuilder, tester};

struct CountingAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(live, Ordering::SeqCst);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, new_size) };
        if !next.is_null() {
            if new_size >= layout.size() {
                let growth = new_size - layout.size();
                let live = LIVE.fetch_add(growth, Ordering::SeqCst) + growth;
                PEAK.fetch_max(live, Ordering::SeqCst);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::SeqCst);
            }
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

async fn peak_growth<T>(future: impl Future<Output = T>) -> (T, usize) {
    let start = LIVE.load(Ordering::SeqCst);
    PEAK.store(start, Ordering::SeqCst);
    let result = future.await;
    let growth = PEAK.load(Ordering::SeqCst).saturating_sub(start);
    (result, growth)
}

// verifies: ATCH-039
#[xmtp_common::test(unwrap_try = true)]
async fn attachment_memory() {
    const SOURCE_BYTES: u64 = 64 * 1024 * 1024;
    const CHUNK_BYTES: usize = 64 * 1024;
    const MAX_OVER_CONTROL: usize = 2 * 1024 * 1024;

    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    let source = sender.path().join("large.bin");
    let mut file = std::fs::File::create(&source)?;
    let chunk = [0x5au8; CHUNK_BYTES];
    for _ in 0..(SOURCE_BYTES / CHUNK_BYTES as u64) {
        file.write_all(&chunk)?;
    }
    file.sync_all()?;
    drop(file);

    tester!(alix, attachments_dir: sender.path().join("attachments"), disable_workers);
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let recipient_client = ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;

    let (_, control) = peak_growth(async {
        xmtp_common::time::sleep(Duration::from_millis(100)).await;
    })
    .await;

    let (pending, create_growth) =
        peak_growth(alix.client.attachments().create(AttachmentSource::Path {
            path: source,
            filename: None,
            mime_type: "application/octet-stream".into(),
        }))
        .await;
    let pending = pending?;
    assert!(
        create_growth <= control + MAX_OVER_CONTROL,
        "create growth {create_growth} exceeds control {control} by more than {MAX_OVER_CONTROL}"
    );

    let remote = pending.remote_attachment().clone();
    let (uploaded, upload_growth) = peak_growth(pending.upload()).await;
    uploaded?;
    assert!(
        upload_growth <= control + MAX_OVER_CONTROL,
        "upload growth {upload_growth} exceeds control {control} by more than {MAX_OVER_CONTROL}"
    );

    let (downloaded, download_growth) =
        peak_growth(recipient_client.attachments().download(&remote)).await;
    let downloaded = downloaded?;
    assert_eq!(std::fs::metadata(downloaded.path)?.len(), SOURCE_BYTES);
    assert!(
        download_growth <= control + MAX_OVER_CONTROL,
        "download growth {download_growth} exceeds control {control} by more than {MAX_OVER_CONTROL}"
    );
}
