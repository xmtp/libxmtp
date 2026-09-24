mod chaos 'apps/chaos/chaos.just'
mod backend 'apps/backend/backend.just'
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'
mod js 'sdks/js.just'
mod docs 'apps/docs/docs.just'
mod cli 'apps/cli/cli.just'
mod web-chat 'apps/web-chat/web-chat.just'

export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")

set shell := ["./dev/nix-shell"]

# Test agent helpers in their locked environment without starting a language server.
agent-test: spec-test
    UV_PROJECT_ENVIRONMENT="{{ justfile_directory() }}/.cache/agents/venv" UV_PYTHON_DOWNLOADS=never uv run --frozen --no-dev --project dev/agents --python "$(command -v python3.11)" python -m unittest discover -s dev/agents -p 'test_*.py' -v

# --- SPECS ---
# See docs/specs/SPEC-spec-format.md and .agents/skills/authoring-specs.

# Validate docs/specs/ and the implements:/verifies: links in the tree.
spec-check *args:
    python3.11 dev/specs/check.py check {{ args }}

# Print every requirement with its evidence. Pass --json for tooling.
spec-index *args:
    python3.11 dev/specs/check.py index {{ args }}

# Print one requirement with its links, for example `just spec-show JOIN-012`.
spec-show id:
    python3.11 dev/specs/check.py show {{ id }}

# The checker's own tests. Stdlib only, so no uv project.
[script("bash")]
spec-test:
    cd dev/specs && python3.11 -m unittest discover -p 'test_*.py'

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

# CI overrides to "cargo llvm-cov nextest --no-fail-fast --no-report" for coverage

cargo_test := env("CARGO_TEST_CMD", "dev/agent-run cargo nextest run")

# Ports and URLs differ per worktree. dev/worktree-env writes dev/docker/.env;
# `_env` loads it so the Rust suites reach this worktree's own stack.
_env := justfile_directory() + "/dev/worktree-env && . " + justfile_directory() + "/dev/docker/load-env"

[script("bash")]
default:
    just --list --list-submodules

# Install all JavaScript workspace dependencies from the root lockfile.
install-js:
    pnpm install --frozen-lockfile

# --- CHECK ---

# `just check`, `just check crate xmtp_mls`, `just check crate xmtp_mls xmtp_db`
[script("bash")]
check target="workspace" *args="":
    just _check-{{ target }} {{ args }}

[private]
_check-workspace:
    dev/agent-run cargo check --locked

[private]
_check-crate +crates:
    args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    dev/agent-run cargo check --locked $args

# --- LINT ---

lint: lint-rust lint-config lint-markdown lint-proto spec-check

lint-proto:
    buf lint proto

lint-rust:
    dev/agent-run cargo clippy --locked --all-features --all-targets --no-deps -- -Dwarnings
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
    markdownlint "**/*.md" ".agents/**/*.md" --ignore "**/CLAUDE.md" --ignore "**/node_modules/**" --ignore "target/**" --ignore "**/dist/**" --ignore "**/_site/**" --ignore "apps/docs/generated/**" --ignore "apps/docs/src/content/docs/reference/*-sdk/**" --ignore "docs/error_glossary.md" --ignore "apps/cli/CHANGELOG.md" --ignore "sdks/{node,browser,agent}/CHANGELOG.md" --disable MD001 MD013

# --- FORMAT ---

[script("bash")]
format:
    nix fmt
    just format-js
    just android format
    just ios format

# Format the root JavaScript files and every package through the pnpm task graph.
format-js:
    pnpm format

# Check JavaScript formatting without changing files.
lint-js-format:
    pnpm format:check

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
    {{ _env }} && SQLX_OFFLINE=true RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" {{ cargo_test }} --profile ci {{ args }}

[private]
_test-crate +crates:
    {{ _env }} && args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    SQLX_OFFLINE=true RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" {{ cargo_test }} --profile ci $args

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

# Declarations and line ranges. Arguments are passed without shell expansion.
[script("bash")]
[positional-arguments]
outline +paths:
    exec dev/ast-outline "$@"

# Read one or more symbol bodies by name.
[script("bash")]
[positional-arguments]
show file +symbols:
    exec dev/ast-outline show "$@"

# CI status for a PR: failures first, then a one-line summary.
[script("bash")]
ci-status pr:
    set -euo pipefail
    # Both rollup types count. A CheckRun that is not COMPLETED is running; any
    # terminal conclusion other than SUCCESS, SKIPPED, or NEUTRAL is a failure.
    # A StatusContext has a state instead: PENDING/EXPECTED running, SUCCESS ok,
    # anything else (FAILURE, ERROR) a failure.
    gh pr view {{ pr }} --repo xmtp/libxmtp --json statusCheckRollup --jq '
      [.statusCheckRollup[]
       | if .__typename == "CheckRun" then
           { name, url: .detailsUrl, at: (.startedAt // ""),
             state: (if .status != "COMPLETED" then "running"
                     elif .conclusion == "SUCCESS" then "ok"
                     elif (.conclusion == "SKIPPED" or .conclusion == "NEUTRAL") then "skipped"
                     else "failed" end) }
         else
           { name: .context, url: .targetUrl, at: (.startedAt // .createdAt // ""),
             state: (if (.state == "PENDING" or .state == "EXPECTED") then "running"
                     elif .state == "SUCCESS" then "ok"
                     else "failed" end) }
         end]
      | group_by(.name) | map(max_by(.at))
      | (map(select(.state == "failed"))
         | if length > 0 then "FAILED:\n" + (map("  \(.name)  \(.url)") | join("\n")) else "" end),
        (map(select(.state == "running"))
         | if length > 0 then "RUNNING: \(length) job(s)" else "" end),
        ("SUMMARY: \(map(select(.state == "ok")) | length) ok, \(map(select(.state == "failed")) | length) failed, \(map(select(.state == "skipped")) | length) skipped, \(map(select(.state == "running")) | length) running")
      | select(. != "")'

# Why one job failed. Strips timestamps and ANSI, keeps failure markers only.
[script("bash")]
ci-failures job:
    set -euo pipefail
    # gh 2.97+ refuses to print a log that contains ANSI escapes (cargo colour
    # output) unless asked. Older gh has no such flag.
    esc=""; gh api --help | grep -q -- --allow-escape-sequences && esc="--allow-escape-sequences"
    gh api $esc repos/xmtp/libxmtp/actions/jobs/{{ job }}/logs \
      | sed -E 's/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z //; s/\x1b\[[0-9;]*[mGKH]//g' \
      | { rg -N '(^##\[error\]|^\s*FAIL |AssertionError|^thread .* panicked|^\s*assertion.*failed|^error(\[[^]]+\])?:|Tests\s+[0-9]+ failed|test result: FAILED|^\s*\S+ FAILED\s*$|^/.*\.(c|m)?[jt]sx?$|^\s*[0-9]+:[0-9]+\s+(error|warning)\s|[×✖]\s|\.(c|m)?[jt]sx?:[0-9]+:[0-9]+|ERR_PNPM|[Ee]rror:|Failed to load|^\[warn\] |Code style issues found)' || test $? -eq 1; } \
      | sort -u | sed -n '1,40p'

# Annotations for a check run. Cheaper than logs when the job records them.
ci-annotations check:
    @gh api --paginate repos/xmtp/libxmtp/check-runs/{{ check }}/annotations \
      --jq '.[] | "\(.path // "-"):\(.start_line // 0)  \(.message | split("\n")[0])"'
