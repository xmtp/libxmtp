# XMTP CLI

Command-line interface for working with XMTP identities, conversations,
messages, consent, and installations. The package uses the in-tree
`@xmtp/node-sdk` when developed in `xmtp/libxmtp`.

## Install

```bash
npm install --global @xmtp/cli
```

For repository development, run commands through Nix:

```bash
dev/nix-shell 'just install'
dev/nix-shell 'just cli check'
dev/nix-shell 'just cli lint'
dev/nix-shell 'just cli test'
```

Tests require the local backend:

```bash
dev/nix-shell 'just backend up'
```

## Initialize

A backend URL is required and has no default. Initialize once to save the URL
and generate wallet and database-encryption keys:

```bash
xmtp init --backend-url http://127.0.0.1:5050
```

The default output is `~/.xmtp/.env`. Use `--output`, `--stdout`, or `--force`
to change how the file is written.

## Configuration priority

1. CLI flags
2. `--env-file <path>`
3. `./.env`
4. `~/.xmtp/.env`

## Environment variables

| Variable                   | Description                                         | Required                            |
| -------------------------- | --------------------------------------------------- | ----------------------------------- |
| `XMTP_BACKEND_URL`         | Backend URL including `http://` or `https://`       | Yes for backend and client commands |
| `XMTP_API_KEY`             | Backend API key, sent as a bearer token             | Only for an authenticated backend   |
| `XMTP_WALLET_KEY`          | Ethereum private key                                | Yes for client commands             |
| `XMTP_DB_ENCRYPTION_KEY`   | 32-byte database encryption key                     | Yes for client commands             |
| `XMTP_ENV`                 | Database label only                                 | No, defaults to `local`             |
| `XMTP_DB_PATH`             | Explicit database path                              | No                                  |
| `XMTP_LOG_LEVEL`           | `off`, `error`, `warn`, `info`, `debug`, or `trace` | No                                  |
| `XMTP_STRUCTURED_LOGGING`  | Enable structured logging when `true`               | No                                  |
| `XMTP_DISABLE_DEVICE_SYNC` | Disable device sync when `true`                     | No                                  |
| `XMTP_APP_VERSION`         | Custom app version                                  | No                                  |

Without `XMTP_DB_PATH`, the CLI stores the database at
`~/.xmtp/<backend-label>/xmtp-db`. The label is derived from the backend origin,
so different backends use different databases. `XMTP_ENV` changes only the SDK
database label; it does not select a backend.

## Usage

```bash
xmtp client info
xmtp can-message 0x...
xmtp conversations create-dm 0x...
xmtp conversations list
xmtp conversation send-text <conversation-id> "Hello"
xmtp conversation messages <conversation-id>
```

Use `--json` for machine-readable output and `xmtp <command> --help` for full
command documentation.

## Links

- [libxmtp repository](https://github.com/xmtp/libxmtp)
- [Report an issue](https://github.com/xmtp/libxmtp/issues)
