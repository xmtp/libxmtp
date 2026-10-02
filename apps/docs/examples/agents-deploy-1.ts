import { Agent } from "@xmtp/agent-sdk";
// #region example1
const agent = await Agent.createFromEnv({
  storage: {
    location: { directory: process.env.RAILWAY_VOLUME_MOUNT_PATH ?? "." },
    label: process.env.XMTP_ENV,
  },
});
// #endregion example1
