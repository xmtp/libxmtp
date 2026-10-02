import { Agent } from "@xmtp/agent-sdk";

// #region example1
const agent = await Agent.createFromEnv({
  backend: { url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050" },
  storage: {
    location: { directory: process.env.RAILWAY_VOLUME_MOUNT_PATH ?? "." },
  },
});
// #endregion example1
