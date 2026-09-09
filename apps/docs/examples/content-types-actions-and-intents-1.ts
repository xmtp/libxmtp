import { Agent } from "@xmtp/agent-sdk";
const agent = await Agent.createFromEnv();

// #region example1
agent.on("intent", async (ctx) => {
  console.log(`Selected action: ${ctx.message.content.actionId}`);
});
// #endregion example1
