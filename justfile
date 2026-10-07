mod chaos 'apps/chaos/chaos.just'
mod backend 'apps/backend/backend.just'
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod sdk 'crates/xmtp_sdk/sdk.just'
mod wasm 'dev/wasm.just'
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

# Source checks do not compile Rust or generate SDK products.
lint-rust-source:
    cargo fmt --check
    cargo hakari generate --diff
    cargo hakari manage-deps --dry-run

lint-js-source:
    pnpm lint:source

# CI restores and validates both complete SDK products before this recipe.
# Build only the handwritten JS packages whose declarations checks consume.
check-js-prepared:
    pnpm --filter @xmtp/agent-sdk exec tsdown
    pnpm --filter @xmtp/cli exec tsdown
    pnpm typecheck:prepared
    pnpm lint:prepared

# Config linting: TOML, Nix, and shell scripts.
lint-config: lint-treefmt
    python3.11 -B dev/agents/test_kache_env.py
    python3.11 dev/tests/test_android_release.py
    python3.11 dev/tests/test_release_push.py
    python3.11 dev/tests/test_android_clock.py
    python3.11 dev/tests/test_android_emulator_start.py
    python3.11 nix/lib/test-android-emulator-platform.py
    python3.11 -B dev/ci/test-selection.py
    python3.11 -B dev/ci/test-suite-gate.py
    python3.11 -B dev/ci/test-recovery-partition.py
    python3.11 -B dev/ci/test_backend_products.py
    python3.11 -B dev/ci/test-docs-reference-cache.py
    python3.11 -B dev/ci/test-nix-output-selection.py
    python3.11 -B dev/ci/check-targeted-shells.py
    python3.11 -B dev/ci/test-targeted-shells.py
    python3.11 -B dev/ci/test-prepared-sdk-lint.py
    python3.11 -B dev/ci/test-kache-diagnostics.py
    python3.11 -B dev/ci/benchmark-test.py

# Transport fixtures need Node and remain in the required SDK checks.
check-sdk-product-transport:
    python3.11 -B dev/ci/test-sdk-products.py

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

# Show compiler cache hits, misses, and store size.
cache-stats:
    kache stats

# Free space, and the target/ and incremental/ size of each worktree.
[script("bash")]
disk:
    set -euo pipefail
    df -h "$(git rev-parse --git-common-dir)" | awk 'NR == 2 { print "free " $4 " of " $2 " (" $5 " used) on " $NF }'
    git worktree list --porcelain | sed -n 's/^worktree //p' | while IFS= read -r root; do
      [ -d "$root/target" ] || { printf '0\t0\t%s\n' "$root"; continue; }
      # One du pass: depth 0 is target/, incremental/ is at depth 2 or 3.
      du -k -d 3 "$root/target" 2>/dev/null | awk -F'\t' -v t="$root/target" -v r="$root" '
        $2 == t { total = $1 } $2 ~ /\/incremental$/ { inc += $1 }
        END { printf "%d\t%d\t%s\n", total, inc, r }' || true
    done | sort -rn | awk -F'\t' '
      BEGIN { print "  target  incremental  worktree" }
      { printf "%7.1fG %11.1fG  %s\n", $1 / 1048576, $2 / 1048576, $3; sum += $1 }
      END { printf "%7.1fG total target/\n", sum / 1048576 }'

# `--minutes N` deletes each crate cache in incremental/ in which nothing
# changed for N minutes. A cache with a `-working` session (rustc is compiling
# that crate now) stays, so this is safe during builds.
# Delete stale incremental/ dirs across all worktrees (default: unused 14+ days).
[script("bash")]
[arg("minutes", long="minutes")]
clean-incremental days="14" minutes="":
    set -euo pipefail
    days={{ quote(days) }}
    minutes={{ quote(minutes) }}
    if ! [[ $days =~ ^[0-9]+$ ]] || { [ -n "$minutes" ] && ! [[ $minutes =~ ^[0-9]+$ ]]; }; then
      echo "days and --minutes take a whole number" >&2
      exit 1
    fi
    stale() {
      if [ -z "$minutes" ]; then
        find "$1/target" -maxdepth 3 -type d -name incremental -atime +"$days" 2>/dev/null
        return
      fi
      find "$1/target" -maxdepth 4 -type d -path '*/incremental/*' ! -path '*/incremental/*/*' -prune 2>/dev/null |
        while IFS= read -r crate; do
          [ -z "$(find "$crate" -maxdepth 1 -name '*-working' -print -quit)" ] || continue
          [ -z "$(find "$crate" -maxdepth 2 -mmin -"$minutes" -print -quit)" ] || continue
          echo "$crate"
        done
    }
    total=0
    while IFS= read -r root; do
      while IFS= read -r dir; do
        [ -n "$dir" ] || continue
        size=$(du -sk "$dir" | cut -f1)
        total=$((total + size))
        [ -n "$minutes" ] || echo "removing $dir ($((size / 1024)) MB)"
        rm -rf "$dir"
      done < <(stale "$root" || true)
    done < <(git worktree list --porcelain | sed -n 's/^worktree //p')
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
    esc=""; if gh api --help | grep -- --allow-escape-sequences >/dev/null; then esc="--allow-escape-sequences"; fi
    gh api $esc repos/xmtp/libxmtp/actions/jobs/{{ job }}/logs \
      | dev/ci-failure-markers

# Check failure markers with local logs. It does not use GitHub.
ci-failures-filter-test:
    python3 dev/tests/test_ci_failure_markers.py

# Annotations for a check run. Cheaper than logs when the job records them.
ci-annotations check:
    @gh api --paginate repos/xmtp/libxmtp/check-runs/{{ check }}/annotations \
      --jq '.[] | "\(.path // "-"):\(.start_line // 0)  \(.message | split("\n")[0])"'
