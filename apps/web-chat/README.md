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

## Interactive QA without a wallet

Use the local backend and app commands above. Nix supplies the pinned build
tools; Docker must be running. The first app launch also builds the Browser SDK
and its WASM assets. Open the local URL printed by Vite, not the deployed site.
Run `dev/nix-shell 'just backend status'` to confirm the worktree's backend URL.

1. Open a fresh browser profile. Choose the ephemeral wallet option rather than
   an external wallet. It creates a disposable local signing identity; no
   WalletConnect project ID, browser wallet extension, or funded account is needed.
   Dismiss the disclaimer with **I understand**.
2. In **Connect to XMTP**, verify the backend URL matches `backend status` and
   click **Connect**. The repository's local backend has authentication disabled,
   so leave the access token empty. Do not use production keys for this check.
3. Create a group from the conversations menu. Give it a recognizable QA name,
   leave its default permissions, and click **Create**. An empty member list
   creates a group containing the current identity.
4. Type a distinctive test message in **Type a message...** and click **Send**.
   Confirm it appears. Reload, select the conversation in the sidebar, and
   confirm the message remains without an application error.

For a two-person round trip, connect a second independent browser profile to the
same backend with another ephemeral wallet. From the first profile, create a
direct message with the second identity's address or inbox ID, or open
`/dm/<address>` from inside the connected app. Send a message, confirm
receipt in the second profile, and reply. Two tabs in one profile share identity
and storage, so they are not independent peers.

The [iOS](../example-ios/README.md), [Android](../example-android/README.md),
and [React Native](../example-react-native/README.md) examples can also serve as
the second peer. Follow their setup guides, select the same backend, and use
only generated local test accounts.

For browser automation on macOS, install the pinned Chromium once with
`dev/nix-shell 'pnpm --filter @xmtp/xmtp.chat exec playwright install chromium'`.
Capture a screenshot and verify the received message, not only that the page
loads. Close the disposable browser profiles and stop Vite with Ctrl-C afterward.
Stop this worktree's services with `dev/nix-shell 'just backend down'` only when
no other QA client is using them.

## Useful commands

- `dev/nix-shell 'just web-chat check'`: Typecheck the app against the in-tree SDK.
- `dev/nix-shell 'just web-chat lint'`: Run oxlint.
- `dev/nix-shell 'just web-chat build'`: Create a production build.
- `dev/nix-shell 'just web-chat test'`: Run browser tests against the worktree backend.

## Deployment

The app is deployed at <https://self-hosted.xmtp.chat>. A merge to
`main` deploys it with `.github/workflows/deploy-web-chat.yml`. Pull
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
