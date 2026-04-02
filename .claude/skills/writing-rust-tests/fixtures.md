# Fixtures & tester! Macro

## tester! Options

```rust
tester!(alix);                              // Default client
tester!(bo, with_name: "bo");               // Named (appears in logs)
tester!(alix2, from: alix1);               // New installation, same identity
tester!(alix, sync_worker);                 // Enable device sync worker
tester!(alix, disable_workers);             // No background workers
tester!(alix, stream);                      // Enable streaming
tester!(alix, proxy);                       // Enable toxiproxy
tester!(alix, ephemeral_db);                // In-memory DB (for snapshots)
tester!(alix, snapshot: snap_arc);          // Restore from Arc<Vec<u8>> snapshot
```

Options chain: `tester!(bo, from: alix, sync_worker, with_name: "bo2");`

The macro calls methods on `TesterBuilder`, so any `TesterBuilder` method works as an option key.

**Source:** `crates/xmtp_mls/src/utils/test/tester_utils.rs`

## Convenience Methods on Tester (Client)

```rust
// Create group, invite other, exchange a message — returns (MlsGroup, msg_text)
let (group, msg) = alix.test_talk_in_new_group_with(&bo).await?;

// Create DM, set consent, exchange a message — returns (MlsGroup, msg_text)
let (dm, msg) = alix.test_talk_in_dm_with(&bo).await?;

// Verify two installations share a sync group
alix1.test_has_same_sync_group_as(&alix2).await?;
```

**Source:** `crates/xmtp_mls/src/test/client_test_utils.rs`

## Convenience Methods on MlsGroup

```rust
// Send message and verify other side received it
group.test_can_talk_with(&bo_group).await?;

// Get last application message bytes
let bytes: Option<Vec<u8>> = group.test_last_message_bytes().await?;

// Get last message directly from the test backend (bypasses local DB cache)
let msg = group.test_get_last_message_from_network().await?;
```

**Source:** `crates/xmtp_mls/src/test/group_test_utils.rs`

## MlsGroupExt Trait (shorthand)

```rust
use crate::utils::test::MlsGroupExt;

group.invite(&bo).await?;       // add_members shorthand
group.send_msg(b"hello").await;  // send_message shorthand
```

**Source:** `crates/xmtp_mls/src/utils/test/tester_utils_trait_ext.rs`

## Tester Internals

Access underlying components when needed:

```rust
alix.client          // The FullXmtpClient
alix.inbox_id()      // Inbox ID string
alix.db()            // Database reference
alix.worker()        // Arc<WorkerMetrics<SyncMetric>> (panics if no sync_worker)
alix.proxies()       // ToxicProxies (panics if no proxy)
alix.db_snapshot()   // Vec<u8> (requires ephemeral_db)
```

## Device Sync Testing Pattern

```rust
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_sync() {
    tester!(alix1, sync_worker, with_name: "alix1");
    tester!(bo, disable_workers);

    let (dm, msg) = alix1.test_talk_in_dm_with(&bo).await?;

    // Second installation of same identity
    tester!(alix2, from: alix1, with_name: "alix2");
    alix2.device_sync_client()
        .send_sync_request(ArchiveOptions::msgs_and_consent(), DeviceSyncUrls::LOCAL_ADDRESS)
        .await?;

    // Wait for sync to complete
    alix1.worker()
        .register_interest(SyncMetric::PayloadSent, 1)
        .wait().await?;
}
```

## bindings/mobile Fixtures

Mobile tests use a different builder since they work with `FfiXmtpClient`:

```rust
use crate::mls::test_utils::{LocalBuilder, LocalTester};

// Quick client creation
let client = new_test_client().await;
let client = new_test_client_with_wallet(wallet).await;

// Builder pattern
let alex = TesterBuilder::new().sync_worker().stream().build().await;
```

Streaming tests use `RustStreamCallback`:

```rust
let cb = Arc::new(RustStreamCallback::default());
let stream = bo.conversations().stream_all_messages(cb.clone(), None).await;
stream.wait_for_ready().await;

alix_group.send(b"hello".to_vec(), FfiSendMessageOpts::default()).await?;
cb.wait_for_delivery(None).await.unwrap();
assert_eq!(cb.message_count(), 1);

stream.end_and_wait().await.unwrap();
```

**Source:** `bindings/mobile/src/mls/test_utils.rs`, `bindings/mobile/src/mls/tests/mod.rs`
