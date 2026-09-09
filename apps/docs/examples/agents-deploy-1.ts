import { Agent } from "@xmtp/agent-sdk";

// #region example1
const customDbPath = (inboxId: string) =>
  `${process.env.RAILWAY_VOLUME_MOUNT_PATH ?? "."}/${process.env.XMTP_ENV}-${inboxId.slice(0, 8)}.db3`;

const agent = await Agent.createFromEnv({
  backendUrl: process.env.XMTP_BACKEND_URL,
  dbPath: customDbPath,
});
// #endregion example1
