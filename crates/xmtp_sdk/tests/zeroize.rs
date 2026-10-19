//! Heap-canary proofs that wallet key import and export wipe every copy the
//! SDK makes. Native only: it installs the heap-canary global allocator.
//! No backend needed. The local signer does not exist under `pure-only`.
#![cfg(all(not(target_arch = "wasm32"), not(feature = "pure-only")))]

use std::{mem::ManuallyDrop, sync::Arc};

use alloy::signers::local::PrivateKeySigner;
use xmtp_cryptography::secret::heap_canary::{Canary, scan_async};
use xmtp_id::InboxOwner;
use xmtp_sdk::{LocalSigningKey, Signer, SigningRequest, local_signer_from_private_key};

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
        sign_and_drop(local_signer_from_private_key(bytes).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0], "local_signer_from_private_key");

    let bytes = KEY.to_vec();
    let ((), hits) = scan_async(&[canary], async move {
        sign_and_drop(LocalSigningKey::from_bytes(bytes).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0], "LocalSigningKey::from_bytes");

    let lower = format!("0x{}", hex::encode(KEY));
    let ((), hits) = scan_async(&[canary], async move {
        sign_and_drop(LocalSigningKey::from_hex(lower).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0], "LocalSigningKey::from_hex, lowercase");

    // The uppercase digits are their own form, so they get their own canary.
    let upper_digits = hex::encode_upper(KEY);
    let upper_canary = Canary::new(upper_digits.as_bytes());
    let upper = format!("0x{upper_digits}");
    let ((), hits) = scan_async(&[canary, upper_canary], async move {
        sign_and_drop(LocalSigningKey::from_hex(upper).await.unwrap()).await;
    })
    .await;
    assert_eq!(hits, [0, 0], "LocalSigningKey::from_hex, uppercase");
}

/// An invalid scalar is wiped on the error path too.
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_import_leaves_no_copy_of_the_input() {
    const INVALID: [u8; 32] = [0xff; 32];
    let bytes = INVALID.to_vec();
    let (result, hits) = scan_async(&[Canary::secp256k1(&INVALID)], async move {
        LocalSigningKey::from_bytes(bytes).await.map(|_| ())
    })
    .await;
    assert!(result.is_err());
    assert_eq!(hits, [0]);
}

/// Export makes exactly one copy: the returned value.
#[xmtp_common::test(unwrap_try = true)]
async fn export_makes_only_the_returned_copy() {
    let key = LocalSigningKey::from_bytes(KEY.to_vec()).await?;
    let ((bytes, hex), hits) = scan_async(&[Canary::secp256k1(&KEY)], async {
        (
            ManuallyDrop::new(key.to_bytes()),
            ManuallyDrop::new(key.to_hex()),
        )
    })
    .await;
    assert_eq!(hits, [0]);
    assert_eq!(ManuallyDrop::into_inner(bytes), KEY);
    assert_eq!(
        ManuallyDrop::into_inner(hex),
        format!("0x{}", hex::encode(KEY))
    );
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
