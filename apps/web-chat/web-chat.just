NIX_DEVSHELL := "js"

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

install:
    yarn

install-ci:
    yarn install --immutable

sdk:
    cd ../../sdks/js && yarn install --immutable
    cd ../../sdks/js && ./dev/bindings wasm
    cd ../../sdks/js && yarn workspace @xmtp/browser-sdk run build

dev: sdk
    {{ _env }} && yarn dev

test: sdk
    {{ _env }} && yarn test

check: sdk
    yarn typecheck

lint:
    yarn lint

build: sdk
    yarn build
