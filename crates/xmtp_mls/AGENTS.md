# xmtp_mls

Core client. Groups, messages, sync, streams.

## Commands

```bash
just check crate xmtp_mls
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_mls                # needs `just backend up` (anvil)
just test v3 -p xmtp_mls --ignore-default-filter test_valid_deletion_by_sender   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_mls -E 'test(/messages::/)'"   # one module
```

## Gotchas

- Needs `just backend up`.
- `just test crate` = nextest `default` profile. `just test v3` = `ci` profile (CI). Both skip flaky stream tests. `--ignore-default-filter` runs them.
- `just test d14n <test_name>` = d14n lane. Builds with `--features d14n`. Separate artifacts, slow.

## Conventions

- Message ids: `src/utils/mod.rs:36 id::calculate_message_id(group_id, bytes, idempotency_key)` and `:61 calculate_message_id_for_intent(intent)` (`src/groups/mls_sync.rs:41`). Never re-derive the `group_id \t key \t payload` hash. If `apps/backend` needs it without `xmtp_mls`, move the function to a shared crate.
- Test clients: `tester!(alix)` / `tester!(bo, from: alix)` / `tester!(alix2, snapshot: snap)` (`src/utils/test/tester_utils.rs:822`, builder `:406 TesterBuilder`). Any `TesterBuilder` method works as a `key: value` or bare `key` argument. `src/utils/test/mod.rs` adds `ClientBuilder::temp_store()`, `.dev()`, `.local()`. All of it sits behind `cfg(test)` or the `test-utils` feature (`src/utils/mod.rs:8`); `apps/backend` and crates below `xmtp_mls` need their own fixture.
- A module may hold several related error enums: `src/groups/error.rs` defines `GroupError:93`, `DeleteMessageError:471`, `MetadataPermissionsError:507`, `DmValidationError:560`. Derive `ErrorCode` only when the code must be stable across the FFI boundary (`:92` derives it; `:470` does not). `RetryableError` is implemented by hand, delegating to inner errors (`:588`).
