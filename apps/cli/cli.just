export NIX_DEVSHELL := "js-node"

root := justfile_directory()
_env := ". " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

sdk:
    ../../dev/js/sdk-package node

check: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run typecheck

lint: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run lint

build: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run build

# Extra arguments go to Vitest, for example `--shard 1/2` (CI).
test *args: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run build
    {{ _env }} && pnpm --filter @xmtp/cli exec vitest run {{ args }}
