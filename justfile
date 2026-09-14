mod backend 'apps/backend/backend.just'
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'
mod js 'sdks/js/js.just'
mod docs 'apps/docs/docs.just'

export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")

set shell := ["./dev/nix-shell"]

# Test the agent shell wrapper and hook without starting a language server.
agent-test:
    python3 -m unittest discover -s dev/agents -p 'test_*.py' -v

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
# Rust by default; Kotlin, Swift, and TypeScript/JavaScript by extension. A file
# with no declarations prints nothing and succeeds.
[script("bash")]
outline file:
    set -euo pipefail
    case "{{ file }}" in
      *.kt|*.kts) pat='^\s{0,8}((public|private|internal|protected|open|abstract|override|suspend|data|sealed|inline|inner|companion|enum|annotation|operator|infix)\s+)*(fun|class|object|interface|typealias|constructor)\b' ;;
      *.swift) pat='^\s{0,8}((public|private|internal|fileprivate|open|static|final|override|mutating|convenience|required|indirect|nonisolated)\s+)*(func|class|struct|enum|protocol|extension|actor|init|typealias|subscript)\b' ;;
      *.ts|*.tsx|*.js|*.mjs|*.cjs) pat='^((export\s+)?(default\s+)?(declare\s+)?(abstract\s+)?(async\s+)?(function\*?|class|interface|enum|namespace)\b|(export\s+)?(declare\s+)?type\s+[A-Za-z_$][\w$]*\s*(<[^>]*>)?\s*=|(export\s+)?(declare\s+)?(const|let|var)\s+[A-Za-z_$][\w$]*|\s{2}((public|private|protected|static|readonly|abstract|override|async|get|set)\s+)*[A-Za-z_$#][\w$]*\s*(<[^>]*>)?\s*\([^;]*$)' ;;
      *) pat='^\s{0,8}(pub(\([^)]*\))?\s+)?(async\s+)?(unsafe\s+)?(fn|struct|enum|impl|trait|mod|type)\b' ;;
    esac
    # Control-flow statements at two-space indent look like TS class members; drop them.
    rg -n "$pat" "{{ file }}" | rg -v '^[0-9]+:\s+(if|for|while|switch|return|catch|throw|await|else|do|try)\b' || test $? -eq 1

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
      | { rg -N '(^##\[error\]|^\s*FAIL |AssertionError|^thread .* panicked|^\s*assertion.*failed|^error(\[[^]]+\])?:|Tests\s+[0-9]+ failed|test result: FAILED|^\s*\S+ FAILED\s*$)' || test $? -eq 1; } \
      | sort -u | sed -n '1,40p'

# Annotations for a check run. Cheaper than logs when the job records them.
ci-annotations check:
    @gh api --paginate repos/xmtp/libxmtp/check-runs/{{ check }}/annotations \
      --jq '.[] | "\(.path // "-"):\(.start_line // 0)  \(.message | split("\n")[0])"'
