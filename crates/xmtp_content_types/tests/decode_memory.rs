//! Codecs reject content with too many elements before building them.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use prost::Message;
use xmtp_content_types::{
    CodecError, ContentCodec, intent::IntentCodec,
    multi_remote_attachment::MultiRemoteAttachmentCodec, reply::ReplyCodec, text::TextCodec,
    transaction_reference::TransactionReferenceCodec, wallet_send_calls::WalletSendCallsCodec,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

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

type Decode = fn(EncodedContent) -> Result<(), CodecError>;

// verifies: CTYPE-033
#[xmtp_common::test(unwrap_try = true)]
fn many_element_content_fails_without_building_elements() {
    const ELEMENTS: usize = 1 << 20;
    const MAX_EXTRA_BYTES: usize = 1 << 20;

    let zeros = "0,".repeat(ELEMENTS - 1);
    let nested = EncodedContent {
        r#type: Some(TextCodec::content_type()),
        parameters: (0..300_000)
            .map(|i| (i.to_string(), String::new()))
            .collect(),
        content: b"text".to_vec(),
        ..Default::default()
    };
    let cases: [(&str, Vec<u8>, Decode); 5] = [
        (
            "MultiRemoteAttachment",
            [0x0a, 0x00].repeat(ELEMENTS),
            |content| MultiRemoteAttachmentCodec::decode(content).map(drop),
        ),
        (
            "WalletSendCalls",
            format!(
                r#"{{"version":"1","chainId":"1","from":"0x","calls":[{}{{}}]}}"#,
                "{},".repeat(ELEMENTS - 1)
            )
            .into_bytes(),
            |content| WalletSendCallsCodec::decode(content).map(drop),
        ),
        (
            "Intent",
            format!(r#"{{"id":"a","actionId":"b","metadata":{{"k":[{zeros}0]}}}}"#).into_bytes(),
            |content| IntentCodec::decode(content).map(drop),
        ),
        (
            "TransactionReference",
            format!(r#"{{"networkId":[{zeros}0],"reference":"0x"}}"#).into_bytes(),
            |content| TransactionReferenceCodec::decode(content).map(drop),
        ),
        ("Reply", nested.encode_to_vec(), |content| {
            ReplyCodec::decode(content).map(drop)
        }),
    ];
    for (case, content, decode) in cases {
        let content = EncodedContent {
            content,
            ..Default::default()
        };
        let baseline = LIVE.load(Ordering::SeqCst);
        PEAK.store(baseline, Ordering::SeqCst);
        let result = decode(content);
        let growth = PEAK.load(Ordering::SeqCst).saturating_sub(baseline);
        assert!(
            result.is_err() && growth < MAX_EXTRA_BYTES,
            "{case}: failed {}, allocated {growth} extra bytes",
            result.is_err()
        );
    }
}
