NIX_DEVSHELL := "js"

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

bindings:
    cd ../../sdks/js && ./dev/bindings wasm

sdk: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/browser-sdk...' run build

dev: sdk
    {{ _env }} && pnpm --filter @xmtp/xmtp.chat run dev

test: sdk
    {{ _env }} && pnpm --filter @xmtp/xmtp.chat run test

check: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/xmtp.chat...' run typecheck

lint: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/xmtp.chat...' run lint

build: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/xmtp.chat...' run build
