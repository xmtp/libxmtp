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
dev/nix-shell 'just install-js'
dev/nix-shell 'just web-chat dev'
```

## Useful commands

- `dev/nix-shell 'just web-chat check'`: Typecheck the app against the in-tree SDK.
- `dev/nix-shell 'just web-chat lint'`: Run oxlint.
- `dev/nix-shell 'just web-chat build'`: Create a production build.
- `dev/nix-shell 'just web-chat test'`: Run browser tests against the worktree backend.

## Deployment

The app is deployed at <https://self-hosted.xmtp.chat>. A merge to
`self-hosted` deploys it with `.github/workflows/deploy-web-chat.yml`. Pull
requests do not deploy.

GitHub Actions generates and stages `@xmtp/browser-sdk` under Nix. The stage
contains the worker, pure codecs, WASM assets, and pinned runtime. The app
uses the workspace package. The workflow builds `dist/` and uploads it with
`vercel deploy --prebuilt`. Vercel serves these static files.

The deployed app has no default backend URL. Each user enters one in the
settings panel. `XMTP_BACKEND_URL` and `VITE_PROJECT_ID` are inlined by Vite at
build time, so values set in the Vercel dashboard have no effect; change them in
the workflow instead. Both must be set on the deploy step as well as the build
step, because `vercel build` re-runs Vite.

The workflow needs these repository secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`,
`VERCEL_PROJECT_ID`, and `WALLETCONNECT_PROJECT_ID`.
