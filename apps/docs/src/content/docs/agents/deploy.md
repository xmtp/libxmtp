---
title: Deploy an agent
---

Deploy the agent on a Node.js host that supports environment variables and persistent storage.

## Configuration

| Variable                 | Purpose                                                        |
| ------------------------ | -------------------------------------------------------------- |
| `XMTP_BACKEND_URL`       | Self-hosted backend URL, including scheme                      |
| `XMTP_WALLET_KEY`        | Agent wallet key in `0x` hex format                            |
| `XMTP_DB_ENCRYPTION_KEY` | Optional 32-byte local database encryption key                 |
| `XMTP_DB_DIRECTORY`      | Optional persistent root for databases and attachments         |
| `XMTP_ENV`               | Optional storage directory label; it does not select a backend |

`Agent.createFromEnv()` creates a missing `XMTP_DB_DIRECTORY` with owner-only permissions where supported and stores the database below it. Mount this directory on a persistent volume. If this directory contains a legacy `xmtp-<inbox-id>.db3` file for the signer, the Agent opens it in place. Without `XMTP_DB_DIRECTORY`, it checks the working directory for a legacy `xmtp-<env>-<inbox-id>.db3` file for the signer. Set `XMTP_ENV` to select the storage label. Files for other inboxes do not block startup. If several files match the signer, set an explicit `storage.location` with `dbPath` and `attachmentsDir`.

Without a legacy database, the storage root is `XMTP_DB_DIRECTORY` or `./xmtp`. A non-empty `XMTP_ENV` adds one directory below that root. Each inbox then uses `{root}/{label}/{deployment}/{inbox-id}/xmtp.db3`, with the label omitted when it is empty. The deployment directory includes a sanitized deployment identifier and its hash. Attachments are in the `attachments` directory next to the database.

Persist the whole storage root. It contains the database, SQLite sidecar files, attachments, and the backend deployment record. Use a consistent database backup or stop the client before you copy live SQLite files. Keep the same encryption key when you open an encrypted database again. If you omit the key, the SDK creates an unencrypted database.

For a host that supplies a persistent volume path, pass that path as a storage directory:

```ts source="agents-deploy-1.ts" region="example1"

```

Caller-supplied `storage` takes precedence over `XMTP_DB_DIRECTORY` and the legacy database search. If `storage.encryptionKey` is absent, `createFromEnv()` still reads `XMTP_DB_ENCRYPTION_KEY`. `XMTP_BACKEND_URL`, when set, overrides `options.backend.url`.

## Security

For an agent to function—whether it's answering questions, executing commands, or providing automated responses—it must be able to read the conversation to understand what's being asked, and write messages to respond.

Like any other user, this means your agent holds the cryptographic keys required to decrypt and send messages in the conversation. As an agent developer, it's important to uphold the security of these keys and messages.

- **Never expose private keys**: Use environment variables.
- **Keep messages secure and private**: Do not log messages in plaintext. Do not share messages with third parties.
- **Label agents clearly**: Clearly identify your agent as an agent and don't have an agent impersonate a human.
