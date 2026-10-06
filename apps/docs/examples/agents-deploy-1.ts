import { Agent } from "@xmtp/agent-sdk";

// #region example1
const agent = await Agent.createFromEnv({
  storage: {
    location: {
      directory: process.env.RAILWAY_VOLUME_MOUNT_PATH ?? "./agent-data",
    },
  },
});
// #endregion example1
