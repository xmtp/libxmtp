# XMTP Web Chat

pnpm app using the in-tree Browser SDK; recipes build that dependency first.

```sh
dev/nix-shell 'just web-chat check'
dev/nix-shell 'just web-chat lint'
dev/nix-shell 'just web-chat test' # needs backend
dev/nix-shell 'just web-chat dev'
```

`XMTP_BACKEND_URL` selects the backend; its origin separates database labels.
Deployment serves prebuilt `dist` and must require release-profile SDK receipts.
Build/deployment details: [README](README.md) and
[workflow](../../.github/workflows/deploy-web-chat.yml).
