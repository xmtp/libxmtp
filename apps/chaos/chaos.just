export NIX_DEVSHELL := env("NIX_DEVSHELL", "rust")
set shell := ["./dev/nix-shell"]
set working-directory := "../.."

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"
_binary := '"${CARGO_TARGET_DIR:-target}/debug/xchaos"'

# Build the local chaos supervisor and its child process command.
@build:
    SQLX_OFFLINE=true {{ root }}/dev/agent-run cargo build --locked -p xchaos --quiet

# Test only the chaos app. A retry must not hide a failure.
@test *args="":
    {{ _env }} && SQLX_OFFLINE=true XMTP_NO_PANIC_ON_DB_LOCK=1 RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" {{ root }}/dev/agent-run cargo nextest run --locked -p xchaos --profile ci --ignore-default-filter --retries 0 {{ args }}

# Run 50 rounds without faults. Use --rounds N for a small local check.
@baseline *args="": build
    {{ _env }} && XMTP_NO_PANIC_ON_DB_LOCK=1 {{ _binary }} baseline {{ args }}

# Run a bounded schedule with a selected fault set.
@run *args="": build
    {{ _env }} && XMTP_NO_PANIC_ON_DB_LOCK=1 {{ _binary }} run {{ args }}

# Run a soak. Pass --hours N; the default is eight hours.
@soak *args="": build
    {{ _env }} && XMTP_NO_PANIC_ON_DB_LOCK=1 {{ _binary }} soak {{ args }}

# Print bounded status. Use --directory PATH to select a run.
@status *args="": build
    {{ _env }} && {{ _binary }} status {{ args }}

# Inspect the newest bundle, or pass its path and --group or --db.
@inspect *args="": build
    {{ _env }} && {{ _binary }} inspect {{ args }}
