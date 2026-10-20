//! Heap-canary proofs that wallet key import and export wipe every copy the
//! SDK makes. Native only: it installs the heap-canary global allocator.
//! No backend needed. The local signer does not exist under `pure-only`.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(all(unix, not(feature = "pure-only")))]

use std::{mem::ManuallyDrop, sync::Arc};

use alloy::signers::local::PrivateKeySigner;
use uniffi::{Lift, Lower, LowerReturn, RustBuffer};
use xmtp_cryptography::secret::heap_canary::{Canary, scan, scan_async};
use xmtp_id::InboxOwner;
use xmtp_sdk::{
    LocalSigningKey, SecretBytes, SecretString, Signer, SigningRequest, UniFfiTag,
    local_signer_from_private_key,
};

xmtp_cryptography::install_heap_canary!();

/// A valid secp256k1 scalar with no repeated pattern.
const KEY: [u8; 32] = [
    0x1c, 0x8e, 0x52, 0xb7, 0x09, 0xf4, 0x63, 0x2d, 0xa1, 0x7b, 0xe0, 0x35, 0xc9, 0x4a, 0x16, 0xd3,
    0x88, 0x2f, 0x71, 0xbc, 0x05, 0xe6, 0x9a, 0x43, 0xfd, 0x10, 0x67, 0xcb, 0x34, 0x98, 0x5e, 0xa2,
];

fn signing_request() -> SigningRequest {
    SigningRequest {
        text: "XMTP zeroize proof".to_owned(),
    }
}

async fn sign_and_drop(signer: Arc<dyn Signer>) {
    signer.sign(signing_request()).await.unwrap();
}

/// Every import path wipes its argument and intermediates, on success.
#[xmtp_common::test(unwrap_try = true)]
async fn imports_leave_no_copy_of_the_key() {
    let canary = Canary::secp256k1(&KEY);

    let bytes = KEY.to_vec();
    let ((), hits) = scan_async(&[canary], async move {
        sign_and_drop(local_signer_from_private_key(bytes.into()).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0], "local_signer_from_private_key");

    let bytes = KEY.to_vec();
    let ((), hits) = scan_async(&[canary], async move {
        sign_and_drop(LocalSigningKey::from_bytes(bytes.into()).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0], "LocalSigningKey::from_bytes");

    let lower = format!("0x{}", hex::encode(KEY));
    let ((), hits) = scan_async(&[canary], async move {
        sign_and_drop(LocalSigningKey::from_hex(lower.into()).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0], "LocalSigningKey::from_hex, lowercase");

    // The uppercase digits are their own form, so they get their own canary.
    let upper_digits = hex::encode_upper(KEY);
    let upper_canary = Canary::new(upper_digits.as_bytes());
    let upper = format!("0x{upper_digits}");
    let ((), hits) = scan_async(&[canary, upper_canary], async move {
        sign_and_drop(LocalSigningKey::from_hex(upper.into()).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0, 0], "LocalSigningKey::from_hex, uppercase");
}

/// Lower `value` as a host argument, then lift it into the parameter type of
/// the function under test, as the UniFFI scaffolding does before it builds
/// the function's future.
fn lift<B, T>(value: B) -> T
where
    B: Lower<UniFfiTag>,
    T: Lift<UniFfiTag, FfiType = B::FfiType>,
{
    T::try_lift(B::lower(value)).unwrap()
}

/// A host can cancel an import before its first poll; UniFFI then drops the
/// future unpolled. The key argument that the future owns is wiped. The lift
/// runs before the scan here; `lift_wipes_the_host_buffer` covers it.
#[xmtp_common::test(unwrap_try = true)]
fn cancelled_import_wipes_its_argument() {
    let canary = Canary::secp256k1(&KEY);

    let future = local_signer_from_private_key(lift(KEY.to_vec()));
    let ((), hits) = scan(&[canary], || drop(future));
    assert_eq!(hits, [0], "local_signer_from_private_key");

    let future = LocalSigningKey::from_bytes(lift(KEY.to_vec()));
    let ((), hits) = scan(&[canary], || drop(future));
    assert_eq!(hits, [0], "LocalSigningKey::from_bytes");

    let future = LocalSigningKey::from_hex(lift(format!("0x{}", hex::encode(KEY))));
    let ((), hits) = scan(&[canary], || drop(future));
    assert_eq!(hits, [0], "LocalSigningKey::from_hex");
}

/// The host passes a key in a `RustBuffer`, which the lift frees. Lowering
/// runs before the scan, as the host does; the lift and the drop of the lifted
/// argument run inside it. A lift that copies out of the buffer and frees it
/// unwiped, as the builtin `Vec<u8>` lift does, leaves the key behind.
#[xmtp_common::test(unwrap_try = true)]
fn lift_wipes_the_host_buffer() {
    let canary = Canary::secp256k1(&KEY);

    let buffer = <Vec<u8> as Lower<UniFfiTag>>::lower(KEY.to_vec());
    let ((), hits) = scan(&[canary], || {
        drop(<SecretBytes as Lift<UniFfiTag>>::try_lift(buffer).unwrap())
    });
    assert_eq!(hits, [0], "SecretBytes lift");

    let buffer = <String as Lower<UniFfiTag>>::lower(format!("0x{}", hex::encode(KEY)));
    let ((), hits) = scan(&[canary], || {
        drop(<SecretString as Lift<UniFfiTag>>::try_lift(buffer).unwrap())
    });
    assert_eq!(hits, [0], "SecretString lift");
}

/// An invalid scalar is wiped on the error path too.
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_import_leaves_no_copy_of_the_input() {
    const INVALID: [u8; 32] = [0xff; 32];
    let bytes = INVALID.to_vec();
    let (result, hits) = scan_async(&[Canary::secp256k1(&INVALID)], async move {
        LocalSigningKey::from_bytes(bytes.into()).await.map(|_| ())
    })
    .await;
    assert!(result.is_err());
    assert_eq!(hits, [0]);
}

/// Export makes exactly one copy: the returned value.
#[xmtp_common::test(unwrap_try = true)]
async fn export_makes_only_the_returned_copy() {
    let key = LocalSigningKey::from_bytes(KEY.to_vec().into()).await?;
    let ((bytes, hex), hits) = scan_async(&[Canary::secp256k1(&KEY)], async {
        // Lower the byte export as the scaffolding does: the lowered buffer
        // is the export the host owns, and nothing else may hold the key.
        let bytes = <SecretBytes as LowerReturn<UniFfiTag>>::lower_return(key.to_bytes())
            .ok()
            .unwrap();
        // UniFFI lowers a `String` by handing over its own allocation, so the
        // lowered hex is the export and no copy is freed.
        let hex = <String as LowerReturn<UniFfiTag>>::lower_return(key.to_hex())
            .ok()
            .unwrap();
        (ManuallyDrop::new(bytes), ManuallyDrop::new(hex))
    })
    .await;
    assert_eq!(hits, [0]);
    let lowered = ManuallyDrop::into_inner(bytes).destroy_into_vec();
    assert_eq!(lowered[..4], 32_i32.to_be_bytes());
    assert_eq!(lowered[4..], KEY);
    let lowered_hex = ManuallyDrop::into_inner(hex).destroy_into_vec();
    assert_eq!(lowered_hex, format!("0x{}", hex::encode(KEY)).into_bytes());
}

/// A malformed host buffer fails without leaving a partial copy of the key:
/// a byte string truncated after half the key, and hex text with an invalid
/// UTF-8 byte. The builtin readers copy before they fail.
#[xmtp_common::test(unwrap_try = true)]
fn malformed_inputs_leave_no_partial_copy() {
    let half = &KEY[..16];
    let mut truncated = 32_i32.to_be_bytes().to_vec();
    truncated.extend_from_slice(half);
    let buffer = RustBuffer::from_vec(truncated);
    let (result, hits) = scan(&[Canary::new(half)], || {
        <SecretBytes as Lift<UniFfiTag>>::try_lift(buffer).is_err()
    });
    assert!(result, "a truncated key must fail");
    assert_eq!(hits, [0], "truncated SecretBytes");

    let text = hex::encode(KEY);
    let mut invalid = 65_i32.to_be_bytes().to_vec();
    invalid.extend_from_slice(text.as_bytes());
    invalid.push(0xff);
    let (result, hits) = scan(&[Canary::new(text.as_bytes())], || {
        let mut buf = invalid.as_slice();
        <SecretString as Lift<UniFfiTag>>::try_read(&mut buf).is_err()
    });
    assert!(result, "invalid UTF-8 must fail");
    assert_eq!(hits, [0], "invalid UTF-8 SecretString");

    // The same text as a host `RustBuffer` (raw bytes, no length prefix).
    let mut invalid = text.as_bytes().to_vec();
    invalid.push(0xff);
    let buffer = RustBuffer::from_vec(invalid);
    let (result, hits) = scan(&[Canary::new(text.as_bytes())], || {
        <SecretString as Lift<UniFfiTag>>::try_lift(buffer).is_err()
    });
    assert!(result, "invalid UTF-8 must fail the lift");
    assert_eq!(hits, [0], "invalid UTF-8 SecretString lift");
}

/// k256 wipes its scalar when a signer drops. This guards the k256 `zeroize`
/// feature through dependency updates.
#[xmtp_common::test(unwrap_try = true)]
async fn wallet_signer_is_wiped_on_drop() {
    let ((), hits) = scan_async(&[Canary::secp256k1(&KEY)], async {
        let signer = Arc::new(PrivateKeySigner::from_slice(&KEY).unwrap());
        InboxOwner::sign(&*signer, "XMTP zeroize proof").unwrap();
        drop(signer);
    })
    .await;
    assert_eq!(hits, [0]);
}
