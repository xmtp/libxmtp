//! A compressed reaction stops decompressing at the reaction limit.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use prost::Message;
use xmtp_content_types::{ContentCodec, compression, reaction::ReactionCodec};
use xmtp_mls_messages::decoded_message::{ContentDecodeFailureKind, MessageBody};
use xmtp_proto::xmtp::mls::message_contents::{
    Compression, EncodedContent, content_types::ReactionV2,
};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

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

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, old, new_size) };
        if !next.is_null() {
            if new_size >= old.size() {
                let live =
                    LIVE.fetch_add(new_size - old.size(), Ordering::SeqCst) + new_size - old.size();
                PEAK.fetch_max(live, Ordering::SeqCst);
            } else {
                LIVE.fetch_sub(old.size() - new_size, Ordering::SeqCst);
            }
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// verifies: CTYPE-034
#[xmtp_common::test(unwrap_try = true)]
fn compressed_reaction_stops_at_reaction_limit() {
    const MAX_EXTRA_BYTES: usize = 1 << 20;

    let reaction = ReactionV2 {
        reference: "0102".into(),
        content: "a".repeat(compression::MAX_DECOMPRESSED_BYTES - 4096),
        ..Default::default()
    };
    let envelope = EncodedContent {
        r#type: Some(ReactionCodec::content_type()),
        content: reaction.encode_to_vec(),
        ..Default::default()
    };
    drop(reaction);
    let raw = compression::compress(envelope, Compression::Deflate)?.encode_to_vec();

    let baseline = LIVE.load(Ordering::SeqCst);
    PEAK.store(baseline, Ordering::SeqCst);
    let body = MessageBody::from_received_bytes(&raw);
    let growth = PEAK.load(Ordering::SeqCst).saturating_sub(baseline);

    let MessageBody::Undecodable(undecodable) = body else {
        panic!("an oversized reaction decoded: {body:?}");
    };
    assert_eq!(
        undecodable.failure.kind,
        ContentDecodeFailureKind::CodecDecodeFailed
    );
    assert!(
        growth < MAX_EXTRA_BYTES,
        "decoding allocated {growth} extra bytes"
    );
}
