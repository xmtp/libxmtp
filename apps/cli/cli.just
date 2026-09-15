NIX_DEVSHELL := "js-node"

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

install:
    yarn

install-ci:
    yarn install --immutable

sdk:
    cd ../../sdks/js && yarn install --immutable
    cd ../../sdks/js && ./dev/bindings node
    cd ../../sdks/js && yarn workspace @xmtp/node-sdk run build

check: sdk
    yarn typecheck

lint:
    yarn lint

build: sdk
    yarn build

manifest: build
    yarn manifest

test: sdk
    yarn build
    {{ _env }} && yarn test

test-ci *args="": sdk
    yarn build
    {{ _env }} && yarn vitest run {{ args }}
