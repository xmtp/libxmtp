//! The GroupId parser rejects oversized text without an input-sized allocation.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use xmtp_proto::types::{GroupId, GroupIdParseError};

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

#[xmtp_common::test(unwrap_try = true)]
fn oversized_hex_group_id_is_rejected_before_decoding() {
    const INPUT_BYTES: usize = 8 * 1024 * 1024;
    const MAX_EXTRA_BYTES: usize = 1024;

    let input = "ab".repeat(INPUT_BYTES / 2);
    let baseline = LIVE.load(Ordering::SeqCst);
    PEAK.store(baseline, Ordering::SeqCst);

    let err = input.parse::<GroupId>().unwrap_err();
    let growth = PEAK.load(Ordering::SeqCst).saturating_sub(baseline);

    assert!(matches!(err, GroupIdParseError::Length(_)));
    assert!(
        growth < MAX_EXTRA_BYTES,
        "rejecting oversized input allocated {growth} extra bytes"
    );
}
