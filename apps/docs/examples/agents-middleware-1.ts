import { Agent } from "@xmtp/agent-sdk";
const agent = await Agent.createFromEnv();

// #region example1
import type { AgentMiddleware } from "@xmtp/agent-sdk";

const ignoreSelf: AgentMiddleware = async (ctx, next) => {
  if (ctx.message.senderInboxId === ctx.client.inboxId) return;
  await next();
};

agent.use(ignoreSelf);
// #endregion example1
