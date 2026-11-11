//! Self-test of the heap-canary allocator. If the harness stops scanning,
//! every zeroization proof that uses it passes vacuously; these tests fail
//! instead.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(unix)]

use std::{hint::black_box, mem::MaybeUninit};

use xmtp_cryptography::secret::{
    Zeroizing,
    heap_canary::{Canary, scan, scan_async},
};

xmtp_cryptography::install_heap_canary!();

const KEY: [u8; 32] = [
    0x5e, 0x91, 0x0c, 0xd7, 0x22, 0x8b, 0xf4, 0x3a, 0x67, 0x1d, 0xa9, 0x40, 0xe3, 0x7c, 0x15, 0xb8,
    0x2f, 0xc6, 0x83, 0x58, 0x0e, 0xd1, 0x9a, 0x74, 0x36, 0xbf, 0x4c, 0xe2, 0x11, 0x87, 0x6d, 0xa5,
];

fn key_vec() -> Vec<u8> {
    black_box(KEY.to_vec())
}

/// Bytes of stack below the caller that [`leave_key_in_dead_stack`] fills:
/// more than the frames of a scan and [`copy_dead_stack`] take.
const DEAD_STACK: usize = 64 * 1024;
/// Bytes of unwritten stack that [`copy_dead_stack`] copies.
const COPIED_STACK: usize = 4 * 1024;

/// Fill the stack below the caller with the key, as earlier work on the
/// thread can leave a secret there.
#[inline(never)]
fn leave_key_in_dead_stack() {
    let mut stack = [0u8; DEAD_STACK];
    for chunk in stack.chunks_exact_mut(KEY.len()) {
        chunk.copy_from_slice(&KEY);
    }
    black_box(&mut stack);
}

/// Copy unwritten stack below the caller to the heap, as a move of a short
/// variant of a large enum copies the unwritten rest of the enum.
#[inline(never)]
fn copy_dead_stack() -> Box<MaybeUninit<[u8; COPIED_STACK]>> {
    let mut stack = MaybeUninit::uninit();
    black_box(&mut stack);
    Box::new(stack)
}

#[test]
fn dropped_vec_is_a_hit_and_zeroizing_is_not() {
    let plain = key_vec();
    let ((), hits) = scan(&[Canary::new(&KEY)], || drop(black_box(plain)));
    assert_eq!(hits, [1]);

    let wiped = Zeroizing::new(key_vec());
    let ((), hits) = scan(&[Canary::new(&KEY)], || drop(black_box(wiped)));
    assert_eq!(hits, [0]);
}

#[test]
fn dropped_hex_string_is_a_hit() {
    let hex = black_box(hex::encode(KEY));
    let ((), hits) = scan(&[Canary::new(&KEY)], || drop(black_box(hex)));
    assert_eq!(hits, [1]);
}

/// tracing records a `&[u8]` field as lowercase hex with a space between
/// bytes, `[5e 91 0c …]`. A freed copy of that text is a hit.
#[test]
fn dropped_spaced_hex_is_a_hit() {
    let spaced = black_box(format!(
        "[{}]",
        KEY.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    assert!(!spaced.contains(&hex::encode(KEY)));
    let ((), hits) = scan(&[Canary::new(&KEY)], || drop(black_box(spaced)));
    assert_eq!(hits, [1]);
}

/// The client database stores a key in an OpenMLS type as a CBOR array of
/// `u8`. A freed encoded value is a hit, though a CBOR array splits the raw
/// bytes. The key holds both bytes at the item boundary: `0x17` is its own
/// item, and `0x18` takes the one-byte prefix.
#[test]
fn dropped_cbor_encoding_is_a_hit() {
    let mut key = KEY;
    key[0] = 0x17;
    key[1] = 0x18;
    let encoded = black_box(cbor4ii::serde::to_vec(Vec::new(), &key.to_vec()).unwrap());
    assert!(!encoded.windows(key.len()).any(|window| window == key));
    let ((), hits) = scan(&[Canary::new(&key)], || drop(black_box(encoded)));
    assert_eq!(hits, [1]);
}

#[test]
fn finds_42_and_16_byte_secrets() {
    let long: [u8; 42] = std::array::from_fn(|i| 0x80 ^ (i as u8).wrapping_mul(37));
    let short: [u8; 16] = std::array::from_fn(|i| 0x11 ^ (i as u8).wrapping_mul(53));
    let long_vec = black_box(long.to_vec());
    let short_vec = black_box(short.to_vec());
    let ((), hits) = scan(&[Canary::new(&long), Canary::new(&short)], || {
        drop(black_box(long_vec));
        drop(black_box(short_vec));
    });
    assert_eq!(hits, [1, 1]);
}

#[test]
fn reversed_copy_is_a_hit_only_for_secp256k1() {
    let mut reversed = key_vec();
    reversed.reverse();
    let ((), hits) = scan(&[Canary::new(&KEY), Canary::secp256k1(&KEY)], || {
        drop(black_box(reversed))
    });
    assert_eq!(hits, [0, 1]);
}

#[test]
fn realloc_old_block_is_a_hit() {
    let mut grows = key_vec();
    assert_eq!(grows.capacity(), KEY.len());
    let ((), hits) = scan(&[Canary::new(&KEY)], || {
        // Growing frees the old block, which still holds the key.
        grows.reserve(4 * KEY.len());
        // The new block is wiped before it is freed.
        drop(Zeroizing::new(black_box(grows)));
    });
    assert_eq!(hits, [1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_async_counts_frees_on_other_workers() {
    let plain = key_vec();
    let test_thread = std::thread::current().id();
    let (worker_thread, hits) = scan_async(&[Canary::new(&KEY)], async move {
        tokio::spawn(async move {
            drop(black_box(plain));
            std::thread::current().id()
        })
        .await
        .unwrap()
    })
    .await;
    assert_ne!(worker_thread, test_thread);
    assert_eq!(hits, [1]);
}

/// A scan does not wipe the stack it starts with. Code under test that
/// copies dead stack to the heap, as a move of a short enum variant does,
/// frees a copy of a secret that earlier work left there, and the scan
/// counts it.
#[test]
fn freed_copy_of_dead_stack_is_a_hit() {
    let canary = Canary::new(&KEY);
    leave_key_in_dead_stack();
    let ((), hits) = scan(&[canary], || drop(black_box(copy_dead_stack())));
    assert_eq!(hits, [1]);
}
