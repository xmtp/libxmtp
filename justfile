mod backend 'apps/backend/backend.just'
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'
mod js 'sdks/js/js.just'
mod docs 'apps/docs/docs.just'

export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")

set shell := ["./dev/nix-shell"]

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

# CI overrides to "cargo llvm-cov nextest --no-fail-fast --no-report" for coverage

cargo_test := env("CARGO_TEST_CMD", "cargo nextest run")

# Ports and URLs differ per worktree. dev/worktree-env writes dev/docker/.env;
# `_env` loads it so the Rust suites reach this worktree's own stack.
_env := justfile_directory() + "/dev/worktree-env && . " + justfile_directory() + "/dev/docker/load-env"

[script("bash")]
default:
    just --list --list-submodules

# --- CHECK ---

# `just check`, `just check crate xmtp_mls`, `just check crate xmtp_mls xmtp_db`
[script("bash")]
check target="workspace" *args="":
    just _check-{{ target }} {{ args }}

[private]
_check-workspace:
    cargo check --locked

[private]
_check-crate +crates:
    args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    cargo check --locked $args

# --- LINT ---

lint: lint-rust lint-config lint-markdown lint-proto

lint-proto:
    buf lint proto

lint-rust:
    cargo clippy --locked --all-features --all-targets --no-deps -- -Dwarnings
    cargo fmt --check
    cargo hakari generate --diff
    cargo hakari manage-deps --dry-run

# Config linting: TOML, Nix, shell scripts
lint-config: lint-treefmt

lint-toml:
    taplo format --check --diff
    taplo check

[script("bash")]
lint-treefmt:
    nix fmt -- --fail-on-change

# Exclude the generated error glossary and release changelogs.
lint-markdown:
    markdownlint "**/*.md" ".agents/**/*.md" --ignore "**/CLAUDE.md" --ignore "**/node_modules/**" --ignore "target/**" --ignore "**/dist/**" --ignore "**/_site/**" --ignore "apps/docs/generated/**" --ignore "apps/docs/src/content/docs/reference/*-sdk/**" --ignore "docs/error_glossary.md" --ignore "sdks/js/*/CHANGELOG.md" --disable MD001 MD013

# --- FORMAT ---

[script("bash")]
format:
    nix fmt
    just android format
    just ios format
    just node format
    just wasm format

# --- TEST ---

# Run the Nix workspace tests.
nix-test:
    nix run nixpkgs#nix-output-monitor build .#nextest.{{ nix_system }}

# `just test`, `just test crate xmtp_mls xmtp_db`
[script("bash")]
test target="workspace" *args="":
    just _test-{{ target }} {{ args }}

[private]
_test-workspace *args="":
    {{ _env }} && SQLX_OFFLINE=true {{ cargo_test }} --profile ci {{ args }}

[private]
_test-crate +crates:
    {{ _env }} && args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    SQLX_OFFLINE=true {{ cargo_test }} --profile ci $args

# Verify the shared validation crate without workspace feature unification.
check-validation:
    dev/check-validation check

# Run the shared validation crate tests on native and wasm Node.
test-validation:
    dev/check-validation test

validation: check-validation test-validation

# --- WORKTREE ---

# Show this worktree's stack identity, ports, and URLs.
worktree:
    just backend status

# --- DISK ---

# Show sccache hit rates and cache size.
cache-stats:
    sccache --show-stats

# Delete stale incremental/ dirs across all worktrees (default: unused 14+ days).
[script("bash")]
clean-incremental days="14":
    set -euo pipefail
    roots=$(git worktree list --porcelain | awk '/^worktree /{print $2}')
    total=0
    for root in $roots; do
      for dir in $(find "$root/target" -maxdepth 2 -type d -name incremental -atime +{{ days }} 2>/dev/null); do
        size=$(du -sk "$dir" | cut -f1)
        total=$((total + size))
        echo "removing $dir ($((size / 1024)) MB)"
        rm -rf "$dir"
      done
    done
    echo "reclaimed $((total / 1024)) MB"

# --- AGENT HELPERS ---
# Compact output for agents. See .agents/skills/check-ci.

# Signature outline of a source file: declarations and line numbers, no bodies.
outline file:
    @rg -n '^\s{0,8}(pub(\([a-z]+\))?\s+)?(async\s+)?(unsafe\s+)?(fn|struct|enum|impl|trait|mod|type)\b' {{ file }}

# CI status for a PR: failures first, then a one-line summary.
[script("bash")]
ci-status pr:
    set -euo pipefail
    gh pr view {{ pr }} --repo xmtp/libxmtp --json statusCheckRollup --jq '
      [.statusCheckRollup[] | select(.__typename == "CheckRun")]
      | group_by(.name) | map(max_by(.startedAt // ""))
      | (map(select(.conclusion == "FAILURE"))
         | if length > 0 then "FAILED:\n" + (map("  \(.name)  \(.detailsUrl)") | join("\n")) else "" end),
        (map(select(.status != "COMPLETED"))
         | if length > 0 then "RUNNING: \(length) job(s)" else "" end),
        ("SUMMARY: \(map(select(.conclusion == "SUCCESS")) | length) ok, \(map(select(.conclusion == "FAILURE")) | length) failed, \(map(select(.conclusion == "SKIPPED")) | length) skipped, \(map(select(.status != "COMPLETED")) | length) running")
      | select(. != "")'

# Why one job failed. Strips timestamps and ANSI, keeps failure markers only.
[script("bash")]
ci-failures job:
    set -euo pipefail
    gh api repos/xmtp/libxmtp/actions/jobs/{{ job }}/logs \
      | sed -E 's/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z //; s/\x1b\[[0-9;]*[mGKH]//g' \
      | rg -N '(^##\[error\]|^\s*FAIL |AssertionError|^thread .* panicked|^\s*assertion.*failed|^error\[E[0-9]+\]|^error: recipe .* failed|Tests\s+[0-9]+ failed|test result: FAILED)' \
      | sort -u | head -40

# Annotations for a check run. Cheaper than logs when the job records them.
ci-annotations check:
    @gh api repos/xmtp/libxmtp/check-runs/{{ check }}/annotations \
      --jq '.[] | "\(.path // "-"):\(.start_line // 0)  \(.message | split("\n")[0])"'
