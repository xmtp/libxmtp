# Fixtures and the `tester!` macro

## `tester!` options

```rust
tester!(alix);                                    // default client, named "alix" in logs
tester!(alix2, from: alix1, with_name: "alix2");  // same identity, new installation; from: keeps the old name unless set
tester!(bo, passkey);                             // passkey identity instead of a wallet
tester!(alix, sync_worker);                       // device sync worker on
tester!(alix, disable_workers);                   // no background workers
tester!(alix, stream);                            // streaming on
tester!(alix, proxy);                             // through toxiproxy; native only
tester!(alix, persistent_db);                     // on-disk DB; the default is in-memory
tester!(alix, snapshot: snap_arc);                // restore from Arc<Vec<u8>>
tester!(alix, backend: &backend);                 // an EphemeralBackend; see backend.md
tester!(alix, backend: &backend, auth: cb);       // plus an Arc<dyn AuthCallback>
tester!(alix, api_client: client);                // a prebuilt TestClient
```

Options chain: `tester!(bo, from: alix, sync_worker, with_name: "bo2")`. The
macro calls methods on `TesterBuilder`, so any builder method works as a key:
`sync_mode`, `worker_config`, `change_callbacks`, `triggers`, `version`,
`external_identity`, `do_not_wait_for_init`, and more.

**Source:** `crates/xmtp_mls/src/utils/test/tester_utils.rs`

## Convenience methods on `Tester`

```rust
let (group, msg) = alix.test_talk_in_new_group_with(&bo).await?;   // (MlsGroup, String)
let (dm, msg) = alix.test_talk_in_dm_with(&bo).await?;              // consent set, one message
alix1.test_has_same_sync_group_as(&alix2).await?;
```

**Source:** `crates/xmtp_mls/src/test/client_test_utils.rs`

## Convenience methods on `MlsGroup`

```rust
let sent: String = group.test_can_talk_with(&bo_group).await?;    // sends, verifies receipt
let bytes: Option<Vec<u8>> = group.test_last_message_bytes().await?;
let msg = group.test_get_last_message_from_network().await?;     // bypasses the local DB
```

**Source:** `crates/xmtp_mls/src/test/group_test_utils.rs`

## `MlsGroupExt` shorthand

```rust
use crate::utils::test::MlsGroupExt;
group.invite(&bo).await?;         // add_members
group.send_msg(b"hello").await;   // send_message; unwraps
```

## Tester internals

```rust
alix.client                       // FullXmtpClient
alix.inbox_id()                   // String
alix.identifier()                 // Identifier
alix.db()                         // database handle
alix.new_installation().await     // second installation of the same identity
alix.worker()                     // Arc<WorkerMetrics<SyncMetric>>; panics without sync_worker
alix.for_each_proxy(async |p| { .. }).await   // panics without proxy
alix.db_snapshot()                // Vec<u8>; panics with persistent_db
```

For a raw `FullXmtpClient` instead of a `Tester`, use
`ClientBuilder::new_test_client(&owner)` from `crates/xmtp_mls/src/utils/test/mod.rs`.

## Device sync pattern

```rust
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_sync() {
    tester!(alix1, sync_worker);
    tester!(bo, disable_workers);
    let (dm, msg) = alix1.test_talk_in_dm_with(&bo).await?;

    tester!(alix2, from: alix1, with_name: "alix2");
    alix1.test_has_same_sync_group_as(&alix2).await?;
    alix1.worker().register_interest(SyncMetric::HmacSent, 1).wait().await?;
}
```

## SDK fixtures

Use the helpers in `crates/xmtp_sdk/src/tests` for public façade tests.
Use core fixtures for storage and protocol tests.
