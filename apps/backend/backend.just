export NIX_DEVSHELL := env("NIX_DEVSHELL", "rust")
set shell := ["./dev/nix-shell"]
set working-directory := "../.."

root := justfile_directory()
musl_arch := if arch() == "aarch64" { "aarch64" } else { "x86_64" }

# Ports and URLs differ per worktree. dev/worktree-env writes dev/docker/.env;
# `_env` loads it so a recipe inherits this worktree's values. An already-set
# variable always wins, so CI and one-off overrides are untouched.
_env := root + "/dev/worktree-env && set -a && . " + root + "/dev/docker/.env && set +a"

# Build the binary.
build:
    nix build {{ root }}#xmtp-backend

# Build the container image for this host's architecture.
#
# A devshell puts a newer glibc on the library path, which breaks the system git
# that Nix starts to fetch a git dependency. Run `nix` outside every devshell.
[script('/bin/sh')]
image:
    nix build {{ root }}#backend-image-{{ musl_arch }}-unknown-linux-musl --out-link {{ root }}/result-backend-image

# Run the backend outside Docker against the `db-up` database.
[script("bash")]
run:
    {{ _env }}
    XMTP_DATABASE_URL="$DATABASE_URL" XMTP_REPLICA_URL="$XMTP_REPLICA_URL" XMTP_CHAIN_31337_URL="$ANVIL_URL" \
      nix run {{ root }}#xmtp-backend -- --config {{ root }}/dev/backend/local.toml

# Backend unit, RPC, storage, replica, and ingress tests. Needs `db-up`.
[script("bash")]
test *args="":
    {{ _env }}
    RUST_TEST_THREADS="${RUST_TEST_THREADS:-4}" cargo test --locked -p xmtp_backend {{ args }}

db-up:
    {{ root }}/dev/docker/up db replica

db-down:
    {{ root }}/dev/docker/down

[script("bash")]
sql-prepare:
    {{ _env }}
    cargo sqlx migrate run --source apps/backend/migrations
    cd apps/backend && cargo sqlx prepare -- --all-targets

[script("bash")]
sql-check:
    {{ _env }}
    cargo sqlx migrate run --source apps/backend/migrations
    cd apps/backend && cargo sqlx prepare --check -- --all-targets

[script("bash")]
schema:
    set -euo pipefail
    schema_tmp="$(mktemp docs/schemas/backend-v1.XXXXXX)"
    trap 'rm -f "$schema_tmp"' EXIT
    cargo run --locked --quiet -p xmtp_backend --example config_schema > "$schema_tmp"
    mv "$schema_tmp" docs/schemas/backend-v1.json

# Start the shared SDK and observability stack.
up *services: image
    docker load --input {{ root }}/result-backend-image
    {{ root }}/dev/docker/up {{ services }}

down:
    {{ root }}/dev/docker/down

logs *services:
    {{ root }}/dev/docker/compose logs {{ services }}

# Show this worktree's stack identity: project, slot, ports, and URLs.
[script("bash")]
status:
    {{ _env }}
    echo "worktree:  $XMTP_WORKTREE_NAME (slot $XMTP_WORKTREE_SLOT)"
    echo "project:   $XMTP_COMPOSE_PROJECT"
    echo "backend:   $XMTP_BACKEND_URL"
    echo "toxic:     $XMTP_BACKEND_TOXIC_URL"
    echo "anvil:     $ANVIL_URL"
    echo "database:  $DATABASE_URL"
    echo "replica:   $XMTP_REPLICA_URL"
    echo "grafana:   http://127.0.0.1:$XMTP_GRAFANA_PORT"
    echo "prometheus: http://127.0.0.1:$XMTP_PROMETHEUS_PORT"
    echo "tempo:     http://127.0.0.1:$XMTP_TEMPO_PORT"
    echo "metrics:   http://127.0.0.1:$XMTP_BACKEND_METRICS_PORT/metrics"
    docker compose -f {{ root }}/dev/docker/compose.yml -p "$XMTP_COMPOSE_PROJECT" ps 2>/dev/null || true

# Stop this worktree's stack and give up its port slot.
[script("bash")]
release:
    {{ root }}/dev/docker/down --volumes || true
    {{ root }}/dev/worktree-env --release

# Verify client traces, backend metrics, and the local dashboard.
observe-check:
    bash {{ root }}/dev/docker/observe-check.sh
