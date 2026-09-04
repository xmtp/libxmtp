# bindings_node

NAPI-RS bindings for Node. Tests are TypeScript (`test/*.test.ts`), not Rust.

## Commands

```bash
just node check                         # install + build release NAPI to dist/
just node lint                          # prettier
dev/nix-shell 'cd bindings/node && yarn lint'   # clippy + rustfmt
just node test                          # install + build with test-utils + vitest
just node test-ci                       # what CI runs (Nix build)
dev/nix-shell 'cd bindings/node && yarn vitest run test/inboxId.test.ts'           # one file
dev/nix-shell 'cd bindings/node && yarn vitest run -t "should generate an inbox id"'   # one test
```

## Gotchas

- Needs `just backend up`.
- Tests import `../dist`. Run `just node test` once before a single-file run.
- `check` builds `--release`. `test` rebuilds with `--features test-utils`. Each switch is a full rebuild.
