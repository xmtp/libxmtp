# XMTP Web Chat

Standalone Yarn 4 project for xmtp.chat, linked to the in-tree browser SDK with
`portal:../../sdks/js/browser-sdk`.

## Commands

- `just web-chat install`: install dependencies and update the lockfile.
- `just web-chat install-ci`: install with the immutable lockfile.
- `just web-chat check`: build the linked SDK and typecheck the app.
- `just web-chat lint`: run ESLint.
- `just web-chat build`: build the linked SDK and app.
- `just web-chat test`: build the linked SDK and run browser tests.
- `just web-chat dev`: build the linked SDK and start Vite with worktree backend settings.

Run every command through `dev/nix-shell`, for example
`dev/nix-shell 'just web-chat test'`. Tests require `just backend up`.

## Backend configuration

- `XMTP_BACKEND_URL` selects the backend directly.
- Browser databases are separated with a label derived from the backend origin.
