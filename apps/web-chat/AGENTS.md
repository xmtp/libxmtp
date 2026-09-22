# XMTP Web Chat

pnpm workspace package for xmtp.chat, linked to the in-tree browser SDK.

## Commands

- `just install`: install the root workspace dependencies.
- `just web-chat check`: build the linked SDK and typecheck the app.
- `just web-chat lint`: run ESLint.
- `just web-chat build`: build the linked SDK and app.
- `just web-chat test`: build the linked SDK and run browser tests.
- `just web-chat dev`: build the linked SDK and start Vite with worktree backend settings.

Tests require `just backend up`.

## Backend configuration

- `XMTP_BACKEND_URL` selects the backend directly.
- Browser databases are separated with a label derived from the backend origin.

## Deployment

`.github/workflows/deploy-web-chat.yml` builds this app under Nix and deploys
`dist/` to Vercel on a push to `self-hosted`. Vercel cannot build the app
itself: the `portal:` dependency chain ends at a Nix-built Rust WASM crate.

Secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID`,
`WALLETCONNECT_PROJECT_ID`. `XMTP_BACKEND_URL` is inlined at build time and is
deliberately empty in the deployed build.
