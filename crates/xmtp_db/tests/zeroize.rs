//! Heap-canary proofs that the MLS key store wipes the private keys it writes
//! and reads. Native only: it installs the heap-canary global allocator.
//! No backend needed.
//!
//! In CI the test macro installs no subscriber, so span fields would never be
//! evaluated. Each scan runs its key store calls under a trace layer
//! ([`traced`]), so a span field that copies a key frees that copy inside the
//! scan.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(unix)]

use openmls::{
    group::GroupId,
    prelude::{GroupEpoch, KeyPackageBundle, SignatureScheme},
    treesync::{
        EncryptionKey,
        test_utils::{read_keys_from_key_store, write_keys_from_key_store},
    },
};
use openmls_basic_credential::{SignatureKeyPair, StorageId};
use openmls_libcrux_crypto::Provider as LibcruxProvider;
use openmls_traits::{
    OpenMlsProvider,
    crypto::OpenMlsCrypto,
    storage::{CURRENT_VERSION, Entity, StorageProvider, traits},
    types::HpkeKeyPair,
};
use serde::{Deserialize, Serialize};
use xmtp_cryptography::{
    XmtpInstallationCredential,
    configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE},
    secret::{
        Secret,
        heap_canary::{Canary, scan},
    },
};
use xmtp_db::{
    DefaultMlsStore, TestDb, XmtpMlsStorageProvider, XmtpOpenMlsProvider, XmtpTestDb,
    sql_key_store::{
        COMMIT_LOG_SIGNER_PRIVATE_KEY, KEY_PACKAGE_WRAPPER_PRIVATE_KEY, SqlKeyStore,
        codec::{encode_bytes, encode_key, encode_value},
    },
    test_utils::generate_key_package_bundle,
};

xmtp_cryptography::install_heap_canary!();

const SIGNATURE_KEY: [u8; 32] = [
    0x5e, 0x91, 0x0b, 0xc7, 0x3a, 0xf2, 0x68, 0x14, 0xd9, 0x2f, 0x86, 0x41, 0xbe, 0x07, 0x7c, 0xe3,
    0x29, 0xa5, 0x50, 0x1d, 0xf8, 0x6b, 0x93, 0x0e, 0xc4, 0x37, 0x8a, 0xd1, 0x62, 0x1f, 0xb6, 0x4d,
];
const LIST_KEYS: [[u8; 32]; 2] = [
    [
        0xa7, 0x13, 0x6e, 0xd0, 0x45, 0x9b, 0x22, 0xfc, 0x81, 0x3d, 0xe9, 0x56, 0x0a, 0xb4, 0x7f,
        0x18, 0xc2, 0x6d, 0x95, 0x3b, 0xe0, 0x4a, 0x17, 0x8e, 0xd5, 0x29, 0x73, 0xbf, 0x06, 0xf1,
        0x5c, 0x88,
    ],
    [
        0x39, 0xd6, 0x84, 0x1b, 0xf5, 0x60, 0xae, 0x27, 0xc9, 0x0d, 0x72, 0xeb, 0x48, 0x93, 0x1e,
        0xb5, 0x6f, 0x02, 0xda, 0x87, 0x34, 0xcc, 0x59, 0x10, 0xa3, 0xfe, 0x65, 0x2b, 0x97, 0x4e,
        0xd2, 0x7a,
    ],
];
const LEAF_IKM: [u8; 32] = [0x11; 32];
const EPOCH_IKM: [u8; 32] = [0x22; 32];
const POST_QUANTUM_IKM: [u8; 32] = [0x33; 32];
const LIST_LABEL: &[u8] = b"ZeroizeTestList";
const LIST_KEY: &[u8] = b"zeroize list";
const WRAPPER_HASH_REF: &[u8] = b"zeroize wrapper key hash ref";
/// A label no production code uses, for rows read through a plain type.
const PLAIN_LABEL: &[u8] = b"ZeroizeTestPlain";
const PLAIN_KEY: &[u8] = b"zeroize plain";
const SIGNER_GROUP: &[u8] = b"zeroize commit log group";

/// Run `run` under a trace layer, which evaluates every span field and
/// records every event, and drop the exported spans before returning, so any
/// copy they hold is freed inside the caller's scan. Returns whether a key
/// store decode failure was logged.
fn traced<T>(run: impl FnOnce() -> T) -> (T, bool) {
    let (result, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
        tracing::error_span!("zeroize").in_scope(run)
    });
    let logged = format!("{spans:?}").contains("key store value decode failed");
    (result, logged)
}

/// A key store on a new encrypted test database.
async fn provider() -> XmtpOpenMlsProvider<DefaultMlsStore> {
    let store = TestDb::create_persistent_store(None).await;
    XmtpOpenMlsProvider::new(SqlKeyStore::new(store.db()))
}

/// An HPKE key pair in an epoch's key pair list. OpenMLS keeps its own
/// list entry type crate-private.
#[derive(Serialize, Deserialize)]
struct EpochKeyPair(HpkeKeyPair);
impl traits::HpkeKeyPair<CURRENT_VERSION> for EpochKeyPair {}
impl Entity<CURRENT_VERSION> for EpochKeyPair {}

/// A list entry that holds a key.
#[derive(Serialize, Deserialize)]
struct ListEntry(Secret);
impl Entity<CURRENT_VERSION> for ListEntry {}

/// A key package bundle holds the init and leaf encryption private keys.
#[xmtp_common::test(unwrap_try = true)]
async fn key_package_bundle_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let bundle = generate_key_package_bundle();
    let hash_ref = bundle.key_package().hash_ref(provider.crypto())?;
    let canaries = [
        Canary::new(bundle.init_private_key()),
        Canary::new(bundle.encryption_private_key()),
    ];

    let ((read_back, _), hits) = scan(&canaries, || {
        traced(|| {
            store.write_key_package(&hash_ref, &bundle).unwrap();
            let read: Option<KeyPackageBundle> = store.key_package(&hash_ref).unwrap();
            store.delete_key_package(&hash_ref).unwrap();
            read.is_some_and(|read| read.init_private_key() == bundle.init_private_key())
        })
    });

    assert!(read_back);
    assert_eq!(hits, [0, 0], "freed heap blocks held a key package key");
}

/// An HPKE key pair, stored alone and in an epoch's key pair list.
#[xmtp_common::test(unwrap_try = true)]
async fn hpke_key_pairs_are_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let crypto = provider.crypto();
    let leaf = crypto.derive_hpke_keypair(CIPHERSUITE.hpke_config(), &LEAF_IKM)?;
    let epoch_pairs = [EpochKeyPair(
        crypto.derive_hpke_keypair(CIPHERSUITE.hpke_config(), &EPOCH_IKM)?,
    )];
    let leaf_public = EncryptionKey::from(leaf.public.clone());
    let group_id = GroupId::from_slice(b"zeroize group");
    let epoch = GroupEpoch::from(1);
    let canaries = [
        Canary::new(&leaf.private),
        Canary::new(&epoch_pairs[0].0.private),
    ];

    let ((read_back, _), hits) = scan(&canaries, || {
        traced(|| {
            write_keys_from_key_store(&provider, leaf.clone());
            let read_leaf = read_keys_from_key_store(&provider, &leaf_public);
            store.delete_encryption_key_pair(&leaf_public).unwrap();

            store
                .write_encryption_epoch_key_pairs(&group_id, &epoch, 0, &epoch_pairs)
                .unwrap();
            let read_epoch: Vec<EpochKeyPair> = store
                .encryption_epoch_key_pairs(&group_id, &epoch, 0)
                .unwrap();
            store
                .delete_encryption_epoch_key_pairs(&group_id, &epoch, 0)
                .unwrap();

            *read_leaf.private == *leaf.private
                && read_epoch.len() == 1
                && *read_epoch[0].0.private == *epoch_pairs[0].0.private
        })
    });

    assert!(read_back);
    assert_eq!(hits, [0, 0], "freed heap blocks held an HPKE private key");
}

/// A signature key pair, as OpenMLS stores one for each leaf.
#[xmtp_common::test(unwrap_try = true)]
async fn signature_key_pair_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let public = XmtpInstallationCredential::from_bytes(&SIGNATURE_KEY)?
        .public_slice()
        .to_vec();
    let key_pair =
        SignatureKeyPair::from_raw(SignatureScheme::ED25519, SIGNATURE_KEY.to_vec(), public);
    let id = StorageId::from(key_pair.to_public_vec());

    let ((read_back, _), hits) = scan(&[Canary::new(&SIGNATURE_KEY)], || {
        traced(|| {
            store.write_signature_key_pair(&id, &key_pair).unwrap();
            let read: Option<SignatureKeyPair> = store.signature_key_pair(&id).unwrap();
            store.delete_signature_key_pair(&id).unwrap();
            read.is_some()
        })
    });

    assert!(read_back);
    assert_eq!(hits, [0], "freed heap blocks held the signature key");
}

/// A stored key with a byte after its value fails to decode, also through a
/// reader that does not wipe: here a `Vec<u8>`, as an OpenMLS type reads a
/// private key field, stored as an array of integers. The codec's pre-scan
/// rejects the row before any decode, so no copy of the key is built and
/// freed unwiped, also when the decode error is logged.
#[xmtp_common::test(unwrap_try = true)]
async fn secret_with_trailing_bytes_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let key = LIST_KEYS[0];
    let mut row = encode_value(PLAIN_LABEL, key.as_slice())?;
    row.push(0);

    let ((failed, logged), hits) = scan(&[Canary::new(&key)], || {
        store
            .write::<CURRENT_VERSION>(PLAIN_LABEL, PLAIN_KEY, &row)
            .unwrap();
        let (read, logged) =
            traced(|| store.read::<CURRENT_VERSION, Vec<u8>>(PLAIN_LABEL, PLAIN_KEY));
        (read.is_err(), logged)
    });

    assert!(failed, "a row with a trailing byte decoded");
    assert!(logged, "the decode failure was not logged");
    assert_eq!(hits, [0], "freed heap blocks held the stored key");
}

/// A stored secret in an indefinite-length byte string whose second chunk is
/// retyped (an integer where a byte string must be) is one well-formed CBOR
/// item. cbor4ii would copy the first chunk, the whole key, into a buffer
/// that is not wiped and then fail on the integer. The codec's pre-scan
/// rejects the row before any decode, so the key is never left in a freed
/// buffer, also when the decode error is logged.
#[xmtp_common::test(unwrap_try = true)]
async fn secret_with_a_retyped_item_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let key = LIST_KEYS[0];
    // An indefinite-length byte string (`5f`), the key as its first chunk,
    // an integer (`01`) where the next chunk must be, and the break (`ff`).
    let mut row = encode_bytes(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &key)?;
    row.insert(0, 0x5f);
    row.extend([0x01, 0xff]);

    let ((failed, logged), hits) = scan(&[Canary::new(&key)], || {
        store
            .write::<CURRENT_VERSION>(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, WRAPPER_HASH_REF, &row)
            .unwrap();
        let (read, logged) = traced(|| store.wrapper_private_key(WRAPPER_HASH_REF).map(drop));
        (read.is_err(), logged)
    });

    assert!(failed, "a retyped chunk decoded");
    assert!(logged, "the decode failure was not logged");
    assert_eq!(hits, [0], "freed heap blocks held the stored key");
}

/// An identity row that fails to decode partway frees no unwiped copy of the
/// installation key: neither when its private key has a retyped item, nor
/// when the key decodes and a later field (an unknown signature scheme)
/// fails.
#[xmtp_common::test(unwrap_try = true)]
fn installation_key_row_that_fails_partway_is_wiped() {
    let row = xmtp_db::db_serialize(&XmtpInstallationCredential::from_bytes(&SIGNATURE_KEY)?)?;
    let field = |name: &[u8]| {
        let text = [&[0x60 | name.len() as u8], name].concat();
        row.windows(text.len())
            .position(|window| window == text)
            .expect("a field of the row")
    };
    let mut retyped = row.clone();
    let at = field(b"public") - 2;
    assert_eq!(
        retyped[at], 0x18,
        "the last private key byte is a one-byte integer item"
    );
    retyped[at] = 0x38;
    let mut unknown_scheme = row.clone();
    let scheme = field(b"ED25519") + b"ED25519".len();
    unknown_scheme[scheme] ^= 1;

    for (row, canary) in [
        (retyped, &SIGNATURE_KEY[..31]),
        (unknown_scheme, &SIGNATURE_KEY[..]),
    ] {
        let (failed, hits) = scan(&[Canary::new(canary)], || {
            xmtp_db::db_deserialize::<XmtpInstallationCredential>(&row).is_err()
        });
        assert!(failed, "a corrupted identity row decoded");
        assert_eq!(hits, [0], "freed heap blocks held the installation key");
    }
}

/// Appending to a list, reading it and removing an entry rewrite the whole
/// list. Neither the list nor the removed entry is freed unwiped.
#[xmtp_common::test(unwrap_try = true)]
async fn list_entries_are_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let [first, second] = LIST_KEYS
        .map(|key| encode_value(LIST_LABEL, &ListEntry(Secret::new(key.to_vec()))).unwrap());
    let canaries = LIST_KEYS.map(|key| Canary::new(&key));

    let ((read_back, _), hits) = scan(&canaries, || {
        traced(|| {
            // The first append writes a new list; the second rewrites it.
            store
                .append::<CURRENT_VERSION>(LIST_LABEL, LIST_KEY, &first)
                .unwrap();
            store
                .append::<CURRENT_VERSION>(LIST_LABEL, LIST_KEY, &second)
                .unwrap();
            let both: Vec<ListEntry> = store
                .read_list::<CURRENT_VERSION, ListEntry>(LIST_LABEL, LIST_KEY)
                .unwrap();
            store
                .remove_item::<CURRENT_VERSION>(LIST_LABEL, LIST_KEY, &first)
                .unwrap();
            let rest: Vec<ListEntry> = store
                .read_list::<CURRENT_VERSION, ListEntry>(LIST_LABEL, LIST_KEY)
                .unwrap();
            store
                .delete::<CURRENT_VERSION>(LIST_LABEL, LIST_KEY)
                .unwrap();
            both.len() == 2 && rest.len() == 1 && rest[0].0.as_slice() == LIST_KEYS[1]
        })
    });

    assert!(read_back);
    assert_eq!(hits, [0, 0], "freed heap blocks held a list entry key");
}

/// A list whose second item is an integer, not a byte string, is one
/// well-formed CBOR item, so it passes the pre-scan and fails after its
/// first item, a key, is read. That item goes straight into the list's wiped
/// buffer, so it is never freed unwiped, also when the error is logged.
#[xmtp_common::test(unwrap_try = true)]
async fn list_that_fails_partway_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let key = LIST_KEYS[0];
    // An array of two items: the key as a byte string, then the integer 1.
    let row = [&[0x82, 0x58, 0x20][..], &key, &[0x01]].concat();

    let ((failed, logged), hits) = scan(&[Canary::new(&key)], || {
        store
            .write::<CURRENT_VERSION>(LIST_LABEL, LIST_KEY, &row)
            .unwrap();
        let (read, logged) = traced(|| {
            store
                .read_list::<CURRENT_VERSION, ListEntry>(LIST_LABEL, LIST_KEY)
                .map(drop)
        });
        (read.is_err(), logged)
    });

    assert!(failed, "a list with an integer item decoded");
    assert!(logged, "the decode failure was not logged");
    assert_eq!(hits, [0], "freed heap blocks held the list's key");
}

/// The post-quantum wrapper private key, written and read only through its
/// two typed methods.
#[xmtp_common::test(unwrap_try = true)]
async fn wrapper_private_key_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let post_quantum = LibcruxProvider::default()
        .crypto()
        .derive_hpke_keypair(POST_QUANTUM_CIPHERSUITE.hpke_config(), &POST_QUANTUM_IKM)?;

    let ((read_back, _), hits) = scan(&[Canary::new(&post_quantum.private)], || {
        traced(|| {
            store
                .write_wrapper_private_key(WRAPPER_HASH_REF, &post_quantum.private)
                .unwrap();
            let read = store.wrapper_private_key(WRAPPER_HASH_REF).unwrap();
            read.is_some_and(|key| key.as_slice() == &*post_quantum.private)
        })
    });

    assert!(read_back);
    assert_eq!(hits, [0], "freed heap blocks held the wrapper private key");
}

/// The commit-log signer key, written and read only through its two typed
/// methods. A stored signer in an indefinite-length byte string with a
/// retyped chunk, which fails to decode, leaves no copy either.
#[xmtp_common::test(unwrap_try = true)]
async fn commit_log_signer_is_wiped() {
    let provider = provider().await;
    let store = provider.storage();
    let key = LIST_KEYS[1];
    let mut retyped = encode_bytes(COMMIT_LOG_SIGNER_PRIVATE_KEY, &key)?;
    retyped.insert(0, 0x5f);
    retyped.extend([0x01, 0xff]);
    let storage_key = encode_key(COMMIT_LOG_SIGNER_PRIVATE_KEY, SIGNER_GROUP)?;

    let ((read_back, failed), hits) = scan(&[Canary::new(&key)], || {
        let (read_back, _) = traced(|| {
            store.write_commit_log_signer(SIGNER_GROUP, &key).unwrap();
            let read = store.commit_log_signer(SIGNER_GROUP).unwrap();
            read.is_some_and(|read| read.as_slice() == key)
        });
        store
            .write::<CURRENT_VERSION>(COMMIT_LOG_SIGNER_PRIVATE_KEY, &storage_key, &retyped)
            .unwrap();
        let (read, _) = traced(|| store.commit_log_signer(SIGNER_GROUP).map(drop));
        (read_back, read.is_err())
    });

    assert!(read_back);
    assert!(failed, "a retyped chunk decoded");
    assert_eq!(hits, [0], "freed heap blocks held the commit-log signer");
}
