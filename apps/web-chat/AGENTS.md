# XMTP Web Chat

pnpm workspace package for xmtp.chat, linked to the in-tree browser SDK.

## Commands

- `dev/nix-shell 'just install-js'`: install the root workspace dependencies.
- `dev/nix-shell 'just web-chat check'`: build the linked SDK and typecheck the app.
- `dev/nix-shell 'just web-chat lint'`: run oxlint.
- `dev/nix-shell 'just web-chat build'`: build the linked SDK and app.
- `dev/nix-shell 'just web-chat test'`: build the linked SDK and run browser tests.
- `dev/nix-shell 'just web-chat dev'`: build the linked SDK and start Vite with worktree backend settings.

Tests require `dev/nix-shell 'just backend up'`.

## Backend configuration

- `XMTP_BACKEND_URL` selects the backend directly.
- Browser databases are separated with a label derived from the backend origin.

## Deployment

`.github/workflows/deploy-web-chat.yml` builds this app under Nix and deploys
`dist/` to Vercel on a push to `self-hosted`. The workflow stages the generated
browser package, including its worker, pure codecs, WASM assets, and pinned
runtime. Vercel serves the prebuilt app.

Secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID`,
`WALLETCONNECT_PROJECT_ID`. `XMTP_BACKEND_URL` is inlined at build time and is
deliberately empty in the deployed build.
