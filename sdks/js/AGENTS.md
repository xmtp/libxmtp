# XMTP JS SDKs

Yarn workspace: `node-sdk` (over `bindings/node`), `browser-sdk` (over `bindings/wasm`), `agent-sdk` (over `node-sdk`).

## Commands

```bash
just js install
just js bindings                        # build node + wasm bindings via Nix, stage into bindings/*/dist
just js check                           # typecheck all
just js build
just js lint                            # eslint
just js test                            # needs `just backend up` (history server)
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk run test'      # one SDK
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/browser-sdk run test'   # playwright
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk run build && yarn workspace @xmtp/agent-sdk run test'
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js/node-sdk && yarn vitest run test/createBackend.test.ts'   # one file
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js/node-sdk && yarn vitest run -t "should create a backend with local env"'   # one test
```

## Gotchas

- Needs `just backend up`. Run `just js install` and `just js bindings` once first.
- `agent-sdk` reads types from `node-sdk/dist`. Build `node-sdk` first.
- Formatting is treefmt prettier (`just lint-config`), not eslint.
