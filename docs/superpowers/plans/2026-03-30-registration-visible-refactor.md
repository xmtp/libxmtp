# Registration Visible Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `registration_visible.rs` — extract tests, use `retry_async!`, always check `is_ready`, break up the long function.

**Architecture:** Convert single file to directory module. Replace hand-rolled polling with `retry_async!` + exponential backoff. Extract helper methods on `XmtpClient`. Add integration test with full registration flow.

**Tech Stack:** Rust, xmtp_common::retry_async!, futures::FuturesUnordered, xmtp_common::test

---

### Task 1: Convert to directory module and move tests

**Files:**
- Delete: `crates/xmtp_mls/src/registration_visible.rs`
- Create: `crates/xmtp_mls/src/registration_visible/mod.rs`
- Create: `crates/xmtp_mls/src/registration_visible/tests.rs`

- [ ] **Step 1: Create directory and move production code**

Create `crates/xmtp_mls/src/registration_visible/mod.rs` with the full contents of `registration_visible.rs` but **without** the `#[cfg(test)] mod quorum_tests` block at the bottom (lines 303-359). Add a test module declaration instead:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 2: Create test file with existing tests**

Create `crates/xmtp_mls/src/registration_visible/tests.rs` with the existing tests moved from the `quorum_tests` module. Update imports to reference parent module:

```rust
use super::*;

#[test]
fn quorum_percentage_ceiling() {
    let q = Quorum::Percentage(0.5);
    assert_eq!(q.required_count(4), 2);
    assert_eq!(q.required_count(5), 3);
    assert_eq!(q.required_count(1), 1);
    assert_eq!(q.required_count(0), 0);
}

#[test]
fn quorum_absolute() {
    let q = Quorum::Absolute(3);
    assert_eq!(q.required_count(10), 3);
    assert_eq!(q.required_count(2), 3);
}

#[test]
fn visibility_confirmation_options_defaults() {
    let opts = VisibilityConfirmationOptions::default();
    assert!(matches!(opts.quorum, Quorum::Percentage(p) if (p - 0.5).abs() < f32::EPSILON));
    assert_eq!(opts.timeout_ms, 30_000);
}

#[xmtp_common::test]
async fn check_node_visibility_times_out_when_no_envelopes() {
    use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host("http://localhost:1".parse().unwrap());
    let client = builder.build().unwrap();

    let cursor = xmtp_proto::types::Cursor::new(1, 1u32);
    let opts = VisibilityConfirmationOptions {
        timeout_ms: 1_000,
        ..Default::default()
    };

    let result =
        check_node_visibility(&client, 1u32, "ab01ab01ab01ab01", &[0u8; 32], cursor, &opts)
            .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        crate::client::ClientError::RegistrationNotVisible { failed_nodes } => {
            assert_eq!(failed_nodes, vec![1u32]);
        }
        other => panic!("Expected RegistrationNotVisible, got: {:?}", other),
    }
}
```

- [ ] **Step 3: Delete old file**

Delete `crates/xmtp_mls/src/registration_visible.rs`. The `mod registration_visible;` in `lib.rs` already resolves to the directory module.

- [ ] **Step 4: Run tests to verify migration**

Run: `cargo nextest run --profile ci -p xmtp_mls -E 'test(registration_visible)'`
Expected: All 4 existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A crates/xmtp_mls/src/registration_visible/ && git rm crates/xmtp_mls/src/registration_visible.rs
git commit -m "refactor: move registration_visible tests to separate file"
```

---

### Task 2: Add `EnvelopesNotYetVisible` error variant and update `RetryableError`

**Files:**
- Modify: `crates/xmtp_mls/src/client.rs:77-211` (ClientError enum + RetryableError impl)

- [ ] **Step 1: Add error variant**

Add after the `RegistrationNotVisible` variant (line 177 in `client.rs`):

```rust
    /// Envelopes not yet visible.
    ///
    /// Registration envelopes haven't propagated to the node yet. Retryable.
    #[error("Envelopes not yet visible on node {node_id}")]
    EnvelopesNotYetVisible { node_id: u32 },
```

- [ ] **Step 2: Mark it as retryable**

In the `impl RetryableError for ClientError` block (around line 201), add to the match:

```rust
            ClientError::EnvelopesNotYetVisible { .. } => true,
```

- [ ] **Step 3: Run check**

Run: `cargo check -p xmtp_mls`
Expected: Compiles (new variant is unused but that's fine — next task uses it).

- [ ] **Step 4: Commit**

```bash
git add crates/xmtp_mls/src/client.rs
git commit -m "feat: add retryable EnvelopesNotYetVisible error variant"
```

---

### Task 3: Replace manual loop with `retry_async!` in `check_node_visibility`

**Files:**
- Modify: `crates/xmtp_mls/src/registration_visible/mod.rs`

- [ ] **Step 1: Remove `sleep_interval_ms` from `VisibilityConfirmationOptions`**

Remove the `sleep_interval_ms` field from the struct, `Default` impl, and `from_parts` method. The struct becomes:

```rust
#[derive(Debug, Clone)]
pub struct VisibilityConfirmationOptions {
    pub quorum: Quorum,
    pub timeout_ms: u64,
}
```

The `Default` impl becomes:

```rust
impl Default for VisibilityConfirmationOptions {
    fn default() -> Self {
        Self {
            quorum: Quorum::Percentage(0.5),
            timeout_ms: 30_000,
        }
    }
}
```

The `from_parts` method becomes:

```rust
    pub fn from_parts(
        quorum_percentage: Option<f32>,
        quorum_absolute: Option<usize>,
        timeout_ms: Option<u64>,
    ) -> Self {
        let defaults = Self::default();
        let quorum = match (quorum_absolute, quorum_percentage) {
            (Some(n), _) => Quorum::Absolute(n),
            (_, Some(p)) => Quorum::Percentage(p),
            _ => defaults.quorum,
        };
        Self {
            quorum,
            timeout_ms: timeout_ms.unwrap_or(defaults.timeout_ms),
        }
    }
```

- [ ] **Step 2: Rewrite `check_node_visibility` to be single-attempt**

Replace the entire `check_node_visibility` function. It now performs a **single query** and returns:
- `Ok(())` if both envelopes are visible
- `Err(ClientError::EnvelopesNotYetVisible { node_id })` if not yet visible (retryable)
- `Err(ClientError::Generic(...))` on build/decode errors (not retryable)

```rust
/// Perform a single query against one node to check whether both the
/// identity-update envelope and the key-package envelope are visible.
pub(crate) async fn check_node_visibility<C: Client>(
    node_client: &C,
    node_id: u32,
    inbox_id: &str,
    installation_id: &[u8],
    cursor: Cursor,
) -> Result<(), ClientError> {
    use xmtp_proto::types::Topic;

    let inbox_id_bytes = hex::decode(inbox_id)
        .map_err(|e| ClientError::Generic(format!("invalid hex inbox_id: {e}")))?;
    let identity_topic = Topic::new_identity_update(&inbox_id_bytes);
    let key_package_topic = Topic::new_key_package(installation_id);

    let topics = vec![identity_topic.cloned_vec(), key_package_topic.cloned_vec()];

    let mut endpoint = QueryEnvelopes::builder()
        .envelopes(EnvelopesQuery {
            topics,
            originator_node_ids: vec![],
            last_seen: None,
        })
        .build()
        .map_err(|e| {
            ClientError::Generic(format!("failed to build QueryEnvelopes endpoint: {e}"))
        })?;

    let response = endpoint.query(node_client).await.map_err(|e| {
        tracing::warn!(node_id, error = %e, "check_node_visibility: API error querying node");
        ClientError::EnvelopesNotYetVisible { node_id }
    })?;

    let mut identity_visible = false;
    let mut key_package_visible = false;

    for env in &response.envelopes {
        let topic = match env.topic() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    "check_node_visibility: failed to extract topic from envelope: {}",
                    e
                );
                continue;
            }
        };

        match topic.kind() {
            TopicKind::IdentityUpdatesV1 => {
                if let Ok(env_cursor) = env.cursor()
                    && env_cursor.originator_id == cursor.originator_id
                    && env_cursor.sequence_id == cursor.sequence_id
                {
                    identity_visible = true;
                }
            }
            TopicKind::KeyPackagesV1 => {
                key_package_visible = true;
            }
            _ => {}
        }
    }

    if identity_visible && key_package_visible {
        Ok(())
    } else {
        Err(ClientError::EnvelopesNotYetVisible { node_id })
    }
}
```

- [ ] **Step 3: Update imports in mod.rs**

Remove unused imports that were only needed for the manual loop (`sleep`, `Duration`, `Instant`). The remaining imports should be:

```rust
use futures::stream::{FuturesUnordered, StreamExt};
use xmtp_api::XmtpApi;
use xmtp_api_d14n::d14n::QueryEnvelopes;
use xmtp_api_d14n::protocol::traits::Envelope;
use xmtp_api_d14n::protocol::traits::XmtpQuery;
use xmtp_common::retry::RetryableError;
use xmtp_db::{identity::StoredIdentity, prelude::*};
use xmtp_proto::api::{Client, Query};
use xmtp_proto::types::{Cursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

use crate::client::{Client as XmtpClient, ClientError};
use crate::context::XmtpSharedContext;
```

- [ ] **Step 4: Update `poll_node_quorum` to wrap with `retry_async!`**

In the `FuturesUnordered` closure inside `wait_for_registration_visible` (which will become `poll_node_quorum` in Task 5), wrap the `check_node_visibility` call:

```rust
async move {
    let retry = xmtp_common::Retry::builder()
        .retries(100)
        .with_strategy(
            xmtp_common::ExponentialBackoff::builder()
                .total_wait_max(xmtp_common::time::Duration::from_millis(timeout_ms))
                .build(),
        )
        .build();
    let result = xmtp_common::retry_async!(retry, (async {
        check_node_visibility(
            &client,
            node_id,
            &inbox_id,
            &installation_id,
            cursor,
        )
        .await
    }));
    let result = result.map_err(|_| ClientError::RegistrationNotVisible {
        failed_nodes: vec![node_id],
    });
    (node_id, result)
}
```

Note: `retries(100)` is high because `total_wait_max` on the strategy is the real timeout bound — retries just needs to be large enough not to be the limiting factor.

- [ ] **Step 5: Update test for new signature**

In `tests.rs`, update `check_node_visibility_times_out_when_no_envelopes` — remove `&opts` parameter from the call (options are no longer passed to the single-attempt function). The test now validates that a single attempt returns `EnvelopesNotYetVisible`:

```rust
#[xmtp_common::test]
async fn check_node_visibility_returns_not_yet_visible_when_no_envelopes() {
    use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host("http://localhost:1".parse().unwrap());
    let client = builder.build().unwrap();

    let cursor = xmtp_proto::types::Cursor::new(1, 1u32);

    let result =
        check_node_visibility(&client, 1u32, "ab01ab01ab01ab01", &[0u8; 32], cursor)
            .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        crate::client::ClientError::EnvelopesNotYetVisible { node_id } => {
            assert_eq!(node_id, 1u32);
        }
        other => panic!("Expected EnvelopesNotYetVisible, got: {:?}", other),
    }
}
```

Also remove `sleep_interval_ms` from the `visibility_confirmation_options_defaults` test.

- [ ] **Step 6: Run tests**

Run: `cargo nextest run --profile ci -p xmtp_mls -E 'test(registration_visible)'`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/xmtp_mls/src/registration_visible/ crates/xmtp_mls/src/client.rs
git commit -m "refactor: use retry_async! with exponential backoff in check_node_visibility"
```

---

### Task 4: Always check `is_ready` and extract helper methods

**Files:**
- Modify: `crates/xmtp_mls/src/registration_visible/mod.rs`

- [ ] **Step 1: Extract `check_is_ready` method**

Add to the `impl<Context> XmtpClient<Context>` block:

```rust
    fn check_is_ready(&self) -> Result<(), ClientError> {
        if self.identity().is_ready() {
            Ok(())
        } else {
            Err(ClientError::RegistrationNotVisible {
                failed_nodes: vec![],
            })
        }
    }
```

- [ ] **Step 2: Extract `load_registration_cursor` method**

Add to the same impl block:

```rust
    fn load_registration_cursor(&self) -> Result<Option<Cursor>, ClientError> {
        let stored_identity: Option<StoredIdentity> =
            self.context.db().fetch(&()).map_err(ClientError::Storage)?;

        Ok(stored_identity.and_then(|si| {
            match (
                si.registration_cursor_originator_id,
                si.registration_cursor_sequence_id,
            ) {
                (Some(orig_id), Some(seq_id)) => Some(Cursor::new(seq_id as u64, orig_id as u32)),
                _ => None,
            }
        }))
    }
```

- [ ] **Step 3: Extract `poll_node_quorum` method**

Add to the same impl block. This takes the quorum resolution, FuturesUnordered spawning, and result-collection loop from `wait_for_registration_visible`:

```rust
    async fn poll_node_quorum(
        &self,
        cursor: Cursor,
        options: &VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        let node_clients = self
            .context
            .api()
            .get_node_clients()
            .await
            .map_err(|e| ClientError::Api(xmtp_api::dyn_err(e)))?;

        if node_clients.is_empty() {
            tracing::warn!("get_node_clients returned empty map; falling back to is_ready check");
            return self.check_is_ready();
        }

        let total_nodes = node_clients.len();
        let mut required = options.quorum.required_count(total_nodes);
        if required == 0 {
            tracing::warn!("quorum resolved to 0; requiring at least 1 node");
            required = 1;
        } else if required > total_nodes {
            tracing::warn!(
                required,
                total_nodes,
                "quorum exceeds node count; clamping to node count"
            );
            required = total_nodes;
        }

        let inbox_id = self.inbox_id().to_string();
        let installation_id: Vec<u8> = self.installation_public_key().to_vec();
        let timeout_ms = options.timeout_ms;

        let futures: FuturesUnordered<_> = node_clients
            .into_iter()
            .map(|(node_id, client)| {
                let inbox_id = inbox_id.clone();
                let installation_id = installation_id.clone();
                async move {
                    let retry = xmtp_common::Retry::builder()
                        .retries(100)
                        .with_strategy(
                            xmtp_common::ExponentialBackoff::builder()
                                .total_wait_max(xmtp_common::time::Duration::from_millis(
                                    timeout_ms,
                                ))
                                .build(),
                        )
                        .build();
                    let result = xmtp_common::retry_async!(retry, (async {
                        check_node_visibility(
                            &client,
                            node_id,
                            &inbox_id,
                            &installation_id,
                            cursor,
                        )
                        .await
                    }));
                    let result = result.map_err(|_| ClientError::RegistrationNotVisible {
                        failed_nodes: vec![node_id],
                    });
                    (node_id, result)
                }
            })
            .collect();

        let mut confirmed = 0usize;
        let mut failed_nodes: Vec<u32> = Vec::new();

        futures::pin_mut!(futures);
        while let Some((node_id, result)) = futures.next().await {
            match result {
                Ok(()) => {
                    confirmed += 1;
                    if confirmed >= required {
                        return Ok(());
                    }
                }
                Err(_) => {
                    failed_nodes.push(node_id);
                    if total_nodes - failed_nodes.len() < required {
                        return Err(ClientError::RegistrationNotVisible { failed_nodes });
                    }
                }
            }
        }

        Err(ClientError::RegistrationNotVisible { failed_nodes })
    }
```

- [ ] **Step 4: Rewrite `wait_for_registration_visible`**

Replace the entire method body:

```rust
    pub async fn wait_for_registration_visible(
        &self,
        options: VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        self.check_is_ready()?;

        let is_d14n = self
            .context
            .api()
            .is_d14n()
            .map_err(|e| ClientError::Api(xmtp_api::dyn_err(e)))?;

        if !is_d14n {
            return Ok(());
        }

        let Some(cursor) = self.load_registration_cursor()? else {
            tracing::warn!(
                "d14n client has no registration cursor (likely registered before migration); \
                 skipping node visibility check"
            );
            return Ok(());
        };

        self.poll_node_quorum(cursor, &options).await
    }
```

- [ ] **Step 5: Clean up — remove the old `check_is_ready` closure and dead code**

Verify no leftover code from the old implementation remains. The `use xmtp_common::time::{Duration, Instant, sleep};` import inside the old function body is gone since `check_node_visibility` no longer uses them.

- [ ] **Step 6: Run tests**

Run: `cargo nextest run --profile ci -p xmtp_mls -E 'test(registration_visible)'`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/xmtp_mls/src/registration_visible/
git commit -m "refactor: extract check_is_ready, load_registration_cursor, poll_node_quorum; always check is_ready"
```

---

### Task 5: Add integration test

**Files:**
- Modify: `crates/xmtp_mls/src/registration_visible/tests.rs`

- [ ] **Step 1: Add integration test**

Append to `tests.rs`:

```rust
#[xmtp_common::test(unwrap_try = true)]
async fn test_wait_for_registration_visible_after_registration() {
    tester!(alice);
    alice
        .wait_for_registration_visible(VisibilityConfirmationOptions::default())
        .await?;
}
```

The `tester!` macro creates a fully registered client. `Tester` derefs to `Client`, so `wait_for_registration_visible` is callable directly.

- [ ] **Step 2: Run integration test with d14n**

Run: `cargo nextest run --features d14n --profile ci-d14n -p xmtp_mls -E 'test(registration_visible)'`
Expected: All tests pass including the new integration test.

- [ ] **Step 3: Commit**

```bash
git add crates/xmtp_mls/src/registration_visible/tests.rs
git commit -m "test: add integration test for wait_for_registration_visible"
```

---

### Task 6: Final verification and submit

**Files:** None (verification only)

- [ ] **Step 1: Run `just check`**

Run: `just check`
Expected: Workspace compiles cleanly.

- [ ] **Step 2: Run `just lint-rust`**

Run: `just lint-rust`
Expected: No warnings or errors. Fix any clippy/fmt issues.

- [ ] **Step 3: Run full registration_visible test suite**

Run both profiles:
```bash
cargo nextest run --profile ci -p xmtp_mls -E 'test(registration_visible)'
cargo nextest run --features d14n --profile ci-d14n -p xmtp_mls -E 'test(registration_visible)'
```
Expected: All tests pass in both profiles.

- [ ] **Step 4: Restack and verify up-stack branches**

```bash
gt restack
```

Check that up-stack branches compile and their tests pass.

- [ ] **Step 5: Submit**

```bash
gt submit
```
