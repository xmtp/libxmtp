mod chaos 'apps/chaos/chaos.just'
mod backend 'apps/backend/backend.just'
mod android 'sdks/android/android.just'
mod example-android 'apps/example-android/example-android.just'
mod ios 'sdks/ios/ios.just'
mod example-ios 'apps/example-ios/example-ios.just'
mod sdk 'crates/xmtp_sdk/sdk.just'
mod wasm 'dev/wasm.just'
mod js 'sdks/js.just'
mod docs 'apps/docs/docs.just'
mod cli 'apps/cli/cli.just'
mod web-chat 'apps/web-chat/web-chat.just'

export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")

set shell := ["./dev/nix-shell"]

[script("bash")]
default:
    just --list --list-submodules

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

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

# CI overrides to "cargo llvm-cov nextest --no-fail-fast --no-report" for coverage

cargo_test := env("CARGO_TEST_CMD", "dev/agent-run cargo nextest run")

# Ports and URLs differ per worktree. dev/worktree-env writes dev/docker/.env;
# `_env` loads it so the Rust suites reach this worktree's own stack.
_env := ". " + justfile_directory() + "/dev/docker/load-env"

# Install all JavaScript workspace dependencies from the root lockfile.
install-js:
    pnpm install --frozen-lockfile

# Install the JavaScript dependencies only when a fresh checkout has none
# (the release-tools test stubs run its `tsx`).
[private]
_js-deps:
    [ -x dev/release-tools/node_modules/.bin/tsx ] || pnpm install --frozen-lockfile

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

# The CI lint suites lint_workspace, lint_js, lint_config, lint_proto,
# lint_deny, and docs_quality, the WASM and SDK generator Clippy of check_rust,
# and `spec-check`. It skips lint_ios, lint_example_ios, and the Android example
# lint (a job of test_example_android), which need mobile toolchains, and the
# cargo-deny advisories check, which does not block.
# The checks run in parallel; `just --jobs 1 lint` runs them one at a time.
[parallel]
lint: _js-deps && lint-rust wasm::lint lint-js-source lint-js-format lint-config lint-markdown lint-proto lint-deny docs::test docs::check-source spec-check

lint-proto:
    buf lint proto

# Dependency bans, licenses, and sources. CI also checks advisories; only its
# daily scheduled run fails on a new advisory.
lint-deny:
    cargo deny --locked check bans licenses sources

# Clippy for the default members and the SDK generator, which is not one.
# Format is checked once, by treefmt in `lint-config`.
lint-rust: lint-rust-source
    dev/agent-run cargo clippy --locked --all-features --all-targets --no-deps -- -Dwarnings
    dev/agent-run cargo clippy -p xmtp-sdk-bindgen --all-targets --no-deps -- -Dwarnings

# Source checks do not compile Rust or generate SDK products.
lint-rust-source:
    cargo hakari generate --diff
    cargo hakari manage-deps --dry-run

lint-js-source:
    pnpm lint:source
    node --test dev/js/test-lint-source.mjs

# Lint config files with treefmt (Rust, TOML, Nix, shell, Python, Kotlin, and
# Swift), check the CI suite declaration, and lint and type-check
# dev/release-tools. The release-tools checks run here, so release-tools edits
# do not select check_types.
lint-config: _js-deps lint-treefmt
    python3.11 -B dev/ci-suites check
    pnpm --filter @xmtp/release-tools --fail-if-no-match run typecheck
    pnpm --filter @xmtp/release-tools --fail-if-no-match run lint

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

# Run the Nix workspace tests. `--inputs-from .` pins nom to flake.lock.
nix-test:
    nix run --inputs-from . nixpkgs#nix-output-monitor -- build .#nextest.{{ nix_system }}

# `just test`, `just test crate xmtp_mls xmtp_db`, `just test dev`
[script("bash")]
test target="workspace" *args="":
    just _test-{{ target }} {{ args }}

# The dev tool tests, in parallel. dev/tests/run finds test_*.py files; add a
# test without editing this recipe. The release push test and the release-tools
# tests run the release-tools `tsx`, so `_js-deps` runs first.
[parallel]
[private]
_test-dev: _js-deps && _test-dev-python _test-dev-fly _test-dev-release-tools

[private]
_test-dev-python:
    python3.11 -B dev/tests/run

[private]
_test-dev-fly:
    node --test dev/fly/render-dashboard.test.mjs

[private]
_test-dev-release-tools:
    pnpm --filter @xmtp/release-tools --fail-if-no-match run test

[private]
_test-workspace *args="":
    {{ _env }} && SQLX_OFFLINE=true RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" {{ cargo_test }} --profile ci {{ args }}

# Workspace coverage in this worktree's environment: coverage/lcov and
# coverage/html. `--diff [BASE]` keeps only the Rust files changed since the
# merge base with BASE (default origin/main, the trunk). Other arguments go to
# `just test workspace`, for example `just coverage --diff origin/main -p xmtp_common`.
[script("bash")]
coverage *args:
    set -euo pipefail
    set -- {{ args }}
    base=""
    if [[ "${1:-}" == --diff ]]; then
      shift
      base=origin/main
      if [[ $# -gt 0 && "$1" != -* ]]; then base="$1"; shift; fi
      merge_base="$(git merge-base "$base" HEAD)" || { echo "Cannot find the merge base of $base and HEAD" >&2; exit 1; }
    fi
    cargo llvm-cov clean --profraw-only --workspace
    CARGO_TEST_CMD="cargo llvm-cov nextest --no-fail-fast --no-report" just test workspace "$@"
    rm -rf coverage
    mkdir -p coverage
    ignore='(xmtp_proto/src/gen)|(test/)|(mock/)|(mock\.rs)|(test_utils\.rs)|(test_utils/)|(tests\.rs)'
    cargo llvm-cov report --ignore-filename-regex "$ignore" --lcov --output-path coverage/lcov
    if [[ -z "$base" ]]; then
      cargo llvm-cov report --ignore-filename-regex "$ignore" --html --output-dir coverage
      echo "Coverage: coverage/lcov and coverage/html/index.html"
      exit 0
    fi
    changed=()
    while IFS= read -r file; do changed+=("*/$file"); done < <(git diff --name-only --diff-filter=ACMR "$merge_base" -- '*.rs')
    if [[ ${#changed[@]} -eq 0 ]]; then
      echo "No Rust file changed since $base"
      exit 0
    fi
    lcov --ignore-errors unused --extract coverage/lcov "${changed[@]}" --output-file coverage/diff.lcov
    genhtml --ignore-errors range coverage/diff.lcov --output-directory coverage/html
    echo "Coverage of the files changed since $base: coverage/html/index.html"

[private]
_test-crate +crates:
    {{ _env }} && args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    SQLX_OFFLINE=true RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" {{ cargo_test }} --profile ci $args

# `check`: standalone native and wasm consumer and dependency checks.
# `test`: native and wasm Node payload tests and canonical encoding vectors.
# Check and test the shared validation crate (mode check, test, or all).
validation mode="all":
    crates/xmtp_mls_validation/dev/check-validation {{ quote(mode) }}

# --- DISK ---

# Free space, and the size of target/ and of its incremental state (incremental/
# and Kache's incremental.kache-auto/) in each worktree.
[script("bash")]
disk:
    set -euo pipefail
    df -h "$(git rev-parse --git-common-dir)" | awk 'NR == 2 { print "free " $4 " of " $2 " (" $5 " used) on " $NF }'
    git worktree list --porcelain | sed -n 's/^worktree //p' | while IFS= read -r root; do
      [ -d "$root/target" ] || { printf '0\t0\t%s\n' "$root"; continue; }
      # One du pass: depth 0 is target/, the incremental dirs are at depth 2 or 3.
      du -k -d 3 "$root/target" 2>/dev/null | awk -F'\t' -v t="$root/target" -v r="$root" '
        $2 == t { total = $1 } $2 ~ /\/incremental(\.kache-auto)?$/ { inc += $1 }
        END { printf "%d\t%d\t%s\n", total, inc, r }' || true
    done | sort -rn | awk -F'\t' '
      BEGIN { print "  target  incremental  worktree" }
      { printf "%7.1fG %11.1fG  %s\n", $1 / 1048576, $2 / 1048576, $3; sum += $1 }
      END { printf "%7.1fG total target/\n", sum / 1048576 }'

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

# Annotations for a check run. Cheaper than logs when the job records them.
ci-annotations check:
    @gh api --paginate repos/xmtp/libxmtp/check-runs/{{ check }}/annotations \
      --jq '.[] | "\(.path // "-"):\(.start_line // 0)  \(.message | split("\n")[0])"'
