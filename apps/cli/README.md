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
dev/nix-shell 'just install-js'
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

Without `XMTP_DB_PATH`, the CLI stores the `local` database at
`~/.xmtp/<backend-label>/xmtp-db`. A new database with another `XMTP_ENV`
label uses `~/.xmtp/<backend-label>/environments/<environment-label>/xmtp-db`.
If the earlier path already has a database, the CLI keeps using it. If both
paths have databases, set `XMTP_DB_PATH` to select one. The backend label comes
from the backend origin. `XMTP_ENV` does not select a backend. An explicit
`XMTP_DB_PATH` stays unchanged.

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

## Handles

A handle is a public name, such as `nick` or `nick#2341`, that people on a
backend use to find an inbox. Handles are public and permanent. An inbox can
change its handle once every 7 days, up to 64 times in total. Its earlier
handles stay with it and still find it.

```bash
xmtp client handle-available nick           # free handles, the bare name first
xmtp client handle-claim nick#2341          # claim one, or switch back to an old one
xmtp client inbox-id -i nick#2341 -k handle # the inbox that holds a handle
xmtp conversations create-dm nick#2341 --identifier-kind handle
```

The backend decides each claim. `handle-claim` prints a plain message and
exits with a nonzero code when the handle is taken, not allowed on the
backend, or claimed less than 7 days after the last change, or when the inbox
has used all 64 changes. It never claims a different handle. Run
`handle-available` again to pick another one. It also refuses the inbox's
current handle before it asks the backend: that claim changes nothing, but it
counts as a change. If the connection fails during a claim, or the backend
refuses it as too soon or over the limit, the command reads the inbox from the
backend, and reports success when the handle is now current.

Handle commands check the handle grammar before they ask the backend, and
refuse other text with a plain message. Error output shows each control
character in user input as a visible `\uXXXX` escape.

Commands that take an identity accept a handle with `--identifier-kind handle`
(`--kind handle` for `client inbox-id`): `can-message`, `conversations
create-dm`, `create-group`, and `get-dm`, and `conversation add-members` and
`remove-members`. JSON output of inbox states and members has `handle`, the
current handle or `null`, and `otherHandles`. The `conversation members` table
shows the current handle only.

The `conversation messages` command accepts named standard content types with
`--content-type` and `--exclude-content-type`. The old `custom` wildcard is
removed. The CLI rejects `--content-type custom` and
`--exclude-content-type custom` instead of returning an unfiltered list.

The version 8 `preferences stream` command reports an HMAC invalidation as
`{"type":"HmacKeyUpdate","keys":{"<group-id>":[{"key":"<hex>","epoch":"<integer>"}]}}`
inside `updates`. The `keys` object is a current snapshot by conversation ID.
Each key is lowercase hex. JSON epochs are decimal strings. This replaces the
old per-update `key` field because the SDK event contains no key payload.

## Links

- [libxmtp repository](https://github.com/xmtp/libxmtp)
- [Report an issue](https://github.com/xmtp/libxmtp/issues)
