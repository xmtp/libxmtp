# Running Tests

## Targeting Specific Tests

All `just test` and `just wasm test` variants pass extra args through to `cargo nextest run`:

```bash
# By test name substring
just test v3 test_send_message
just test d14n test_send_message
just wasm test-v3 test_send_message

# By crate + test name
just test v3 -p xmtp_mls test_send_message

# Run a test the profile's default-filter excludes (see Profiles)
just test v3 -p xmtp_mls --ignore-default-filter test_can_stream_group_messages_for_updates
```

`just` does not preserve shell quoting in `{{ args }}`, so `-E '<expr>'` breaks
through `just test`. Use `dev/nix-shell` with a quoted string instead:

```bash
# By nextest filter expression
dev/nix-shell "cargo nextest run --profile ci -E 'test(test_send_message)'"

# By module path pattern
dev/nix-shell "cargo nextest run --profile ci -p xmtp_mls -E 'test(/groups::tests/)'"

# By package filter
dev/nix-shell "cargo nextest run --profile ci -E 'package(xmtp_mls)'"
```

**Note:** `just test d14n` already scopes to `xmtp_mls` and its reverse deps via `-E 'package(xmtp_mls)' -E 'rdeps(xmtp_mls)'`. Any additional filters you pass are combined with this scope (both conditions must match).

## Direct cargo nextest (when you need full control)

Wrap in `dev/nix-shell '<cmd>'` so the pinned toolchain is used.

```bash
# V3 with specific test
dev/nix-shell 'cargo nextest run --profile ci test_send_message'

# d14n with specific test
dev/nix-shell 'cargo nextest run --features d14n --profile ci-d14n test_send_message'

# WASM with specific test
NIX_DEVSHELL=wasm dev/nix-shell 'cargo nextest run --profile ci --cargo-profile wasm-test \
  --target wasm32-unknown-unknown -p xmtp_mls test_send_message'

# Combine filters
dev/nix-shell "cargo nextest run -p xmtp_mls -E 'test(/groups/)' --profile ci"
```

## Nextest Filter Expression Syntax

```text
test(name)           # Match test name
test(/regex/)        # Match test name by regex
package(crate_name)  # Match by crate
rdeps(crate_name)    # Crate + all reverse dependencies
deps(crate_name)     # Crate + all dependencies
kind(lib|test|bin)   # Match by target kind
```

Combine with `&` (and), `|` (or), `not`:

```text
-E 'package(xmtp_mls) & test(/groups/)'
-E 'not test(/streaming/)'
```

## Profiles

| Profile | Usage | Notes |
| ------- | ----- | ----- |
| `ci` | V3 tests | Skips flaky streaming tests, 90s slow timeout |
| `ci-d14n` | d14n tests | Skips commit_log tests |
| `default` | Local dev | 3x exponential retries |

Config: `.config/nextest.toml`

## Test Logging

```bash
RUST_LOG=xmtp_mls=debug just test v3 test_name       # Filter by crate
CONTEXTUAL=1 just test v3 test_name                    # Tree-format logs
STRUCTURED=1 just test v3 test_name                    # JSON logs
```

## Backend Services

Tests creating clients or exchanging messages require the local XMTP node:

```bash
just backend up      # Build validation service + start Docker containers
just backend down    # Stop containers
```

## CI

CI uses Nix derivations for hermetic test builds:

```bash
just nix-test                                          # Build + run v3 and d14n via Nix
just wasm test-ci                                      # WASM tests via Nix
just node test-ci                                      # Node tests via Nix
```

## Coverage

```bash
dev/nix-shell 'dev/llvm-cov'           # full run, writes coverage/lcov and coverage/ html
dev/nix-shell 'dev/test/diff-coverage' # only files changed vs the parent branch, opens html
```

`CARGO_TEST_CMD="cargo llvm-cov nextest --no-fail-fast --no-report" just test` also works.
