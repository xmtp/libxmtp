# XMTP JS SDKs

Yarn workspace: `node-sdk` (over `bindings/node`), `browser-sdk` (over `bindings/wasm`), `agent-sdk` (over `node-sdk`).

## Commands

```bash
just js install
just js bindings                        # build node + wasm bindings via Nix, stage into bindings/*/dist
just js bindings-node                    # build only Node bindings via Nix
just js install-node-ci                  # install only Node and agent workspaces
just js check-node                       # typecheck Node and agent SDKs
just js lint-node                        # lint Node and agent SDKs
just js build-node                       # build Node and agent SDKs
just js check                           # typecheck all
just js build
just js lint                            # eslint
just js test                            # needs `just backend up`
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk run test'      # one SDK
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/browser-sdk run test'   # playwright
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk run build && yarn workspace @xmtp/agent-sdk run test'
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js/node-sdk && yarn vitest run test/createBackend.test.ts'   # one file
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js/node-sdk && yarn vitest run -t "should create a backend with an explicit URL"'   # one test
```

## Gotchas

- Export `XMTP_BACKEND_URL=http://127.0.0.1:5050`. Tests require this URL.
- Needs `just backend up`. Run `just js install` and `just js bindings` once first for full local SDK work.
- Node and agent CI uses `NIX_DEVSHELL=js-node`, `just js install-node-ci`, and `just js bindings-node`.
- Verify dependency changes with the focused CI install. It omits root development tools; declare required tools in the selected workspace and run them with `yarn workspace <name> exec`.
- `agent-sdk` reads types from `node-sdk/dist`. Build `node-sdk` first.
- Formatting is treefmt prettier (`just lint-config`), not eslint.
