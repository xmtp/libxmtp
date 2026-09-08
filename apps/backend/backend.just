export NIX_DEVSHELL := env("NIX_DEVSHELL", "rust")
set shell := ["./dev/nix-shell"]
set working-directory := "../.."

root := justfile_directory()
db_url := env("DATABASE_URL", "postgres://xmtp:xmtp@localhost:55432/xmtp_backend")
musl_arch := if arch() == "aarch64" { "aarch64" } else { "x86_64" }

# Build the binary.
build:
    nix build {{ root }}#xmtp-backend

# Build the container image for this host's architecture.
image:
    nix build {{ root }}#backend-image-{{ musl_arch }}-unknown-linux-musl --out-link {{ root }}/result-backend-image

# Run the backend outside Docker against the `db-up` database.
run:
    XMTP_DATABASE_URL="{{ db_url }}" XMTP_CHAIN_31337_URL="http://localhost:8545" \
      nix run {{ root }}#xmtp-backend -- --config {{ root }}/dev/backend/local.toml

# Backend unit, RPC, storage, replica, and ingress tests. Needs `db-up`.
test *args="":
    DATABASE_URL="{{ db_url }}" RUST_TEST_THREADS="${RUST_TEST_THREADS:-4}" cargo test --locked -p xmtp_backend {{ args }}

db-up:
    docker compose -f {{ root }}/dev/backend/compose.yml up --detach --wait

db-down:
    docker compose -f {{ root }}/dev/backend/compose.yml down

sql-prepare:
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx migrate run --source apps/backend/migrations
    cd apps/backend && DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx prepare -- --all-targets

sql-check:
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx migrate run --source apps/backend/migrations
    cd apps/backend && DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx prepare --check -- --all-targets

[script("bash")]
schema:
    set -euo pipefail
    schema_tmp="$(mktemp docs/schemas/backend-v1.XXXXXX)"
    trap 'rm -f "$schema_tmp"' EXIT
    cargo run --locked --quiet -p xmtp_backend --example config_schema > "$schema_tmp"
    mv "$schema_tmp" docs/schemas/backend-v1.json

# Start the SDK test stack: db, backend (this host's image), toxiproxy, anvil.
up: image
    docker load --input {{ root }}/result-backend-image
    {{ root }}/dev/docker/up

down:
    {{ root }}/dev/docker/down
