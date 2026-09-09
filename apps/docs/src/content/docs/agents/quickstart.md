---
title: Build an agent
---

Use the XMTP Agent SDK to build a Node.js agent that listens for messages and responds.

## Install

```bash
npm i @xmtp/agent-sdk
```

## Configure

Create a `.env` file:

```bash
XMTP_BACKEND_URL=https://your-backend.example.com
XMTP_WALLET_KEY=0x...
XMTP_DB_ENCRYPTION_KEY=0x...
XMTP_ENV=my-app
```

`XMTP_BACKEND_URL` is required and must include its scheme. `XMTP_ENV` is only a label for the default database file name. The wallet key must use `0x` hex format. The database encryption key is 32 bytes.

## Start the agent

```ts source="agents-quickstart-1.ts" region="example1"

```

An agent must start once before another client can find and message it.

:::caution
Persist the local database across restarts and deployments. Losing it creates a new installation. An inbox supports 10 installations.
:::

Use Node 22 or another supported Node.js LTS release. A minimal container image must include CA certificates for an HTTPS backend.
