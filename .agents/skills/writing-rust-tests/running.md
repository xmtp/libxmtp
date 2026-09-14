# Running tests

## Targeting specific tests

`just test` and `just wasm test` pass extra args through to `cargo nextest run`:

```bash
just test workspace test_send_message               # by name substring
just test workspace -p xmtp_mls test_send_message   # by crate and name
just test crate xmtp_mls xmtp_db                    # whole crates
just wasm test test_send_message

# A test the profile's default-filter excludes
just test workspace -p xmtp_mls --ignore-default-filter test_stream_all_messages_does_not_lose_messages
```

`just` does not preserve shell quoting in `{{ args }}`, so `-E '<expr>'` breaks
through `just test`. Use `dev/nix-shell` with a quoted string:

```bash
dev/nix-shell "cargo nextest run --profile ci -E 'test(test_send_message)'"
dev/nix-shell "cargo nextest run --profile ci -p xmtp_mls -E 'test(/groups::tests/)'"
dev/nix-shell "cargo nextest run --profile ci -E 'package(xmtp_mls)'"
```

## Direct nextest

Wrap in `dev/nix-shell '<cmd>'` so the pinned toolchain is used.

```bash
dev/nix-shell 'cargo nextest run --profile ci test_send_message'
NIX_DEVSHELL=wasm dev/nix-shell 'cargo nextest run --profile ci --cargo-profile wasm-test \
  --target wasm32-unknown-unknown -p xmtp_mls test_send_message'
```

## Filter expression syntax

```text
test(name)           # match test name
test(/regex/)        # regex on the test name
package(crate_name)  # by crate
rdeps(crate_name)    # crate plus reverse dependencies
deps(crate_name)     # crate plus dependencies
kind(lib|test|bin)   # by target kind
```

Combine with `&`, `|`, `not`: `-E 'package(xmtp_mls) & not test(/streaming/)'`.

## Profiles

Config: `.config/nextest.toml`. Both profiles share the `default-filter`, which
excludes `xmtp_backend` (run it with `just backend test`) and one flaky
streaming test.

| Profile | Used by | Notes |
| --- | --- | --- |
| `ci` | every `just test` variant | 90 s slow timeout; retries only for `test_db_migrates` |
| `default` | a bare `cargo nextest run` | 3 exponential retries |

## Logging

```bash
RUST_LOG=xmtp_mls=debug just test workspace test_name
STRUCTURED=1 just test workspace test_name       # JSON
XMTP_TEST_LOGGING=false just test workspace test_name
```

`just wasm test` sets `RUST_LOG=off` and a fixed package list.

## Backend services

Tests that create clients need the stack for this worktree:

```bash
just backend up        # build the image and start the containers
just backend status    # ports and URLs for this checkout
just backend down
```

`apps/backend` tests use `cargo test` and only need `just backend db-up`.
See [backend.md](backend.md).

## CI

Nix derivations give hermetic builds:

```bash
just nix-test          # native tests through Nix
just wasm test-ci
just node test-ci
```

## Coverage

```bash
dev/nix-shell 'dev/llvm-cov'             # full run; coverage/lcov and coverage/html
dev/nix-shell 'dev/test/diff-coverage'   # files changed vs the parent branch
```

`CARGO_TEST_CMD="cargo llvm-cov nextest --no-fail-fast --no-report" just test` also works.
