---
title: Deploy an agent
---

Deploy the agent on a Node.js host that supports environment variables and persistent storage.

## Required configuration

| Variable                 | Purpose                                                    |
| ------------------------ | ---------------------------------------------------------- |
| `XMTP_BACKEND_URL`       | Self-hosted backend URL, including scheme                  |
| `XMTP_WALLET_KEY`        | Agent wallet key in `0x` hex format                        |
| `XMTP_DB_ENCRYPTION_KEY` | 32-byte local database encryption key                      |
| `XMTP_DB_DIRECTORY`      | Persistent directory for Agent SDK databases               |
| `XMTP_ENV`               | Optional database file label; it does not select a backend |

`Agent.createFromEnv()` creates `XMTP_DB_DIRECTORY` with owner-only permissions and stores the database there. Mount this directory on a persistent volume.

Back up these SQLite files for persistent storage:

- `{env}-{description}.db3` - Main database
- `{env}-{description}.db3-shm` - Shared memory
- `{env}-{description}.db3-wal` - Write-ahead log
- `{env}-{description}.db3.sqlcipher_salt` - Encryption salt

Rough estimate: **1GB ≈ 15,000 conversations**. Plan based on your expected volume.

For a provider that supplies `RAILWAY_VOLUME_MOUNT_PATH`, use a database path callback:

```ts source="agents-deploy-1.ts" region="example1"

```

Use `pm2-runtime` when PM2 runs in a container. Set `unstable_restarts: 10000` so PM2 does not stop restarts during rapid crash cycles.

## Security

For an agent to function—whether it's answering questions, executing commands, or providing automated responses—it must be able to read the conversation to understand what's being asked, and write messages to respond.

Like any other user, this means your agent holds the cryptographic keys required to decrypt and send messages in the conversation. As an agent developer, it's important to uphold the security of these keys and messages.

- **Never expose private keys**: Use environment variables.
- **Keep messages secure and private**: Do not log messages in plaintext. Do not share messages with third parties.
- **Label agents clearly**: Clearly identify your agent as an agent and don't have an agent impersonate a human.
