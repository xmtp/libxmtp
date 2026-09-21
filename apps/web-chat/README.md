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
dev/nix-shell 'just install'
dev/nix-shell 'just web-chat dev'
```

## Useful commands

- `just web-chat check`: Typecheck the app against the in-tree SDK.
- `just web-chat lint`: Run oxlint.
- `just web-chat build`: Create a production build.
- `just web-chat test`: Run browser tests against the worktree backend.

## Deployment

The app is deployed at <https://self-hosted.xmtp.chat>. A merge to
`self-hosted` deploys it with `.github/workflows/deploy-web-chat.yml`. Pull
requests do not deploy.

The build runs in GitHub Actions under Nix, not on Vercel. The app resolves
`@xmtp/browser-sdk` through a `portal:` dependency, and that SDK resolves
`@xmtp/wasm-bindings` through another one, whose `dist/` comes from a Nix build
of a Rust crate. The Vercel build container has neither Nix nor a Rust
toolchain, so the workflow builds `dist/` and uploads it with
`vercel deploy --prebuilt`. Vercel serves static files only.

The deployed app has no default backend URL. Each user enters one in the
settings panel. `XMTP_BACKEND_URL` and `VITE_PROJECT_ID` are inlined by Vite at
build time, so values set in the Vercel dashboard have no effect; change them in
the workflow instead. Both must be set on the deploy step as well as the build
step, because `vercel build` re-runs Vite.

The workflow needs these repository secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`,
`VERCEL_PROJECT_ID`, and `WALLETCONNECT_PROJECT_ID`.
