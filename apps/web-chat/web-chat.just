NIX_DEVSHELL := "js"

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

sdk:
    cd {{ root }} && pnpm -r --filter '@xmtp/browser-sdk...' run build

dev: sdk
    {{ _env }} && pnpm --filter @xmtp/xmtp.chat run dev

test: sdk
    {{ _env }} && pnpm --filter @xmtp/xmtp.chat run test

check: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/xmtp.chat...' run typecheck

lint: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/xmtp.chat...' run lint

build: sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/xmtp.chat...' run build
