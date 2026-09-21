NIX_DEVSHELL := "js-node"

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

sdk:
    cd ../../sdks/js && ./dev/bindings node

check: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run typecheck

lint:
    pnpm --filter @xmtp/cli run lint

build: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run build

manifest: build
    pnpm --filter @xmtp/cli run manifest

test: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run build
    {{ _env }} && pnpm --filter @xmtp/cli run test

test-ci *args="": sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/cli...' run build
    {{ _env }} && pnpm --filter @xmtp/cli exec vitest run {{ args }}
