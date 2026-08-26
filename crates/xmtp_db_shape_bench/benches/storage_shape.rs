//! Storage-shape A/B microbench.
//!
//! Isolates the one thing the blocking-vs-ready-future change touches: openmls'
//! `StorageProvider` methods on `SqlKeyStore` going `fn` -> ready-future `async fn`
//! over the same diesel/SQLite backend. No network, no test-utils, no client.
//!
//!   blocking (today):            cargo bench -p xmtp_db_shape_bench (from the crate dir)
//!   ready-future (is_sync off):  RUSTC_BOOTSTRAP=1 cargo bench --no-default-features
//!
//! Both arms are driven the same way -- a single poll of an always-ready future via
//! `now_or_never()` on the async arm, a direct call on the blocking arm -- so the
//! async arm carries no executor overhead the blocking arm lacks. Any delta is the
//! ready-future construction + poll, which is what we want to measure.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use openmls_traits::storage::traits::{SignatureKeyPair, SignaturePublicKey};
use openmls_traits::storage::{CURRENT_VERSION, Entity, Key, StorageProvider};
use serde::{Deserialize, Serialize};
use xmtp_db::sql_key_store::SqlKeyStore;
use xmtp_db::{EncryptedMessageStore, NativeDb};

// Resolve a StorageProvider call regardless of shape: a direct value on the blocking
// shape, one poll of the already-ready future on the ready-future shape.
macro_rules! drive {
    ($e:expr) => {{
        #[cfg(feature = "blocking")]
        {
            $e
        }
        #[cfg(not(feature = "blocking"))]
        {
            futures::FutureExt::now_or_never($e)
                .expect("diesel-backed storage future resolves synchronously")
        }
    }};
}

#[derive(Serialize)]
struct PubKey(Vec<u8>);
impl Key<CURRENT_VERSION> for PubKey {}
impl SignaturePublicKey<CURRENT_VERSION> for PubKey {}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
struct StoredKeyPair(Vec<u8>);
impl Entity<CURRENT_VERSION> for StoredKeyPair {}
impl SignatureKeyPair<CURRENT_VERSION> for StoredKeyPair {}

fn bench_storage(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("shape_bench.db3");
    let db = NativeDb::builder()
        .persistent(path.to_str().expect("utf8 path"))
        .key([0u8; 32])
        .single_connection()
        .build()
        .expect("build native db");
    let store = EncryptedMessageStore::new(db).expect("create encrypted store");
    let key_store = SqlKeyStore::new(store.conn());

    let pk = PubKey(vec![7u8; 32]);
    let kp = StoredKeyPair(vec![9u8; 96]);
    // Seed one entry so the read path returns Some(..).
    drive!(key_store.write_signature_key_pair::<PubKey, StoredKeyPair>(&pk, &kp))
        .expect("seed write");

    let mut group = c.benchmark_group("storage_shape");
    group.sample_size(200);

    group.bench_function("write_signature_key_pair", |b| {
        b.iter(|| {
            drive!(
                key_store
                    .write_signature_key_pair::<PubKey, StoredKeyPair>(black_box(&pk), black_box(&kp))
            )
            .expect("write");
        });
    });

    group.bench_function("read_signature_key_pair", |b| {
        b.iter(|| {
            let got: Option<StoredKeyPair> =
                drive!(key_store.signature_key_pair::<PubKey, StoredKeyPair>(black_box(&pk)))
                    .expect("read");
            black_box(got);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_storage);
criterion_main!(benches);
