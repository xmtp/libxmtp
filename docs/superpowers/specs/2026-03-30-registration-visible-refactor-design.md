# Registration Visible Refactor

**Date:** 2026-03-30
**File:** `crates/xmtp_mls/src/registration_visible.rs`

## Goals

1. Move tests to a separate file
2. Add integration tests with full d14n registration flow
3. Replace manual polling loop with `retry_async!` macro
4. Always perform `is_ready` check before d14n-specific node polling
5. Break `wait_for_registration_visible` into smaller functions

## Design

### 1. Move tests to separate file

Convert `registration_visible.rs` to a directory module:
- `src/registration_visible/mod.rs` — production code (current `registration_visible.rs` minus `#[cfg(test)]` block)
- `src/registration_visible/tests.rs` — all tests (existing unit tests + new integration test)

The `mod.rs` file declares `#[cfg(test)] mod tests;`.

### 2. Integration test

Add a test using the `tester!` macro for a full registration flow:

```rust
#[xmtp_common::test(unwrap_try = true)]
async fn test_wait_for_registration_visible_after_registration() {
    tester!(alice);
    alice.client
        .wait_for_registration_visible(VisibilityConfirmationOptions::default())
        .await
        .unwrap();
}
```

This validates: registered identity -> stored cursor -> d14n node query -> quorum confirmation.

### 3. Replace manual loop with `retry_async!`

**Current:** `check_node_visibility` has a hand-rolled `loop` with `sleep` and `Instant::now()` timeout tracking.

**New:**
- Add a new `ClientError` variant: `EnvelopesNotYetVisible` — marked as retryable via `RetryableError` impl.
- `check_node_visibility` becomes a single-attempt function that returns:
  - `Ok(())` when both envelopes are visible
  - `Err(ClientError::EnvelopesNotYetVisible)` when not yet visible (retryable)
  - `Err(ClientError::RegistrationNotVisible { .. })` on hard failures (not retryable)
- The caller wraps it with `retry_async!` using a configured `Retry<ExponentialBackoff>`:
  - `ExponentialBackoff::builder().total_wait_max(timeout).build()`
  - Default `Retry` retries (5) increased to allow sufficient polling time within the timeout window
- On final retry exhaustion, map the error to `ClientError::RegistrationNotVisible { failed_nodes: vec![node_id] }`.

**`VisibilityConfirmationOptions` changes:**
- Remove `sleep_interval_ms` field (exponential backoff handles timing)
- Remove `sleep_interval_ms` parameter from `from_parts()`
- Keep `quorum` and `timeout_ms`

### 4. Always call `is_ready` first

**Current flow:** V3 clients check `is_ready`. D14n clients skip `is_ready` and go to node polling.

**New flow:**
```
check_is_ready()?;              // always runs, all networks
if !is_d14n { return Ok(()) }   // v3 done here
// d14n: additionally poll nodes
```

D14n clients get `is_ready` check **plus** node visibility confirmation.

### 5. Extract helper functions

Extract from `wait_for_registration_visible`:

- **`load_registration_cursor(&self) -> Result<Option<Cursor>, ClientError>`**
  Fetches `StoredIdentity` from DB, extracts `Cursor` from `registration_cursor_originator_id` / `registration_cursor_sequence_id` fields. Returns `None` if either field is missing.

- **`poll_node_quorum(&self, cursor: Cursor, options: &VisibilityConfirmationOptions) -> Result<(), ClientError>`**
  Gets node clients via `self.context.api().get_node_clients()`. Computes required count from quorum config. Spawns `FuturesUnordered` of per-node `check_node_visibility` calls wrapped in `retry_async!`. Collects results with early-exit on quorum reached or quorum impossible.

- **`check_is_ready(&self) -> Result<(), ClientError>`**
  Wraps the `self.identity().is_ready()` check, returning `ClientError::RegistrationNotVisible` if not ready.

**Resulting `wait_for_registration_visible`:**
```rust
pub async fn wait_for_registration_visible(&self, options: VisibilityConfirmationOptions) -> Result<(), ClientError> {
    self.check_is_ready()?;

    let is_d14n = self.context.api().is_d14n()
        .map_err(|e| ClientError::Api(xmtp_api::dyn_err(e)))?;
    if !is_d14n { return Ok(()); }

    let Some(cursor) = self.load_registration_cursor()? else {
        tracing::warn!("d14n client has no registration cursor; skipping node visibility check");
        return Ok(());
    };

    self.poll_node_quorum(cursor, &options).await
}
```

No new clones introduced. Existing clones in `poll_node_quorum` remain (required for moving into async closures).

## Verification

1. All existing unit tests pass after migration to new file
2. New integration test passes with `just test d14n` (requires local node running)
3. `just check` succeeds (workspace compiles)
4. Up-stack branches are healthy after restacking
5. `gt submit` to push changes
