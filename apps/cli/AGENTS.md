# XMTP CLI

pnpm package using the in-tree generated Node SDK. Recipes stage it first.

The CLI shares the root Cargo target with the SDKs. Release through
`release.yml` with `cli=true`, which also selects the matching Node SDK.

```sh
dev/nix-shell 'just cli check'
dev/nix-shell 'just cli lint'
dev/nix-shell 'just cli test [vitest args]' # needs backend
```

- Register stable colon-separated command IDs in `src/commands.ts`; Oclif reads
  the exported table and `CustomHelp` when creating its manifest.
- Require `--backend-url` or `XMTP_BACKEND_URL` for network/client commands.
  `env` labels databases only. Backend origin sets default storage; explicit paths win.
- Tokens use `XMTP_API_KEY`, never argv. User commands: [README](README.md).
