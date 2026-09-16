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
