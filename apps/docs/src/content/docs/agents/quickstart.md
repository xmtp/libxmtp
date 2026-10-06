---
title: Build an agent
---

Use the XMTP Agent SDK to build a Node.js agent that listens for messages and responds.

## Install

```bash
npm i @xmtp/agent-sdk
```

## Configure

Create a `.env` file. Load it into `process.env` before you call `Agent.createFromEnv()`. The SDK does not read the file itself:

```bash
XMTP_BACKEND_URL=https://your-backend.example.com
XMTP_WALLET_KEY=0x...
XMTP_DB_ENCRYPTION_KEY=0x...
XMTP_ENV=my-app
```

`XMTP_BACKEND_URL` is required and must include its scheme. `XMTP_ENV` is only a label for a directory under the storage root. The wallet key must use `0x` hex format. The database encryption key is 32 bytes.

## Start the agent

```ts source="agents-quickstart-1.ts" region="example1"

```

`Agent.createFromEnv()` creates the client and registers its installation. `agent.start()` starts message and conversation readers. The `start` event means that the local readers are ready. The backend can still be offline.

:::caution
Persist the local database across restarts and deployments. Losing it creates a new installation. The default backend limit is 10 installations per inbox.
:::

Use Node.js 22.12.0 or later. A minimal container image must include CA certificates for an HTTPS backend.
