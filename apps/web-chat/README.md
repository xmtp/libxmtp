# xmtp.chat app

Use this React app to connect to and inspect an XMTP backend in a browser.

The app is built using the in-tree XMTP browser SDK, React, Mantine, and wagmi.

## Run xmtp.chat locally

### Run a local backend

Start a local backend from the repository root:

`dev/nix-shell 'just backend up'`

### Setup environment

The development recipe loads the worktree backend URL automatically. Set
`VITE_PROJECT_ID` in `.env` to enable WalletConnect.

### Start the app

```bash
dev/nix-shell 'just web-chat install'
dev/nix-shell 'just web-chat dev'
```

## Useful commands

- `just web-chat check`: Typecheck the app against the in-tree SDK.
- `just web-chat lint`: Run ESLint.
- `just web-chat build`: Create a production build.
- `just web-chat test`: Run browser tests against the worktree backend.

## Deployment

A merge to `self-hosted` deploys this app to Vercel with
`.github/workflows/deploy-web-chat.yml`. Pull requests do not deploy.

The build runs in GitHub Actions under Nix, not on Vercel. The app resolves
`@xmtp/browser-sdk` through a `portal:` dependency, and that SDK resolves
`@xmtp/wasm-bindings` through another one, whose `dist/` comes from a Nix build
of a Rust crate. The Vercel build container has neither Nix nor a Rust
toolchain, so the workflow builds `dist/` and uploads it with
`vercel deploy --prebuilt`. Vercel serves static files only.

The deployed app has no default backend URL. Each user enters one in the
settings panel. `XMTP_BACKEND_URL` is inlined at build time, so a value set in
the Vercel dashboard has no effect; change it in the workflow instead.

The workflow needs these repository secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`,
`VERCEL_PROJECT_ID`, and `WALLETCONNECT_PROJECT_ID`.
